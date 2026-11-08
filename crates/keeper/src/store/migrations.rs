//! Embedded schema migrations, applied in order and tracked by `PRAGMA user_version`. Append only: a released
//! migration never changes.

pub(super) const MIGRATIONS: &[&str] = &[
    // 1: committees, ceremonies, the ECDH audit log and observed configs.
    "
    CREATE TABLE manifests (
        vault BLOB NOT NULL,
        epoch INTEGER NOT NULL,
        body BLOB NOT NULL,
        status TEXT NOT NULL CHECK (status IN ('active', 'retired', 'removed')),
        updated_at INTEGER NOT NULL,
        PRIMARY KEY (vault, epoch)
    );
    CREATE TABLE ceremonies (
        session BLOB PRIMARY KEY,
        kind TEXT NOT NULL CHECK (kind IN ('dkg', 'reshare')),
        vault BLOB,
        status TEXT NOT NULL,
        params TEXT NOT NULL,
        result TEXT,
        updated_at INTEGER NOT NULL
    );
    CREATE TABLE audit (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        at INTEGER NOT NULL,
        vault BLOB NOT NULL,
        requester TEXT NOT NULL,
        purpose TEXT NOT NULL,
        points INTEGER NOT NULL,
        outcome TEXT NOT NULL
    );
    CREATE INDEX audit_by_vault ON audit (vault, id);
    CREATE TABLE observations (
        vault BLOB NOT NULL,
        version INTEGER NOT NULL,
        threshold INTEGER NOT NULL,
        signer_count INTEGER NOT NULL,
        signer_set_commitment BLOB NOT NULL,
        viewing_threshold INTEGER NOT NULL,
        committee_commitment BLOB NOT NULL,
        observed_at INTEGER NOT NULL,
        PRIMARY KEY (vault, version)
    );
    ",
];
