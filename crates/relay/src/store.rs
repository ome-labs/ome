//! Mailbox storage. Rows hold the opaque mailbox id, the expiry and the sealed bytes; who deposited an envelope is never
//! recorded.

use std::{path::Path, sync::Mutex};

use ome_wire::{
    limits::{MAX_ENVELOPE_BYTES, MAX_FETCH_ENVELOPES},
    messages::mailbox::{Refusal, RelayEnvelope, StoredEnvelope},
};
use rusqlite::{Connection, params};

use crate::{Error, Limits, Result};

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS envelopes (
    id BLOB PRIMARY KEY,
    mailbox BLOB NOT NULL,
    expires_at INTEGER NOT NULL,
    sealed BLOB NOT NULL,
    size INTEGER NOT NULL,
    stored_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS envelopes_by_mailbox ON envelopes (mailbox, stored_at);
CREATE INDEX IF NOT EXISTS envelopes_by_expiry ON envelopes (expires_at);
";

fn ts(seconds: u64) -> i64 {
    i64::try_from(seconds).unwrap_or(i64::MAX)
}

/// Envelope count and total sealed bytes held.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Usage {
    /// Envelopes.
    pub envelopes: u64,
    /// Sealed bytes.
    pub bytes: u64,
}

/// The relay's envelope store.
pub struct MailboxStore {
    conn: Mutex<Connection>,
    limits: Limits,
}

impl MailboxStore {
    /// Opens (creating) the store at `path`.
    ///
    /// # Errors
    /// Storage errors.
    pub fn open(path: &Path, limits: Limits) -> Result<Self> {
        Self::init(Connection::open(path)?, limits)
    }

    /// An in-memory store (tests).
    ///
    /// # Errors
    /// Storage errors.
    pub fn in_memory(limits: Limits) -> Result<Self> {
        Self::init(Connection::open_in_memory()?, limits)
    }

    fn init(conn: Connection, limits: Limits) -> Result<Self> {
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "secure_delete", "ON")?;
        conn.execute_batch(SCHEMA)?;
        Ok(Self {
            conn: Mutex::new(conn),
            limits,
        })
    }

    fn with<T>(&self, f: impl FnOnce(&Connection) -> rusqlite::Result<T>) -> Result<T> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| Error::Storage("lock poisoned".into()))?;
        Ok(f(&conn)?)
    }

    /// Stores an envelope, or says why not.
    ///
    /// # Errors
    /// Storage errors (a refusal is `Ok(Err(_))`).
    pub fn deposit(&self, envelope: &RelayEnvelope, now: u64) -> Result<Result<[u8; 32], Refusal>> {
        let size = envelope.sealed.len();
        if size == 0 || size > MAX_ENVELOPE_BYTES {
            return Ok(Err(Refusal::TooLarge));
        }
        if envelope.expires_at <= now || envelope.expires_at > now + self.limits.max_ttl_secs {
            return Ok(Err(Refusal::BadExpiry));
        }
        let id = envelope.id().map_err(|e| Error::Invalid(e.to_string()))?;
        let limits = self.limits;
        self.with(|c| {
            let (count, bytes): (i64, i64) = c.query_row(
                "SELECT COUNT(*), COALESCE(SUM(size), 0) FROM envelopes WHERE mailbox = ?1",
                params![envelope.recipient],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            let total: i64 =
                c.query_row("SELECT COALESCE(SUM(size), 0) FROM envelopes", [], |row| {
                    row.get(0)
                })?;
            let size = i64::try_from(size).unwrap_or(i64::MAX);
            let over = count >= ts(limits.max_envelopes_per_mailbox)
                || bytes + size > ts(limits.max_bytes_per_mailbox)
                || total + size > ts(limits.max_total_bytes);
            if over {
                return Ok(Err(Refusal::QuotaExceeded));
            }
            c.execute(
                "INSERT OR IGNORE INTO envelopes (id, mailbox, expires_at, sealed, size, stored_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    id,
                    envelope.recipient,
                    ts(envelope.expires_at),
                    envelope.sealed,
                    size,
                    ts(now)
                ],
            )?;
            Ok(Ok(id))
        })
    }

    /// Unexpired envelopes of `mailboxes`, oldest first, up to the fetch size limit and at most
    /// [`MAX_FETCH_ENVELOPES`], so the answer always fits the wire's bounds; the keeper deletes and fetches again.
    ///
    /// # Errors
    /// Storage errors.
    pub fn fetch(&self, mailboxes: &[[u8; 32]], now: u64) -> Result<Vec<StoredEnvelope>> {
        let limit = self.limits.max_fetch_bytes;
        self.with(|c| {
            let mut statement = c.prepare(
                "SELECT id, mailbox, expires_at, sealed FROM envelopes WHERE mailbox = ?1 AND expires_at > ?2
                 ORDER BY stored_at, id",
            )?;
            let mut found = Vec::new();
            let mut bytes = 0usize;
            'mailboxes: for mailbox in mailboxes {
                let rows = statement.query_map(params![mailbox, ts(now)], |row| {
                    Ok(StoredEnvelope {
                        id: row.get(0)?,
                        envelope: RelayEnvelope {
                            recipient: row.get(1)?,
                            expires_at: u64::try_from(row.get::<_, i64>(2)?).unwrap_or(0),
                            sealed: row.get(3)?,
                        },
                    })
                })?;
                for row in rows {
                    let stored = row?;
                    bytes += stored.envelope.sealed.len();
                    if (bytes > limit && !found.is_empty()) || found.len() == MAX_FETCH_ENVELOPES {
                        break 'mailboxes;
                    }
                    found.push(stored);
                }
            }
            Ok(found)
        })
    }

    /// Deletes envelopes by id; returns how many existed.
    ///
    /// # Errors
    /// Storage errors.
    pub fn delete(&self, ids: &[[u8; 32]]) -> Result<u32> {
        self.with(|c| {
            let mut deleted = 0u32;
            for id in ids {
                deleted +=
                    u32::try_from(c.execute("DELETE FROM envelopes WHERE id = ?1", params![id])?)
                        .unwrap_or(0);
            }
            Ok(deleted)
        })
    }

    /// Drops expired envelopes; returns how many.
    ///
    /// # Errors
    /// Storage errors.
    pub fn purge(&self, now: u64) -> Result<usize> {
        self.with(|c| {
            c.execute(
                "DELETE FROM envelopes WHERE expires_at <= ?1",
                params![ts(now)],
            )
        })
    }

    /// Current usage.
    ///
    /// # Errors
    /// Storage errors.
    pub fn usage(&self) -> Result<Usage> {
        self.with(|c| {
            c.query_row(
                "SELECT COUNT(*), COALESCE(SUM(size), 0) FROM envelopes",
                [],
                |row| {
                    Ok(Usage {
                        envelopes: u64::try_from(row.get::<_, i64>(0)?).unwrap_or(0),
                        bytes: u64::try_from(row.get::<_, i64>(1)?).unwrap_or(0),
                    })
                },
            )
        })
    }
}
