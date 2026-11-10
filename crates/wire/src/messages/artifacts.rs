//! Contract artifacts between members' keepers. An artifact (the gzip-compressed JSON) travels in chunks under one
//! message limit, each named by its hash in a manifest. A keeper checks chunks against the manifest it fetched; the
//! daemon checks the reassembled artifact against the contract class, so neither trusts the member who sent it.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::bounded;
use crate::{
    Error,
    limits::{MAX_ARTIFACT_CHUNK_BYTES, MAX_ARTIFACT_CHUNKS, MAX_ARTIFACT_REQUEST},
};

/// Domain of [`chunk_hash`].
pub const CHUNK_DOMAIN: &[u8] = b"ome.artifact.chunk.v0";

/// `SHA-256("ome.artifact.chunk.v0" ‖ bytes)`.
#[must_use]
pub fn chunk_hash(bytes: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(CHUNK_DOMAIN);
    hasher.update(bytes);
    hasher.finalize().into()
}

/// Asks a member's keeper for an artifact: its manifest when `chunks` is empty, else those chunks.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactRequest {
    /// The contract class.
    pub class_id: [u8; 32],
    /// Chunk indexes; empty for the manifest.
    pub chunks: Vec<u32>,
}

impl ArtifactRequest {
    pub(super) fn validate(&self) -> Result<(), Error> {
        bounded(
            "artifact chunks",
            self.chunks.len(),
            0,
            MAX_ARTIFACT_REQUEST,
        )
    }
}

/// What an artifact is made of: its chunks' hashes, in order.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactManifest {
    /// The contract class.
    pub class_id: [u8; 32],
    /// [`chunk_hash`] of each chunk.
    pub chunks: Vec<[u8; 32]>,
}

impl ArtifactManifest {
    pub(super) fn validate(&self) -> Result<(), Error> {
        bounded(
            "artifact manifest",
            self.chunks.len(),
            1,
            MAX_ARTIFACT_CHUNKS,
        )
    }
}

/// One chunk of an artifact.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactChunk {
    /// The contract class.
    pub class_id: [u8; 32],
    /// Its index in the manifest.
    pub index: u32,
    /// The bytes.
    pub bytes: Vec<u8>,
}

impl ArtifactChunk {
    pub(super) fn validate(&self) -> Result<(), Error> {
        bounded(
            "artifact chunk",
            self.bytes.len(),
            1,
            MAX_ARTIFACT_CHUNK_BYTES,
        )?;
        let index = usize::try_from(self.index).unwrap_or(usize::MAX);
        bounded("artifact chunk index", index, 0, MAX_ARTIFACT_CHUNKS - 1)
    }
}
