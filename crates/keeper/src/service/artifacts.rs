//! Contract artifacts between members. The daemon puts the artifacts its proposals call; a member's keeper fetches one
//! from the vault's members when its daemon asks: the manifest first, then the chunks in batches, each checked against
//! the manifest's hash. Whether the artifact is the code of its class is the daemon's check, after reassembly.

use std::collections::BTreeSet;

use ome_api::{Hex32, keeper::KeeperEvent};
use ome_net::Peer;
use ome_wire::{
    Header, IdentityPublic, Message,
    limits::{MAX_ARTIFACT_CHUNK_BYTES, MAX_ARTIFACT_CHUNKS, MAX_ARTIFACT_REQUEST},
    messages::artifacts::{ArtifactChunk, ArtifactManifest, ArtifactRequest, chunk_hash},
};

use crate::{Error, Result, keeper::Keeper, store::ManifestStatus};

/// Bytes of artifact chunks a keeper stores at most.
const MAX_STORED_BYTES: u64 = 512 << 20;
/// A fetch that heard nothing for this long may start again.
const FETCH_STALE_SECS: u64 = 60;

/// An artifact being fetched: from which vault's members, and the chunks asked for that haven't arrived.
pub(crate) struct ArtifactFetch {
    vault: [u8; 32],
    epoch: u32,
    peer: Option<Peer>,
    waiting: BTreeSet<u32>,
    heard_at: u64,
}

/// How much of an artifact is stored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArtifactProgress {
    /// Every chunk is here.
    pub complete: bool,
    /// Chunks held.
    pub have: u32,
    /// Chunks in the artifact, once known.
    pub count: Option<u32>,
}

impl Keeper {
    /// Stores one chunk of an artifact this keeper's daemon holds. When the last chunk arrives the artifact is
    /// complete and its manifest is the chunks' hashes.
    ///
    /// # Errors
    /// [`Error::Invalid`] for a chunk out of bounds, a count that differs from the stored one, or a full store.
    pub fn put_artifact(
        &self,
        class_id: [u8; 32],
        index: u32,
        count: u32,
        bytes: &[u8],
    ) -> Result<ArtifactProgress> {
        let chunks = usize::try_from(count).unwrap_or(usize::MAX);
        if !(1..=MAX_ARTIFACT_CHUNKS).contains(&chunks) || index >= count {
            return Err(Error::Invalid(format!(
                "artifact chunk {index} of {count} out of bounds"
            )));
        }
        if bytes.is_empty() || bytes.len() > MAX_ARTIFACT_CHUNK_BYTES {
            return Err(Error::Invalid("artifact chunk size out of bounds".into()));
        }
        let store = &self.0.store;
        match store.artifact(&class_id)? {
            Some(record) if record.complete => return self.artifact_progress(class_id),
            Some(record) if record.count != count => {
                return Err(Error::Invalid(
                    "artifact chunk count differs from the stored one".into(),
                ));
            }
            Some(_) => {}
            None => {
                self.make_room(bytes.len())?;
                store.begin_artifact(&class_id, count, None, self.now())?;
            }
        }
        store.put_artifact_chunk(&class_id, index, bytes)?;
        if store.artifact_indexes(&class_id)?.len() == chunks {
            let hashes = (0..count)
                .map(|i| {
                    store
                        .artifact_chunk(&class_id, i)?
                        .map(|chunk| chunk_hash(&chunk))
                        .ok_or_else(|| Error::Storage("artifact chunk vanished".into()))
                })
                .collect::<Result<Vec<_>>>()?;
            store.complete_artifact(&class_id, &hashes)?;
        }
        self.artifact_progress(class_id)
    }

    /// One chunk of a complete artifact, and how many it has.
    ///
    /// # Errors
    /// Storage errors.
    pub fn artifact_chunk(&self, class_id: [u8; 32], index: u32) -> Result<Option<(u32, Vec<u8>)>> {
        let Some(record) = self.0.store.artifact(&class_id)?.filter(|r| r.complete) else {
            return Ok(None);
        };
        Ok(self
            .0
            .store
            .artifact_chunk(&class_id, index)?
            .map(|bytes| (record.count, bytes)))
    }

