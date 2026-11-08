//! Store-and-forward for members who are offline. A keeper seals what a peer could not receive directly into their
//! pairwise mailbox (`ome_crypto::sealing::mailbox`) at one or more untrusted relays, which see only opaque mailbox
//! ids, expiry times and ciphertext.

use std::{
    collections::HashSet,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex, PoisonError},
};

use ome_wire::{
    limits::{MAX_DELETE_IDS, MAX_FETCH_MAILBOXES},
    messages::mailbox::{
        MailboxRequest, MailboxResponse, RelayEnvelope, StoredEnvelope, decode_response,
        encode_request,
    },
};

use crate::{Error, Libp2pTransport};

/// Boxed mailbox future.
pub type MailboxFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, Error>> + Send + 'a>>;

/// Where mailboxes live: relays, or memory for tests and embedded keepers.
pub trait Mailbox: Send + Sync + 'static {
    /// Stores an envelope; succeeds if any relay stored it.
    fn deposit(&self, envelope: RelayEnvelope) -> MailboxFuture<'_, ()>;
    /// Envelopes held for `mailboxes`, without duplicates.
    fn fetch(&self, mailboxes: Vec<[u8; 32]>) -> MailboxFuture<'_, Vec<StoredEnvelope>>;
    /// Deletes delivered envelopes everywhere.
    fn delete(&self, ids: Vec<[u8; 32]>) -> MailboxFuture<'_, ()>;
}

/// Mailboxes at a node's configured relays: deposits go to every relay, fetches merge every relay's answer, deletes go
/// everywhere. Requests larger than the wire limits are split.
pub struct RelayMailbox {
    transport: Arc<Libp2pTransport>,
}

impl RelayMailbox {
    /// Mailboxes at `transport`'s relays.
    #[must_use]
    pub fn new(transport: Arc<Libp2pTransport>) -> Self {
        Self { transport }
    }

    /// Every relay's answer to `request`.
    async fn ask(&self, request: &MailboxRequest) -> Vec<Result<MailboxResponse, Error>> {
        let bytes = match encode_request(request) {
            Ok(bytes) => bytes,
            Err(error) => return vec![Err(error.into())],
        };
        let answers = self.transport.relays().iter().map(|relay| {
            let bytes = bytes.clone();
            async move {
                Ok(decode_response(
                    &self.transport.mailbox(*relay, bytes).await?,
                )?)
            }
        });
        futures::future::join_all(answers).await
    }
}

impl Mailbox for RelayMailbox {
    fn deposit(&self, envelope: RelayEnvelope) -> MailboxFuture<'_, ()> {
        Box::pin(async move {
            let answers = self.ask(&MailboxRequest::Deposit(envelope)).await;
            if answers
                .iter()
                .any(|a| matches!(a, Ok(MailboxResponse::Stored { .. })))
            {
                Ok(())
            } else {
                tracing::debug!(?answers, "no relay stored the envelope");
                Err(Error::NotStored)
            }
        })
    }

    fn fetch(&self, mailboxes: Vec<[u8; 32]>) -> MailboxFuture<'_, Vec<StoredEnvelope>> {
        Box::pin(async move {
            let mut seen = HashSet::new();
            let mut envelopes = Vec::new();
            for chunk in mailboxes.chunks(MAX_FETCH_MAILBOXES) {
                for answer in self
                    .ask(&MailboxRequest::Fetch {
                        mailboxes: chunk.to_vec(),
                    })
                    .await
                {
                    match answer {
                        Ok(MailboxResponse::Envelopes(found)) => {
                            envelopes.extend(found.into_iter().filter(|e| seen.insert(e.id)));
                        }
                        Ok(other) => tracing::debug!(?other, "unexpected mailbox answer"),
                        Err(error) => tracing::debug!(%error, "mailbox fetch failed"),
                    }
                }
            }
            Ok(envelopes)
        })
    }

    fn delete(&self, ids: Vec<[u8; 32]>) -> MailboxFuture<'_, ()> {
        Box::pin(async move {
            for chunk in ids.chunks(MAX_DELETE_IDS) {
                self.ask(&MailboxRequest::Delete {
                    ids: chunk.to_vec(),
                })
                .await;
            }
            Ok(())
        })
    }
}

/// An in-process mailbox, shared by keepers on a `MemoryNetwork`.
#[derive(Default)]
pub struct MemoryMailbox {
    envelopes: Mutex<Vec<StoredEnvelope>>,
}

impl MemoryMailbox {
    /// Number of envelopes held.
    #[must_use]
    pub fn len(&self) -> usize {
        self.envelopes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .len()
    }

    /// Whether no envelope is held.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Mailbox for MemoryMailbox {
    fn deposit(&self, envelope: RelayEnvelope) -> MailboxFuture<'_, ()> {
        let stored = envelope.id().map(|id| StoredEnvelope { id, envelope });
        Box::pin(async move {
            let stored = stored?;
            let mut envelopes = self
                .envelopes
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            if envelopes.iter().all(|e| e.id != stored.id) {
                envelopes.push(stored);
            }
            Ok(())
        })
    }

    fn fetch(&self, mailboxes: Vec<[u8; 32]>) -> MailboxFuture<'_, Vec<StoredEnvelope>> {
        let found = self
            .envelopes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .filter(|e| mailboxes.contains(&e.envelope.recipient))
            .cloned()
            .collect();
        Box::pin(async move { Ok(found) })
    }

    fn delete(&self, ids: Vec<[u8; 32]>) -> MailboxFuture<'_, ()> {
        self.envelopes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .retain(|e| !ids.contains(&e.id));
        Box::pin(async { Ok(()) })
    }
}
