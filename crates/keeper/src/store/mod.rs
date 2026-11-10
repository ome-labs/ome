//! Local `SQLite` database for public, non-secret state: committee manifests, ceremony status and results, proposals,
//! payloads, votes and receipts, contract artifacts, lobbies, the ECDH audit log and observed vault configs. Secrets never go here (see
//! [`crate::custody`]).

mod artifacts;
mod migrations;

use std::{path::Path, sync::Mutex};

use ome_protocol::vault::Config;
use ome_wire::{
    IdentityPublic, Message, decode_body, encode_body,
    messages::ceremony::Manifest,
    types::{WirePoint, field, parse_field},
};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Serialize, de::DeserializeOwned};

use crate::{Error, Result};

pub use artifacts::ArtifactRecord;

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

/// A stored execution receipt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReceiptEntry {
    /// Proposal id.
    pub proposal: [u8; 32],
    /// Transaction that executed it.
    pub tx_hash: [u8; 32],
    /// Unix seconds it was stored.
    pub at: u64,
    /// The signed receipt (a `Message::ExecutionReceipt`); `None` for one stored before receipts were signed.
    pub body: Option<Vec<u8>>,
}

/// A receipt's proposal id and its signed body (`None` for one stored before receipts were signed).
pub type StoredReceipt = ([u8; 32], Option<Vec<u8>>);

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

    pub(super) fn with<T>(&self, f: impl FnOnce(&Connection) -> rusqlite::Result<T>) -> Result<T> {
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

    // --- Proposals, approvals, receipts -----------------------------------------------------------------------------

    /// Stores a proposal body (idempotent).
    ///
    /// # Errors
    /// Storage errors.
    pub fn put_proposal(
        &self,
        id: &[u8; 32],
        vault: &[u8; 32],
        body: &[u8],
        now: u64,
    ) -> Result<bool> {
        self.with(|c| {
            c.execute(
                "INSERT OR IGNORE INTO proposals (id, vault, body, created_at) VALUES (?1, ?2, ?3, ?4)",
                params![id, vault, body, ts(now)],
            )
            .map(|n| n > 0)
        })
    }

    /// A proposal body and its vault.
    ///
    /// # Errors
    /// Storage errors.
    pub fn proposal(&self, id: &[u8; 32]) -> Result<Option<([u8; 32], Vec<u8>)>> {
        self.with(|c| {
            c.query_row(
                "SELECT vault, body FROM proposals WHERE id = ?1",
                params![id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
        })
    }

    /// Proposal bodies of `vault`, oldest first.
    ///
    /// # Errors
    /// Storage errors.
    pub fn proposals(&self, vault: &[u8; 32]) -> Result<Vec<Vec<u8>>> {
        self.with(|c| {
            let mut statement =
                c.prepare("SELECT body FROM proposals WHERE vault = ?1 ORDER BY created_at, id")?;
            statement
                .query_map(params![vault], |row| row.get(0))?
                .collect()
        })
    }

    /// Stores an approval body (one per slot; idempotent).
    ///
    /// # Errors
    /// Storage errors.
    pub fn put_approval(
        &self,
        proposal: &[u8; 32],
        slot: u32,
        body: &[u8],
        now: u64,
    ) -> Result<bool> {
        self.with(|c| {
            c.execute(
                "INSERT OR IGNORE INTO approvals (proposal, slot, body, created_at) VALUES (?1, ?2, ?3, ?4)",
                params![proposal, slot, body, ts(now)],
            )
            .map(|n| n > 0)
        })
    }

    /// Approval bodies of a proposal, by slot.
    ///
    /// # Errors
    /// Storage errors.
    pub fn approvals(&self, proposal: &[u8; 32]) -> Result<Vec<Vec<u8>>> {
        self.with(|c| {
            let mut statement =
                c.prepare("SELECT body FROM approvals WHERE proposal = ?1 ORDER BY slot")?;
            statement
                .query_map(params![proposal], |row| row.get(0))?
                .collect()
        })
    }

    /// Approval bodies of a proposal with the time each was stored, by slot.
    ///
    /// # Errors
    /// Storage errors.
    pub fn approval_entries(&self, proposal: &[u8; 32]) -> Result<Vec<(Vec<u8>, u64)>> {
        self.vote_entries("approvals", proposal)
    }

    /// Rejection bodies of a proposal with the time each was stored, by slot.
    ///
    /// # Errors
    /// Storage errors.
    pub fn rejection_entries(&self, proposal: &[u8; 32]) -> Result<Vec<(Vec<u8>, u64)>> {
        self.vote_entries("rejections", proposal)
    }

    fn vote_entries(&self, table: &str, proposal: &[u8; 32]) -> Result<Vec<(Vec<u8>, u64)>> {
        self.with(|c| {
            let mut statement = c.prepare(&format!(
                "SELECT body, created_at FROM {table} WHERE proposal = ?1 ORDER BY slot"
            ))?;
            statement
                .query_map(params![proposal], |row| {
                    Ok((
                        row.get(0)?,
                        u64::try_from(row.get::<_, i64>(1)?).unwrap_or(0),
                    ))
                })?
                .collect()
        })
    }

    /// Ids of every proposal of `vault`.
    ///
    /// # Errors
    /// Storage errors.
    pub fn proposal_ids(&self, vault: &[u8; 32]) -> Result<Vec<[u8; 32]>> {
        self.with(|c| {
            let mut statement =
                c.prepare("SELECT id FROM proposals WHERE vault = ?1 ORDER BY created_at, id")?;
            statement
                .query_map(params![vault], |row| row.get(0))?
                .collect()
        })
    }

    /// `(proposal, slot)` of every approval of `vault`.
    ///
    /// # Errors
    /// Storage errors.
    pub fn approval_refs(&self, vault: &[u8; 32]) -> Result<Vec<([u8; 32], u32)>> {
        self.with(|c| {
            let mut statement = c.prepare(
                "SELECT a.proposal, a.slot FROM approvals a JOIN proposals p ON p.id = a.proposal WHERE p.vault = ?1
                 ORDER BY a.created_at",
            )?;
            statement.query_map(params![vault], |row| Ok((row.get(0)?, row.get(1)?)))?.collect()
        })
    }

    /// One approval body.
    ///
    /// # Errors
    /// Storage errors.
    pub fn approval(&self, proposal: &[u8; 32], slot: u32) -> Result<Option<Vec<u8>>> {
        self.with(|c| {
            c.query_row(
                "SELECT body FROM approvals WHERE proposal = ?1 AND slot = ?2",
                params![proposal, slot],
                |row| row.get(0),
            )
            .optional()
        })
    }

    /// Stores a rejection body (one per slot; idempotent).
    ///
    /// # Errors
    /// Storage errors.
    pub fn put_rejection(
        &self,
        proposal: &[u8; 32],
        slot: u32,
        body: &[u8],
        now: u64,
    ) -> Result<bool> {
        self.with(|c| {
            c.execute(
                "INSERT OR IGNORE INTO rejections (proposal, slot, body, created_at) VALUES (?1, ?2, ?3, ?4)",
                params![proposal, slot, body, ts(now)],
            )
            .map(|n| n > 0)
        })
    }

    /// Rejection bodies of a proposal, by slot.
    ///
    /// # Errors
    /// Storage errors.
    pub fn rejections(&self, proposal: &[u8; 32]) -> Result<Vec<Vec<u8>>> {
        self.with(|c| {
            let mut statement =
                c.prepare("SELECT body FROM rejections WHERE proposal = ?1 ORDER BY slot")?;
            statement
                .query_map(params![proposal], |row| row.get(0))?
                .collect()
        })
    }

    /// `(proposal, slot)` of every rejection of `vault`.
    ///
    /// # Errors
    /// Storage errors.
    pub fn rejection_refs(&self, vault: &[u8; 32]) -> Result<Vec<([u8; 32], u32)>> {
        self.with(|c| {
            let mut statement = c.prepare(
                "SELECT r.proposal, r.slot FROM rejections r JOIN proposals p ON p.id = r.proposal WHERE p.vault = ?1
                 ORDER BY r.created_at",
            )?;
            statement.query_map(params![vault], |row| Ok((row.get(0)?, row.get(1)?)))?.collect()
        })
    }

    /// One rejection body.
    ///
    /// # Errors
    /// Storage errors.
    pub fn rejection(&self, proposal: &[u8; 32], slot: u32) -> Result<Option<Vec<u8>>> {
        self.with(|c| {
            c.query_row(
                "SELECT body FROM rejections WHERE proposal = ?1 AND slot = ?2",
                params![proposal, slot],
                |row| row.get(0),
            )
            .optional()
        })
    }

    /// `(proposal, tx_hash)` of every receipt of `vault`.
    ///
    /// # Errors
    /// Storage errors.
    pub fn receipts(&self, vault: &[u8; 32]) -> Result<Vec<StoredReceipt>> {
        self.with(|c| {
            let mut statement = c.prepare(
                "SELECT proposal, body FROM receipts WHERE vault = ?1 ORDER BY created_at",
            )?;
            statement
                .query_map(params![vault], |row| Ok((row.get(0)?, row.get(1)?)))?
                .collect()
        })
    }

    /// Records that `proposal` executed in `tx_hash`. Returns whether anything changed (a new receipt, or one that
    /// replaces a different transaction); storing the same receipt again keeps its original time.
    ///
    /// # Errors
    /// Storage errors.
    pub fn put_receipt(
        &self,
        proposal: &[u8; 32],
        vault: &[u8; 32],
        tx_hash: &[u8; 32],
        body: &[u8],
        now: u64,
    ) -> Result<bool> {
        self.with(|c| {
            c.execute(
                "INSERT INTO receipts (proposal, vault, tx_hash, body, created_at) VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT (proposal) DO UPDATE SET
                    vault = excluded.vault, tx_hash = excluded.tx_hash, body = excluded.body,
                    created_at = excluded.created_at
                 WHERE receipts.vault IS NOT excluded.vault OR receipts.tx_hash IS NOT excluded.tx_hash",
                params![proposal, vault, tx_hash, body, ts(now)],
            )
            .map(|n| n > 0)
        })
    }

    /// Every receipt of `vault` with the time it was stored, newest first.
    ///
    /// # Errors
    /// Storage errors.
    pub fn receipt_entries(&self, vault: &[u8; 32]) -> Result<Vec<ReceiptEntry>> {
        self.with(|c| {
            let mut statement = c.prepare(
                "SELECT proposal, tx_hash, created_at, body FROM receipts WHERE vault = ?1
                 ORDER BY created_at DESC, rowid DESC",
            )?;
            statement
                .query_map(params![vault], |row| {
                    Ok(ReceiptEntry {
                        proposal: row.get(0)?,
                        tx_hash: row.get(1)?,
                        at: u64::try_from(row.get::<_, i64>(2)?).unwrap_or(0),
                        body: row.get(3)?,
                    })
                })?
                .collect()
        })
    }

    /// Whether some known proposal of `vault` executed in `tx_hash`.
    ///
    /// # Errors
    /// Storage errors.
    pub fn has_receipt(&self, vault: &[u8; 32], tx_hash: &[u8; 32]) -> Result<bool> {
        self.with(|c| {
            c.query_row(
                "SELECT 1 FROM receipts WHERE vault = ?1 AND tx_hash = ?2",
                params![vault, tx_hash],
                |_| Ok(()),
            )
            .optional()
            .map(|r| r.is_some())
        })
    }

    // --- Lobbies -----------------------------------------------------------------------------------------------------

    /// Inserts or replaces a lobby with its role, status and record (JSON).
    ///
    /// # Errors
    /// Storage errors.
    pub fn put_lobby(
        &self,
        lobby: &[u8; 16],
        role: &str,
        status: &str,
        record: &impl Serialize,
        now: u64,
    ) -> Result<()> {
        let record = json(record)?;
        self.with(|c| {
            c.execute(
                "INSERT INTO lobbies (lobby, role, status, record, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT (lobby) DO UPDATE SET role = excluded.role, status = excluded.status,
                 record = excluded.record, updated_at = excluded.updated_at",
                params![lobby, role, status, record, ts(now)],
            )
            .map(drop)
        })
    }

    /// Every lobby record (JSON), oldest first.
    ///
    /// # Errors
    /// Storage errors.
    pub fn lobbies(&self) -> Result<Vec<String>> {
        self.with(|c| {
            let mut statement = c.prepare("SELECT record FROM lobbies ORDER BY rowid")?;
            statement.query_map([], |row| row.get(0))?.collect()
        })
    }

    // --- Payloads ---------------------------------------------------------------------------------------------------

    /// Stores a proposal payload under its hash (idempotent). Returns whether it was new.
    ///
    /// # Errors
    /// Storage errors.
    pub fn put_payload(
        &self,
        hash: &[u8; 32],
        vault: &[u8; 32],
        bytes: &[u8],
        now: u64,
    ) -> Result<bool> {
        self.with(|c| {
            c.execute(
                "INSERT OR IGNORE INTO payloads (hash, vault, bytes, created_at) VALUES (?1, ?2, ?3, ?4)",
                params![hash, vault, bytes, ts(now)],
            )
            .map(|changed| changed > 0)
        })
    }

    /// A stored payload of `vault`.
    ///
    /// # Errors
    /// Storage errors.
    pub fn payload(&self, vault: &[u8; 32], hash: &[u8; 32]) -> Result<Option<Vec<u8>>> {
        self.with(|c| {
            c.query_row(
                "SELECT bytes FROM payloads WHERE vault = ?1 AND hash = ?2",
                params![vault, hash],
                |row| row.get(0),
            )
            .optional()
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

    /// The newest config version observed for `vault`.
    ///
    /// # Errors
    /// Storage errors.
    pub fn latest_observed_version(&self, vault: &[u8; 32]) -> Result<Option<u32>> {
        self.with(|c| {
            c.query_row(
                "SELECT MAX(version) FROM observations WHERE vault = ?1",
                params![vault],
                |row| row.get(0),
            )
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
/// A paired signing device.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceRecord {
    /// Ed25519 identity.
    pub identity: IdentityPublic,
    /// X25519 transport key.
    pub transport: [u8; 32],
    /// The Grumpkin key it signs approvals with.
    pub signer_key: WirePoint,
    /// Display name.
    pub name: String,
    /// When it paired (unix seconds).
    pub paired_at: u64,
}

impl Store {
    // --- Devices -----------------------------------------------------------------------------------------------------

    /// Stores a paired device, replacing an earlier pairing of the same identity.
    ///
    /// # Errors
    /// Storage errors, or a transport key another device uses.
    pub fn put_device(&self, device: &DeviceRecord) -> Result<()> {
        let mut key = Vec::with_capacity(64);
        key.extend_from_slice(&device.signer_key.x);
        key.extend_from_slice(&device.signer_key.y);
        self.with(|c| {
            c.execute(
                "INSERT INTO devices (identity, transport, signer_key, name, paired_at) VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT (identity) DO UPDATE SET transport = excluded.transport,
                 signer_key = excluded.signer_key, name = excluded.name, paired_at = excluded.paired_at",
                params![device.identity.0, device.transport, key, device.name, ts(device.paired_at)],
            )
            .map(drop)
        })
    }

    /// Every paired device.
    ///
    /// # Errors
    /// Storage errors.
    pub fn devices(&self) -> Result<Vec<DeviceRecord>> {
        type Row = ([u8; 32], [u8; 32], Vec<u8>, String, i64);
        let rows: Vec<Row> = self.with(|c| {
            let mut statement = c.prepare(
                "SELECT identity, transport, signer_key, name, paired_at FROM devices ORDER BY paired_at",
            )?;
            statement
                .query_map([], |row| {
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?))
                })?
                .collect()
        })?;
        rows.into_iter()
            .map(|(identity, transport, key, name, paired_at)| {
                let (x, y) = key
                    .split_at_checked(32)
                    .filter(|(_, y)| y.len() == 32)
                    .ok_or_else(|| Error::Storage("stored device key is not a point".into()))?;
                Ok(DeviceRecord {
                    identity: IdentityPublic(identity),
                    transport,
                    signer_key: WirePoint {
                        x: x.try_into().unwrap_or_default(),
                        y: y.try_into().unwrap_or_default(),
                    },
                    name,
                    paired_at: u64::try_from(paired_at).unwrap_or(0),
                })
            })
            .collect()
    }

    /// Forgets a device. Returns whether it was paired.
    ///
    /// # Errors
    /// Storage errors.
    pub fn remove_device(&self, identity: &IdentityPublic) -> Result<bool> {
        self.with(|c| {
            c.execute(
                "DELETE FROM devices WHERE identity = ?1",
                params![identity.0],
            )
            .map(|n| n > 0)
        })
    }

    /// The newest config observed for `vault`.
    ///
    /// # Errors
    /// Storage errors, or a stored commitment that is not a field.
    pub fn latest_observation(&self, vault: &[u8; 32]) -> Result<Option<Config>> {
        type Row = (u32, u32, u32, [u8; 32], u32, [u8; 32]);
        let row: Option<Row> = self.with(|c| {
            c.query_row(
                "SELECT version, threshold, signer_count, signer_set_commitment, viewing_threshold,
                 committee_commitment FROM observations WHERE vault = ?1 ORDER BY version DESC LIMIT 1",
                params![vault],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                    ))
                },
            )
            .optional()
        })?;
        row.map(
            |(version, threshold, signer_count, signers, viewing_threshold, committee)| {
                Ok(Config {
                    threshold,
                    signer_count,
                    version,
                    signer_set_commitment: parse_field(&signers)?,
                    viewing_threshold,
                    committee_commitment: parse_field(&committee)?,
                })
            },
        )
        .transpose()
    }
}

/// Unix seconds as the `SQLite` integer type.
pub(super) fn ts(seconds: u64) -> i64 {
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
