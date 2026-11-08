use ome_wire::IdentityPublic;

/// Keeper errors. Messages never contain secret material.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The data directory is missing, unreadable or not initialised.
    #[error("data directory: {0}")]
    DataDir(String),
    /// Wrong master key or passphrase, or a sealed file was modified or swapped.
    #[error("cannot unseal {0}: wrong master key or tampered file")]
    Unseal(String),
    /// A secret file is missing.
    #[error("missing secret {0}")]
    MissingSecret(String),
    /// Local storage failed.
    #[error("storage: {0}")]
    Storage(String),
    /// The vault is not held by this keeper.
    #[error("unknown vault")]
    UnknownVault,
    /// The ceremony session is unknown.
    #[error("unknown ceremony")]
    UnknownCeremony,
    /// The request names another committee epoch.
    #[error("committee epoch {actual} is not the current epoch {expected}")]
    WrongEpoch {
        /// Current epoch.
        expected: u32,
        /// Requested epoch.
        actual: u32,
    },
    /// The requester is not a current committee member.
    #[error("{0:?} is not a member of the committee")]
    NotMember(IdentityPublic),
    /// The requester exceeded its ECDH budget.
    #[error("rate limit exceeded")]
    RateLimited,
    /// Not enough members answered in time.
    #[error("only {received} of {needed} members answered")]
    Unavailable {
        /// Members needed.
        needed: usize,
        /// Members that answered with valid partials.
        received: usize,
    },
    /// Invalid input: malformed points, an inconsistent manifest, bad parameters.
    #[error("invalid: {0}")]
    Invalid(String),
    /// A verified rotation removed this keeper from the vault's committee.
    #[error("this keeper was removed from the vault's committee")]
    Removed,
    /// A ceremony with this session id already exists.
    #[error("ceremony already exists")]
    CeremonyExists,
    /// A peer did not acknowledge in time.
    #[error("transport: {0}")]
    Transport(String),
    /// The network layer failed (relay mailboxes, the node).
    #[error(transparent)]
    Net(#[from] ome_net::Error),
    /// A cryptographic check failed.
    #[error(transparent)]
    Crypto(#[from] ome_crypto::Error),
    /// A wire value is malformed.
    #[error(transparent)]
    Wire(#[from] ome_wire::Error),
    /// A vault rule refused the manifest, config or rotation.
    #[error(transparent)]
    Protocol(#[from] ome_protocol::Error),
    /// Filesystem failure.
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

impl From<rusqlite::Error> for Error {
    fn from(error: rusqlite::Error) -> Self {
        Self::Storage(error.to_string())
    }
}

/// Keeper result.
pub type Result<T, E = Error> = std::result::Result<T, E>;
