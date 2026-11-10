//! Contract artifacts, stored by class and handed between members' keepers in chunks. The keeper only checks that
//! each chunk matches the manifest it fetched; the daemon checks the whole artifact against the class id.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{Hex32, HexBytes};

/// Params of `artifacts.put`: one chunk of an artifact this member's daemon holds.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactPutParams {
    /// The contract class.
    pub class_id: Hex32,
    /// Chunk index, from 0.
    pub index: u32,
    /// Chunks in the artifact.
    pub count: u32,
    /// The chunk: a slice of the gzip-compressed artifact JSON.
    pub bytes: HexBytes,
}

/// Params of `artifacts.get`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactChunkParams {
    /// The contract class.
    pub class_id: Hex32,
    /// Chunk index, from 0.
    pub index: u32,
}

/// One stored chunk.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ArtifactChunk {
    /// Chunks in the artifact.
    pub count: u32,
    /// The chunk.
    pub bytes: HexBytes,
}

/// Params of `artifacts.fetch` and `artifacts.status`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactParams {
    /// The vault whose members are asked for it.
    pub vault: Hex32,
    /// The contract class.
    pub class_id: Hex32,
}

/// How much of an artifact this keeper holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ArtifactStatus {
    /// Every chunk is here.
    pub complete: bool,
    /// Chunks held.
    pub have: u32,
    /// Chunks in the artifact, once known.
    pub count: Option<u32>,
}