    /// How much of an artifact is stored.
    ///
    /// # Errors
    /// Storage errors.
    pub fn artifact_progress(&self, class_id: [u8; 32]) -> Result<ArtifactProgress> {
        let record = self.0.store.artifact(&class_id)?;
        let have =
            u32::try_from(self.0.store.artifact_indexes(&class_id)?.len()).unwrap_or(u32::MAX);
        Ok(ArtifactProgress {
            complete: record.as_ref().is_some_and(|r| r.complete),
            have,
            count: record.map(|r| r.count),
        })
    }

    /// Asks `vault`'s other members for an artifact unless it is stored or already on its way. An `artifact` event
    /// reports when it is complete.
    ///
    /// # Errors
    /// [`Error::UnknownVault`], storage errors.
    pub async fn fetch_artifact(
        &self,
        vault: [u8; 32],
        class_id: [u8; 32],
    ) -> Result<ArtifactProgress> {
        let progress = self.artifact_progress(class_id)?;
        if progress.complete {
            return Ok(progress);
        }
        let now = self.now();
        let (peers, epoch) = {
            let mut state = self.0.state.lock().await;
            if state
                .artifact_fetches
                .get(&class_id)
                .is_some_and(|f| now.saturating_sub(f.heard_at) < FETCH_STALE_SECS)
            {
                return Ok(progress);
            }
            let v = state
                .vaults
                .get(&vault)
                .filter(|v| v.status == ManifestStatus::Active)
                .ok_or(Error::UnknownVault)?;
            let peers = v.committee.peers_except(&self.identity());
            let epoch = v.committee.epoch();
            state.artifact_fetches.insert(
                class_id,
                ArtifactFetch {
                    vault,
                    epoch,
                    peer: None,
                    waiting: BTreeSet::new(),
                    heard_at: now,
                },
            );
            (peers, epoch)
        };
        let (frame, _) = self.sign(
            vault,
            epoch,
            Message::ArtifactRequest(ArtifactRequest {
                class_id,
                chunks: Vec::new(),
            }),
        )?;
        for peer in peers {
            let keeper = self.clone();
            let frame = frame.clone();
            tokio::spawn(async move { keeper.send(peer, frame).await });
        }
        Ok(progress)
    }

    /// Answers a member: the manifest of a complete artifact, or the chunks asked for that are stored.
    pub(crate) async fn on_artifact_request(
        &self,
        from: IdentityPublic,
        header: Header,
        request: ArtifactRequest,
    ) {
        let Some(peer) = self.member_peer(header.scope, &from).await else {
            return;
        };
        let Ok(Some(record)) = self.0.store.artifact(&request.class_id) else {
            return;
        };
        let replies: Vec<Message> = if request.chunks.is_empty() {
            match record.hashes.filter(|_| record.complete) {
                Some(chunks) => vec![Message::ArtifactManifest(ArtifactManifest {
                    class_id: request.class_id,
                    chunks,
                })],
                None => Vec::new(),
            }
        } else {
            request
                .chunks
                .iter()
                .filter_map(|&index| {
                    let bytes = self
                        .0
                        .store
                        .artifact_chunk(&request.class_id, index)
                        .ok()??;
                    Some(Message::ArtifactChunk(ArtifactChunk {
                        class_id: request.class_id,
                        index,
                        bytes,
                    }))
                })
                .collect()
        };
        for reply in replies {
            if let Ok((frame, _)) = self.sign(header.scope, header.epoch, reply) {
                self.send(peer.clone(), frame).await;
            }
        }
    }

    /// Adopts the first manifest a member sends for an artifact being fetched, and asks it for the chunks.
    pub(crate) async fn on_artifact_manifest(
        &self,
        from: IdentityPublic,
        header: Header,
        manifest: ArtifactManifest,
    ) {
        let Some(peer) = self.member_peer(header.scope, &from).await else {
            return;
        };
        {
            let mut state = self.0.state.lock().await;
            let Some(fetch) = state
                .artifact_fetches
                .get_mut(&manifest.class_id)
                .filter(|f| f.vault == header.scope && f.peer.is_none())
            else {
                return;
            };
            fetch.peer = Some(peer);
            fetch.heard_at = self.now();
        }
        let count = u32::try_from(manifest.chunks.len()).unwrap_or(u32::MAX);
        let stored = self.make_room(0).and_then(|()| {
            self.0.store.begin_artifact(
                &manifest.class_id,
                count,
                Some(&manifest.chunks),
                self.now(),
            )
        });
        if let Err(error) = stored {
            self.give_up(manifest.class_id, &error.to_string()).await;
            return;
        }
        self.request_chunks(manifest.class_id).await;
    }

