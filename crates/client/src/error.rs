//! What a call can fail with.

use std::path::PathBuf;

use ome_api::{ErrorKind, RpcError};

/// A client failure: the service's own error, or the connection to it.
#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    /// The service is not listening.
    #[error("cannot reach {}: {source}", path.display())]
    Connect {
        /// Socket or pipe.
        path: PathBuf,
        /// Why.
        source: std::io::Error,
    },
    /// The service closed the connection.
    #[error("the connection closed")]
    Closed,
    /// The service answered with an error.
    #[error("{0}")]
    Rpc(RpcError),
    /// The service answered with something that is not the method's result.
    #[error("unexpected reply: {0}")]
    Protocol(String),
    /// The service's address or token could not be found.
    #[error("{0}")]
    Discovery(String),
    /// The event stream fell behind and dropped this many events.
    #[error("missed {0} events")]
    Lagged(u64),
    /// Reading or writing the connection failed.
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

impl ClientError {
    /// The API error kind, so callers handle a local failure the way they would the service's.
    #[must_use]
    pub fn kind(&self) -> ErrorKind {
        match self {
            Self::Rpc(error) => error.kind(),
            Self::Connect { .. } | Self::Closed | Self::Discovery(_) => ErrorKind::NotReady,
            Self::Protocol(_) | Self::Lagged(_) | Self::Io(_) => ErrorKind::Failed,
        }
    }
}

/// Client result.
pub type Result<T, E = ClientError> = std::result::Result<T, E>;
