//! One authenticated connection: requests get increasing ids and are answered in any order, so calls run concurrently;
//! notifications go to every event stream.

use std::{
    collections::HashMap,
    marker::PhantomData,
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

use ome_api::{
    AuthParams, Method,
    jsonrpc::{EVENT, Request, Response},
};
use serde::de::DeserializeOwned;
use serde_json::Value;
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    sync::{Mutex as AsyncMutex, broadcast, oneshot},
    task::JoinHandle,
};

use crate::{
    ClientError, Result,
    transport::{self, Reader, Writer},
};

/// Largest accepted reply line, the services' own request limit.
const MAX_LINE: usize = 8 << 20;

/// Events buffered per stream before the slowest one starts missing them.
const EVENT_BUFFER: usize = 1024;

type Pending = Mutex<HashMap<u64, oneshot::Sender<Result<Value>>>>;

/// What the reader task shares with callers.
#[derive(Default)]
struct Shared {
    pending: Pending,
    closed: AtomicBool,
}

impl Shared {
    fn pending(&self) -> std::sync::MutexGuard<'_, HashMap<u64, oneshot::Sender<Result<Value>>>> {
        self.pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// An authenticated session with a keeper or daemon.
pub(crate) struct Session {
    writer: AsyncMutex<Writer>,
    shared: Arc<Shared>,
    next: AtomicU64,
    events: broadcast::Receiver<Value>,
    subscribed: AtomicBool,
    reader: JoinHandle<()>,
}

impl Drop for Session {
    fn drop(&mut self) {
        self.reader.abort();
    }
}

impl Session {
    /// Connects to `path` and presents `token`.
    pub(crate) async fn open(path: &Path, token: &str) -> Result<Self> {
        let (reader, writer) =
            transport::connect(path)
                .await
                .map_err(|source| ClientError::Connect {
                    path: path.to_owned(),
                    source,
                })?;
        let shared = Arc::new(Shared::default());
        let (events, receiver) = broadcast::channel(EVENT_BUFFER);
        let session = Self {
            writer: AsyncMutex::new(writer),
            shared: Arc::clone(&shared),
            next: AtomicU64::new(1),
            events: receiver,
            subscribed: AtomicBool::new(false),
            reader: tokio::spawn(read(reader, shared, events)),
        };
        // Both services name and shape `session.auth` and `events.subscribe` alike.
        session
            .call::<ome_api::keeper::SessionAuth>(&AuthParams {
                token: token.to_owned(),
            })
            .await?;
        Ok(session)
    }

    /// Calls `M`.
    pub(crate) async fn call<M: Method>(&self, params: &M::Params) -> Result<M::Result> {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let request = Request::new::<M>(id, params)
            .map_err(|e| ClientError::Protocol(format!("params: {e}")))?;
        let mut line =
            serde_json::to_vec(&request).map_err(|e| ClientError::Protocol(e.to_string()))?;
        line.push(b'\n');

        let (answer, answered) = oneshot::channel();
        self.shared.pending().insert(id, answer);
        if self.shared.closed.load(Ordering::Acquire) {
            self.shared.pending().remove(&id);
            return Err(ClientError::Closed);
        }
        {
            let mut writer = self.writer.lock().await;
            let written = async {
                writer.write_all(&line).await?;
                writer.flush().await
            }
            .await;
            if let Err(error) = written {
                self.shared.pending().remove(&id);
                return Err(error.into());
            }
        }
        let result = answered.await.map_err(|_| ClientError::Closed)??;
        serde_json::from_value(result)
            .map_err(|e| ClientError::Protocol(format!("{} result: {e}", M::NAME)))
    }

    /// A stream of the service's events. Asks the service to send them (method `M`) on first use.
    pub(crate) async fn subscribe<M: Method, E>(&self, params: &M::Params) -> Result<Events<E>> {
        let receiver = self.events.resubscribe();
        if !self.subscribed.swap(true, Ordering::AcqRel)
            && let Err(error) = self.call::<M>(params).await
        {
            self.subscribed.store(false, Ordering::Release);
            return Err(error);
        }
        Ok(Events {
            receiver,
            event: PhantomData,
        })
    }

    /// Like [`Session::subscribe`], but always asks, and returns the service's answer: a subscription that replays
    /// missed events sends them right after it, so the stream is opened before asking.
    pub(crate) async fn subscribe_answered<M: Method, E>(
        &self,
        params: &M::Params,
    ) -> Result<(M::Result, Events<E>)> {
        let receiver = self.events.resubscribe();
        let answer = self.call::<M>(params).await?;
        self.subscribed.store(true, Ordering::Release);
        Ok((
            answer,
            Events {
                receiver,
                event: PhantomData,
            },
        ))
    }
}

/// Reads replies and notifications until the connection closes, then fails every call still waiting.
async fn read(reader: Reader, shared: Arc<Shared>, events: broadcast::Sender<Value>) {
    let mut reader = BufReader::new(reader);
    loop {
        let mut line = Vec::new();
        match (&mut reader)
            .take(MAX_LINE as u64 + 1)
            .read_until(b'\n', &mut line)
            .await
        {
            Ok(0) | Err(_) => break,
            Ok(_) if line.len() > MAX_LINE => break,
            Ok(_) => {}
        }
        let Ok(value) = serde_json::from_slice::<Value>(&line) else {
            break;
        };
        if value.get("method").and_then(Value::as_str) == Some(EVENT) {
            if let Some(params) = value.get("params") {
                let _ = events.send(params.clone());
            }
            continue;
        }
        let Ok(response) = serde_json::from_value::<Response>(value) else {
            break;
        };
        let Some(id) = response.id.as_u64() else {
            continue;
        };
        let Some(answer) = shared.pending().remove(&id) else {
            continue;
        };
        let _ = answer.send(match response.error {
            Some(error) => Err(ClientError::Rpc(error)),
            None => Ok(response.result.unwrap_or(Value::Null)),
        });
    }
    shared.closed.store(true, Ordering::Release);
    for (_, answer) in shared.pending().drain() {
        let _ = answer.send(Err(ClientError::Closed));
    }
}

/// A service's events, in the order it sent them.
pub struct Events<E> {
    receiver: broadcast::Receiver<Value>,
    event: PhantomData<fn() -> E>,
}

impl<E: DeserializeOwned> Events<E> {
    /// The next event; `None` once the connection closed.
    pub async fn next(&mut self) -> Option<Result<E>> {
        match self.receiver.recv().await {
            Ok(value) => Some(
                serde_json::from_value(value)
                    .map_err(|e| ClientError::Protocol(format!("event: {e}"))),
            ),
            Err(broadcast::error::RecvError::Lagged(missed)) => {
                Some(Err(ClientError::Lagged(missed)))
            }
            Err(broadcast::error::RecvError::Closed) => None,
        }
    }
}
