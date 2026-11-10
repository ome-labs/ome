//! Contract artifacts by class: a row with the chunk count and, once known, the chunks' hashes; one row per chunk.
//! An artifact this keeper's daemon put learns its hashes when its last chunk arrives; one fetched from a member
//! starts from the member's manifest.

use rusqlite::{OptionalExtension, params};

use super::{Store, ts};
use crate::Result;

/// What is stored of an artifact.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArtifactRecord {
    /// Chunks in the artifact.
    pub count: u32,
    /// The chunks' hashes, once known.
    pub hashes: Option<Vec<[u8; 32]>>,
    /// Every chunk is here and matches its hash.
    pub complete: bool,
}

impl Store {
    /// The artifact of `class_id`, if any of it is stored.
    ///
    /// # Errors
    /// Storage errors.
    pub fn artifact(&self, class_id: &[u8; 32]) -> Result<Option<ArtifactRecord>> {
        self.with(|c| {
            c.query_row(
                "SELECT count, hashes, complete FROM artifacts WHERE class_id = ?1",
                params![class_id],
                |row| {
                    let hashes: Option<Vec<u8>> = row.get(1)?;
                    Ok(ArtifactRecord {
                        count: row.get(0)?,
                        hashes: hashes.map(|bytes| split_hashes(&bytes)),
                        complete: row.get(2)?,
                    })
                },
            )
            .optional()
        })
    }

    /// Records an artifact of `count` chunks, with their hashes when known. Keeps an existing record.
    ///
    /// # Errors
    /// Storage errors.
    pub fn begin_artifact(
        &self,
        class_id: &[u8; 32],
        count: u32,
        hashes: Option<&[[u8; 32]]>,
        now: u64,
    ) -> Result<()> {
        self.with(|c| {
            c.execute(
                "INSERT OR IGNORE INTO artifacts (class_id, count, hashes, complete, created_at) \
                 VALUES (?1, ?2, ?3, 0, ?4)",
                params![class_id, count, hashes.map(join_hashes), ts(now)],
            )
            .map(drop)
        })
    }

    /// Stores a chunk; `false` if it was already stored.
    ///
    /// # Errors
    /// Storage errors.
    pub fn put_artifact_chunk(
        &self,
        class_id: &[u8; 32],
        index: u32,
        bytes: &[u8],
    ) -> Result<bool> {
        self.with(|c| {
            c.execute(
                "INSERT OR IGNORE INTO artifact_chunks (class_id, idx, bytes) VALUES (?1, ?2, ?3)",
                params![class_id, index, bytes],
            )
            .map(|changed| changed > 0)
        })
    }

    /// One chunk.
    ///
    /// # Errors
    /// Storage errors.
    pub fn artifact_chunk(&self, class_id: &[u8; 32], index: u32) -> Result<Option<Vec<u8>>> {
        self.with(|c| {
            c.query_row(
                "SELECT bytes FROM artifact_chunks WHERE class_id = ?1 AND idx = ?2",
                params![class_id, index],
                |row| row.get(0),
            )
            .optional()
        })
    }

    /// The indexes of the chunks stored, ascending.
    ///
    /// # Errors
    /// Storage errors.
    pub fn artifact_indexes(&self, class_id: &[u8; 32]) -> Result<Vec<u32>> {
        self.with(|c| {
            let mut statement =
                c.prepare("SELECT idx FROM artifact_chunks WHERE class_id = ?1 ORDER BY idx")?;
            let rows = statement.query_map(params![class_id], |row| row.get(0))?;
            rows.collect()
        })
    }

    /// Marks an artifact complete, with its chunks' hashes.
    ///
    /// # Errors
    /// Storage errors.
    pub fn complete_artifact(&self, class_id: &[u8; 32], hashes: &[[u8; 32]]) -> Result<()> {
        self.with(|c| {
            c.execute(
                "UPDATE artifacts SET complete = 1, hashes = ?2 WHERE class_id = ?1",
                params![class_id, join_hashes(hashes)],
            )
            .map(drop)
        })
    }

    /// Forgets an artifact and its chunks.
    ///
    /// # Errors
    /// Storage errors.
    pub fn remove_artifact(&self, class_id: &[u8; 32]) -> Result<()> {
        self.with(|c| {
            c.execute(
                "DELETE FROM artifact_chunks WHERE class_id = ?1",
                params![class_id],
            )?;
            c.execute(
                "DELETE FROM artifacts WHERE class_id = ?1",
                params![class_id],
            )
            .map(drop)
        })
    }

    /// Bytes of every stored chunk.
    ///
    /// # Errors
    /// Storage errors.
    pub fn artifact_bytes(&self) -> Result<u64> {
        self.with(|c| {
            c.query_row(
                "SELECT COALESCE(SUM(LENGTH(bytes)), 0) FROM artifact_chunks",
                [],
                |row| row.get::<_, i64>(0),
            )
        })
        .map(|total| u64::try_from(total).unwrap_or_default())
    }
}

fn join_hashes(hashes: &[[u8; 32]]) -> Vec<u8> {
    hashes.concat()
}

fn split_hashes(bytes: &[u8]) -> Vec<[u8; 32]> {
    bytes.as_chunks::<32>().0.to_vec()
}
