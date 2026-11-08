/// Why a delivery or a node operation failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /// The network failed to deliver (dial, stream or timeout).
    #[error("transport: {0}")]
    Transport(String),
    /// An address or key is not usable.
    #[error("invalid: {0}")]
    Invalid(String),
    /// The peer is not one this node may talk to.
    #[error("peer is not admitted")]
    NotAdmitted,
    /// The peer is not reachable.
    #[error("peer unreachable")]
    Unreachable,
    /// The node was stopped.
    #[error("node stopped")]
    Stopped,
    /// No relay stored an envelope.
    #[error("no relay stored the envelope")]
    NotStored,
    /// A relay answered with bytes that are not a mailbox response.
    #[error(transparent)]
    Wire(#[from] ome_wire::Error),
}

pub(crate) fn transport_error(error: impl std::fmt::Display) -> Error {
    Error::Transport(error.to_string())
}
