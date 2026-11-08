//! Local `SQLite` database for public, non-secret state: committee manifests, ceremony status and results, the ECDH
//! audit log and observed vault configs. Secrets never go here (see [`crate::custody`]).

mod migrations;

use std::{path::Path, sync::Mutex};

use ome_protocol::vault::Config;
use ome_wire::{Message, decode_body, encode_body, messages::ceremony::Manifest, types::field};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Serialize, de::DeserializeOwned};

use crate::{Error, Result};

/// Status of a stored manifest.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ManifestStatus {
    /// The committee currently serving the vault.
    Active,
    /// Superseded by a later epoch.
    Retired,
    /// This keeper is not in the vault's current committee.
    Removed,
}

impl ManifestStatus {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Retired => "retired",
            Self::Removed => "removed",
        }
    }
}

/// A stored ceremony row.
#[derive(Clone, Debug)]
pub struct CeremonyRow {
    /// `dkg` or `reshare`.
    pub kind: String,
    /// Vault (reshare only).
    pub vault: Option<[u8; 32]>,
    /// Status string.
    pub status: String,
    /// Parameters (JSON).
    pub params: String,
    /// Outcome (JSON), once finished.
    pub result: Option<String>,
}

/// One ECDH audit entry.
#[derive(Clone, Debug, Serialize)]
pub struct AuditEntry {
    /// Unix seconds.
    pub at: u64,
    /// `local` or the requesting keeper's identity (hex).
    pub requester: String,
    /// `decryption`, `tagging`, `handshake`, ...
    pub purpose: String,
    /// Number of points.
    pub points: u32,
    /// `served` or the refusal reason.
    pub outcome: String,
}

/// The keeper database.
pub struct Store {
    conn: Mutex<Connection>,
}

impl Store {
    /// Opens (creating and migrating) the database at `path`.
    ///
    /// # Errors
    /// Storage errors.
    pub fn open(path: &Path) -> Result<Self> {
        Self::init(Connection::open(path)?)
    }