    /// Stores a chunk that matches the manifest; completes the artifact, or asks for the next batch.
    pub(crate) async fn on_artifact_chunk(
        &self,
        from: IdentityPublic,
        header: Header,
        chunk: ArtifactChunk,
    ) {
        if self.member_peer(header.scope, &from).await.is_none() {
            return;
        }
        let Ok(Some(record)) = self.0.store.artifact(&chunk.class_id) else {
            return;
        };
        let expected = record
            .hashes
            .as_ref()
            .and_then(|hashes| hashes.get(usize::try_from(chunk.index).ok()?));
        if record.complete || expected != Some(&chunk_hash(&chunk.bytes)) {
            return;
        }
        let batch_done = {
            let mut state = self.0.state.lock().await;
            let Some(fetch) = state.artifact_fetches.get_mut(&chunk.class_id) else {
                return;
            };
            fetch.waiting.remove(&chunk.index);
            fetch.heard_at = self.now();
            fetch.waiting.is_empty()
        };
        if let Err(error) =
            self.0
                .store
                .put_artifact_chunk(&chunk.class_id, chunk.index, &chunk.bytes)
        {
            self.give_up(chunk.class_id, &error.to_string()).await;
            return;
        }
        let have = self
            .0
            .store
            .artifact_indexes(&chunk.class_id)
            .map_or(0, |indexes| indexes.len());
        if have == record.hashes.as_ref().map_or(usize::MAX, Vec::len) {
            self.finish(chunk.class_id, record.hashes.unwrap_or_default())
                .await;
        } else if batch_done {
            self.request_chunks(chunk.class_id).await;
        }
    }

    /// Asks the member that sent the manifest for the next chunks that are missing.
    async fn request_chunks(&self, class_id: [u8; 32]) {
        let Ok(Some(record)) = self.0.store.artifact(&class_id) else {
            return;
        };
        let have: BTreeSet<u32> = self
            .0
            .store
            .artifact_indexes(&class_id)
            .unwrap_or_default()
            .into_iter()
            .collect();
        let missing: Vec<u32> = (0..record.count)
            .filter(|i| !have.contains(i))
            .take(MAX_ARTIFACT_REQUEST)
            .collect();
        let (peer, vault, epoch) = {
            let mut state = self.0.state.lock().await;
            let Some(fetch) = state.artifact_fetches.get_mut(&class_id) else {
                return;
            };
            let Some(peer) = fetch.peer.clone() else {
                return;
            };
            fetch.waiting = missing.iter().copied().collect();
            (peer, fetch.vault, fetch.epoch)
        };
        let request = Message::ArtifactRequest(ArtifactRequest {
            class_id,
            chunks: missing,
        });
        if let Ok((frame, _)) = self.sign(vault, epoch, request) {
            self.send(peer, frame).await;
        }
    }

    async fn finish(&self, class_id: [u8; 32], hashes: Vec<[u8; 32]>) {
        let Some(fetch) = self.0.state.lock().await.artifact_fetches.remove(&class_id) else {
            return;
        };
        match self.0.store.complete_artifact(&class_id, &hashes) {
            Ok(()) => self.emit(KeeperEvent::Artifact {
                vault: Hex32(fetch.vault),
                class_id: Hex32(class_id),
                complete: true,
                error: None,
            }),
            Err(error) => self.give_up(class_id, &error.to_string()).await,
        }
    }

    async fn give_up(&self, class_id: [u8; 32], reason: &str) {
        let Some(fetch) = self.0.state.lock().await.artifact_fetches.remove(&class_id) else {
            return;
        };
        let _ = self.0.store.remove_artifact(&class_id);
        self.emit(KeeperEvent::Artifact {
            vault: Hex32(fetch.vault),
            class_id: Hex32(class_id),
            complete: false,
            error: Some(reason.to_owned()),
        });
    }

    /// Refuses a new artifact when the store already holds [`MAX_STORED_BYTES`].
    fn make_room(&self, adding: usize) -> Result<()> {
        let adding = u64::try_from(adding).unwrap_or(u64::MAX);
        if self.0.store.artifact_bytes()?.saturating_add(adding) > MAX_STORED_BYTES {
            return Err(Error::Invalid(
                "this keeper's artifact store is full".into(),
            ));
        }
        Ok(())
    }
}
