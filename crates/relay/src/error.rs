/// Relay errors.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Storage failure.
    #[error("storage: {0}")]
    Storage(String),
    /// Invalid configuration or input.
    #[error("invalid: {0}")]
    Invalid(String),
    /// Network failure.
    #[error("network: {0}")]
    Network(String),
    /// Filesystem failure.
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

impl From<rusqlite::Error> for Error {
    fn from(error: rusqlite::Error) -> Self {
        Self::Storage(error.to_string())
    }
}

/// Relay result.
pub type Result<T, E = Error> = std::result::Result<T, E>;