    /// An in-memory database (tests).
    ///
    /// # Errors
    /// Storage errors.
    pub fn in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self> {
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.pragma_update(None, "secure_delete", "ON")?;
        let version: i64 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
        let version = usize::try_from(version).unwrap_or(0);
        for (i, migration) in migrations::MIGRATIONS.iter().enumerate().skip(version) {
            conn.execute_batch(&format!(
                "BEGIN; {migration} PRAGMA user_version = {}; COMMIT;",
                i + 1
            ))?;
        }
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    fn with<T>(&self, f: impl FnOnce(&Connection) -> rusqlite::Result<T>) -> Result<T> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| Error::Storage("database lock poisoned".into()))?;
        Ok(f(&conn)?)
    }

    // --- Manifests ---------------------------------------------------------------------------------------------------

    /// Stores a manifest with `status`.
    ///
    /// # Errors
    /// Storage or encoding errors.
    pub fn put_manifest(
        &self,
        manifest: &Manifest,
        status: ManifestStatus,
        now: u64,
    ) -> Result<()> {
        let body = encode_body(&Message::Manifest(manifest.clone()))?;
        self.with(|c| {
            c.execute(
                "INSERT INTO manifests (vault, epoch, body, status, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT (vault, epoch) DO UPDATE SET body = excluded.body, status = excluded.status,
                 updated_at = excluded.updated_at",
                params![manifest.vault, manifest.epoch, body, status.as_str(), ts(now)],
            )
            .map(drop)
        })
    }

    /// Sets the status of every other epoch of `vault` to `retired` and `epoch` to `status`.
    ///
    /// # Errors
    /// Storage errors.
    pub fn set_manifest_status(
        &self,
        vault: &[u8; 32],
        epoch: u32,
        status: ManifestStatus,
        now: u64,
    ) -> Result<()> {
        self.with(|c| {
            c.execute(
                "UPDATE manifests SET status = 'retired', updated_at = ?3 WHERE vault = ?1 AND epoch != ?2
                 AND status = 'active'",
                params![vault, epoch, ts(now)],
            )?;
            c.execute(
                "UPDATE manifests SET status = ?3, updated_at = ?4 WHERE vault = ?1 AND epoch = ?2",
                params![vault, epoch, status.as_str(), ts(now)],
            )
            .map(drop)
        })
    }

    /// Latest manifest of `vault` with its status.
    ///
    /// # Errors
    /// Storage or decoding errors.
    pub fn latest_manifest(&self, vault: &[u8; 32]) -> Result<Option<(Manifest, ManifestStatus)>> {
        let row: Option<(Vec<u8>, String)> = self.with(|c| {
            c.query_row(
                "SELECT body, status FROM manifests WHERE vault = ?1 ORDER BY epoch DESC LIMIT 1",
                params![vault],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
        })?;
        row.map(|(body, status)| Ok((decode_manifest(&body)?, parse_status(&status)?)))
            .transpose()
    }

    /// Every vault's latest manifest.
    ///
    /// # Errors
    /// Storage or decoding errors.
    pub fn vaults(&self) -> Result<Vec<(Manifest, ManifestStatus)>> {
        let rows: Vec<(Vec<u8>, String)> = self.with(|c| {
            let mut statement = c.prepare(
                "SELECT body, status FROM manifests m WHERE epoch = (SELECT MAX(epoch) FROM manifests WHERE vault = m.vault)",
            )?;
            statement
                .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
                .collect()
        })?;
        rows.into_iter()
            .map(|(body, status)| Ok((decode_manifest(&body)?, parse_status(&status)?)))
            .collect()
    }

    // --- Ceremonies --------------------------------------------------------------------------------------------------

    /// Inserts a new ceremony.
    ///
    /// # Errors
    /// [`Error::CeremonyExists`] for a reused session id.
    pub fn create_ceremony(
        &self,
        session: &[u8; 32],
        kind: &str,
        vault: Option<&[u8; 32]>,
        params: &impl Serialize,
        now: u64,
    ) -> Result<()> {
        let params = json(params)?;
        let inserted = self.with(|c| {
            c.execute(
                "INSERT OR IGNORE INTO ceremonies (session, kind, vault, status, params, updated_at)
                 VALUES (?1, ?2, ?3, 'running', ?4, ?5)",
                params![session, kind, vault, params, ts(now)],
            )
        })?;
        if inserted == 0 {
            return Err(Error::CeremonyExists);
        }
        Ok(())
    }

    /// Records a ceremony's status and (once finished) result.
    ///
    /// # Errors
    /// Storage errors.
    pub fn update_ceremony(
        &self,
        session: &[u8; 32],
        status: &str,
        result: Option<&impl Serialize>,
        now: u64,
    ) -> Result<()> {
        let result = result.map(json).transpose()?;
        self.with(|c| {
            c.execute(
                "UPDATE ceremonies SET status = ?2, result = COALESCE(?3, result), updated_at = ?4 WHERE session = ?1",
                params![session, status, result, ts(now)],
            )
            .map(drop)
        })
    }

    /// A ceremony row.
    ///
    /// # Errors
    /// Storage errors.
    pub fn ceremony(&self, session: &[u8; 32]) -> Result<Option<CeremonyRow>> {
        self.with(|c| {
            c.query_row(
                "SELECT kind, vault, status, params, result FROM ceremonies WHERE session = ?1",
                params![session],
                |row| {
                    Ok(CeremonyRow {
                        kind: row.get(0)?,
                        vault: row.get(1)?,
                        status: row.get(2)?,
                        params: row.get(3)?,
                        result: row.get(4)?,
                    })
                },
            )
            .optional()
        })
    }

    /// Sessions still marked running (crash recovery aborts them: ceremony secrets are never persisted).
    ///
    /// # Errors
    /// Storage errors.
    pub fn running_ceremonies(&self) -> Result<Vec<[u8; 32]>> {
        self.with(|c| {
            let mut statement =
                c.prepare("SELECT session FROM ceremonies WHERE status = 'running'")?;
            statement.query_map([], |row| row.get(0))?.collect()
        })
    }

    /// Completed reshares of `vault` not yet activated, with their results (JSON).
    ///
    /// # Errors
    /// Storage errors.
    pub fn completed_reshares(&self, vault: &[u8; 32]) -> Result<Vec<([u8; 32], String)>> {
        self.with(|c| {
            let mut statement = c.prepare(
                "SELECT session, result FROM ceremonies WHERE kind = 'reshare' AND vault = ?1 AND status = 'completed'
                 AND result IS NOT NULL ORDER BY updated_at DESC",
            )?;
            statement.query_map(params![vault], |row| Ok((row.get(0)?, row.get(1)?)))?.collect()
        })
    }

    /// Completed reshares, of any vault, finished before `before` and never activated: their pending shares expire.
    ///
    /// # Errors
    /// Storage errors.
    pub fn stale_reshares(&self, before: u64) -> Result<Vec<([u8; 32], [u8; 32])>> {
        self.with(|c| {
            let mut statement = c.prepare(
                "SELECT session, vault FROM ceremonies WHERE kind = 'reshare' AND status = 'completed'
                 AND updated_at < ?1",
            )?;
            statement.query_map(params![ts(before)], |row| Ok((row.get(0)?, row.get(1)?)))?.collect()
        })
    }

    // --- Audit and observations --------------------------------------------------------------------------------------

    /// Appends an ECDH audit entry.
    ///
    /// # Errors
    /// Storage errors.
    pub fn audit(&self, vault: &[u8; 32], entry: &AuditEntry) -> Result<()> {
        self.with(|c| {
            c.execute(
                "INSERT INTO audit (at, vault, requester, purpose, points, outcome) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![ts(entry.at), vault, entry.requester, entry.purpose, entry.points, entry.outcome],
            )
            .map(drop)
        })
    }

    /// The latest `limit` audit entries of `vault`, newest first.
    ///
    /// # Errors
    /// Storage errors.
    pub fn audit_log(&self, vault: &[u8; 32], limit: u32) -> Result<Vec<AuditEntry>> {
        self.with(|c| {
            let mut statement = c.prepare(
                "SELECT at, requester, purpose, points, outcome FROM audit WHERE vault = ?1 ORDER BY id DESC LIMIT ?2",
            )?;
            statement
                .query_map(params![vault, limit], |row| {
                    Ok(AuditEntry {
                        at: u64::try_from(row.get::<_, i64>(0)?).unwrap_or(0),
                        requester: row.get(1)?,
                        purpose: row.get(2)?,
                        points: row.get(3)?,
                        outcome: row.get(4)?,
                    })
                })?
                .collect()
        })
    }

    /// Records a config the member's daemon observed on chain.
    ///
    /// # Errors
    /// Storage errors.
    pub fn observe(&self, vault: &[u8; 32], config: &Config, now: u64) -> Result<()> {
        let signers = field(&config.signer_set_commitment);
        let committee = field(&config.committee_commitment);
        self.with(|c| {
            c.execute(
                "INSERT OR IGNORE INTO observations (vault, version, threshold, signer_count, signer_set_commitment,
                 viewing_threshold, committee_commitment, observed_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    vault,
                    config.version,
                    config.threshold,
                    config.signer_count,
                    signers,
                    config.viewing_threshold,
                    committee,
                    ts(now)
                ],
            )
            .map(drop)
        })
    }
}
/// Unix seconds as the `SQLite` integer type.
fn ts(seconds: u64) -> i64 {
    i64::try_from(seconds).unwrap_or(i64::MAX)
}

fn decode_manifest(body: &[u8]) -> Result<Manifest> {
    match decode_body(body)? {
        Message::Manifest(manifest) => Ok(manifest),
        _ => Err(Error::Storage(
            "stored manifest has another message kind".into(),
        )),
    }
}

fn parse_status(status: &str) -> Result<ManifestStatus> {
    match status {
        "active" => Ok(ManifestStatus::Active),
        "retired" => Ok(ManifestStatus::Retired),
        "removed" => Ok(ManifestStatus::Removed),
        other => Err(Error::Storage(format!("unknown manifest status {other}"))),
    }
}

fn json(value: &impl Serialize) -> Result<String> {
    serde_json::to_string(value).map_err(|e| Error::Storage(e.to_string()))
}

/// Parses a stored JSON column.
///
/// # Errors
/// [`Error::Storage`] for malformed JSON.
pub fn from_json<T: DeserializeOwned>(text: &str) -> Result<T> {
    serde_json::from_str(text).map_err(|e| Error::Storage(e.to_string()))
}
