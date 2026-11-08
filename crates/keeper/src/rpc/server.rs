use std::path::PathBuf;

use ome_api::{
    AuthParams, ErrorKind, Method, RpcError,
    jsonrpc::{Notification, Request, Response},
    keeper::{EventsSubscribe, SessionAuth},
};
use serde_json::Value;
use subtle::ConstantTimeEq;
use tokio::{
    io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader},
    sync::mpsc,
};
use zeroize::Zeroizing;

use super::methods;
use crate::{Error, Keeper, Result};

/// Largest accepted request line (an ECDH batch of 4096 points is about 600 KiB).
const MAX_LINE: usize = 8 << 20;

/// The token every RPC session must present first (`session.auth`). Compared in constant time.
#[derive(Clone)]
pub struct RpcToken(Zeroizing<Vec<u8>>);

impl RpcToken {
    /// Wraps a token.
    #[must_use]
    pub fn new(token: &str) -> Self {
        Self(Zeroizing::new(token.as_bytes().to_vec()))
    }

    fn matches(&self, candidate: &str) -> bool {
        let candidate = candidate.as_bytes();
        candidate.len() == self.0.len() && bool::from(candidate.ct_eq(&self.0))
    }
}

/// Serves JSON-RPC 2.0 (newline-delimited) on a Unix domain socket at `path`, readable only by the owner.
///
/// # Errors
/// Bind failures.
#[cfg(unix)]
pub async fn serve(keeper: Keeper, path: PathBuf, token: RpcToken) -> Result<()> {
    use std::os::unix::fs::{FileTypeExt, PermissionsExt};

    let path = path.as_path();
    if let Ok(metadata) = std::fs::symlink_metadata(path) {
        if metadata.file_type().is_socket() {
            std::fs::remove_file(path)?;
        } else {
            return Err(Error::DataDir(format!(
                "{} exists and is not a socket",
                path.display()
            )));
        }
    }
    let listener = tokio::net::UnixListener::bind(path)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    tracing::info!(socket = %path.display(), "rpc listening");
    loop {
        let (stream, _) = listener.accept().await?;
        let keeper = keeper.clone();
        let token = token.clone();
        tokio::spawn(async move {
            let (reader, writer) = stream.into_split();
            connection(keeper, token, reader, writer).await;
        });
    }
}

/// Serves JSON-RPC 2.0 on the named pipe `path` (e.g. `\\.\pipe\ome-keeper`), refusing remote clients.
///
/// # Errors
/// Pipe creation failures.
#[cfg(windows)]
pub async fn serve(keeper: Keeper, path: PathBuf, token: RpcToken) -> Result<()> {
    use tokio::net::windows::named_pipe::ServerOptions;

    let name = path.as_os_str();
    let mut server = ServerOptions::new()
        .first_pipe_instance(true)
        .reject_remote_clients(true)
        .create(name)?;
    loop {
        server.connect().await?;
        let connected = server;
        server = ServerOptions::new()
            .reject_remote_clients(true)
            .create(name)?;
        let keeper = keeper.clone();
        let token = token.clone();
        tokio::spawn(async move {
            let (reader, writer) = tokio::io::split(connected);
            connection(keeper, token, reader, writer).await;
        });
    }
}

async fn connection<R, W>(keeper: Keeper, token: RpcToken, reader: R, mut writer: W)
where
    R: AsyncRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static,
{
    let (out, mut outgoing) = mpsc::channel::<Value>(256);
    let writing = tokio::spawn(async move {
        while let Some(value) = outgoing.recv().await {
            let mut line = value.to_string();
            line.push('\n');
            if writer.write_all(line.as_bytes()).await.is_err() {
                break;
            }
        }
    });

    let mut reader = BufReader::new(reader);
    let mut authenticated = false;
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
        let Ok(request) = serde_json::from_slice::<Request>(&line) else {
            let error = RpcError::new(ErrorKind::Parse, "parse error");
            let _ = out.send(reply(Response::err(Value::Null, error))).await;
            continue;
        };
        let id = request.id.clone();
        let response = if request.method == SessionAuth::NAME {
            authenticated = request
                .params::<AuthParams>()
                .is_ok_and(|p| token.matches(&p.token));
            if authenticated {
                Response::ok(id, Value::Null)
            } else {
                Response::err(id, RpcError::new(ErrorKind::Unauthorized, "invalid token"))
            }
        } else if !authenticated {
            let error = RpcError::new(
                ErrorKind::Unauthorized,
                "authenticate with session.auth first",
            );
            Response::err(id, error)
        } else if request.jsonrpc != "2.0" || request.method.is_empty() {
            let error = RpcError::new(ErrorKind::InvalidRequest, "not a JSON-RPC 2.0 request");
            Response::err(id, error)
        } else if request.method == EventsSubscribe::NAME {
            let mut events = keeper.subscribe();
            let out = out.clone();
            tokio::spawn(async move {
                while let Ok(event) = events.recv().await {
                    if out.send(reply(Notification::event(event))).await.is_err() {
                        break;
                    }
                }
            });
            Response::ok(id, Value::Null)
        } else {
            // Methods run concurrently so a slow ECDH batch does not block the session.
            let keeper = keeper.clone();
            let out = out.clone();
            tokio::spawn(async move {
                let response = match methods::call(&keeper, &request).await {
                    Ok(result) => Response::ok(id, result),
                    Err(error) => Response::err(id, error),
                };
                let _ = out.send(reply(response)).await;
            });
            continue;
        };
        if out.send(reply(response)).await.is_err() {
            break;
        }
    }
    drop(out);
    let _ = writing.await;
}

fn reply(message: impl serde::Serialize) -> Value {
    serde_json::to_value(message).unwrap_or(Value::Null)
}
