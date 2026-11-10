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
    // 2: spending records and founding lobbies (public roster and plan; the invite secret lives in custody).
    "
    CREATE TABLE proposals (
        id BLOB PRIMARY KEY,
        vault BLOB NOT NULL,
        body BLOB NOT NULL,
        created_at INTEGER NOT NULL
    );
    CREATE INDEX proposals_by_vault ON proposals (vault, created_at);
    CREATE TABLE payloads (
        hash BLOB PRIMARY KEY,
        vault BLOB NOT NULL,
        bytes BLOB NOT NULL,
        created_at INTEGER NOT NULL
    );
    CREATE TABLE approvals (
        proposal BLOB NOT NULL,
        slot INTEGER NOT NULL,
        body BLOB NOT NULL,
        created_at INTEGER NOT NULL,
        PRIMARY KEY (proposal, slot)
    );
    CREATE TABLE rejections (
        proposal BLOB NOT NULL,
        slot INTEGER NOT NULL,
        body BLOB NOT NULL,
        created_at INTEGER NOT NULL,
        PRIMARY KEY (proposal, slot)
    );
    CREATE TABLE receipts (
        proposal BLOB PRIMARY KEY,
        vault BLOB NOT NULL,
        tx_hash BLOB NOT NULL,
        created_at INTEGER NOT NULL
    );
    CREATE INDEX receipts_by_tx ON receipts (tx_hash);
    CREATE TABLE lobbies (
        lobby BLOB PRIMARY KEY,
        role TEXT NOT NULL CHECK (role IN ('host', 'guest')),
        status TEXT NOT NULL CHECK (status IN ('open', 'started', 'closed')),
        record TEXT NOT NULL,
        updated_at INTEGER NOT NULL
    );
    ",
    // 3: the member's paired signing devices (public keys only).
    "
    CREATE TABLE devices (
        identity BLOB PRIMARY KEY,
        transport BLOB NOT NULL UNIQUE,
        signer_key BLOB NOT NULL,
        name TEXT NOT NULL,
        paired_at INTEGER NOT NULL
    );
    ",
    // 2: contract artifacts, by class, in chunks.
    "
    CREATE TABLE artifacts (
        class_id BLOB PRIMARY KEY,
        count INTEGER NOT NULL,
        hashes BLOB,
        complete INTEGER NOT NULL DEFAULT 0,
        created_at INTEGER NOT NULL
    );
    CREATE TABLE artifact_chunks (
        class_id BLOB NOT NULL,
        idx INTEGER NOT NULL,
        bytes BLOB NOT NULL,
        PRIMARY KEY (class_id, idx)
    );
    ",
    // 5: receipts keep their signed body, which names the executing member's keeper.
    "
    ALTER TABLE receipts ADD COLUMN body BLOB;
    ",
];
