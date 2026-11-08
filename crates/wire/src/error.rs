/// Errors for malformed, oversized, unauthentic, replayed or incompatible messages.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /// The encoding is larger than its limit.
    #[error("message too large: {0} bytes")]
    TooLarge(usize),
    /// The bytes are not the canonical encoding of a message.
    #[error("malformed message")]
    Malformed,
    /// The message uses a protocol version this build does not speak.
    #[error("unsupported protocol version {0}")]
    UnsupportedVersion(u16),
    /// No protocol version is supported by every party.
    #[error("no protocol version is supported by every party")]
    NoCommonVersion,
    /// A field exceeds its bound (count, length or range).
    #[error("{field} out of bounds")]
    OutOfBounds {
        /// The offending field.
        field: &'static str,
    },
    /// The signature does not verify for the claimed signer.
    #[error("invalid signature")]
    InvalidSignature,
    /// The signer is not who the receiver expected.
    #[error("unexpected signer")]
    UnexpectedSigner,
    /// The message id was already seen from this signer.
    #[error("replayed message")]
    Replayed,
    /// The message belongs to another epoch.
    #[error("wrong epoch: expected {expected}, got {actual}")]
    WrongEpoch {
        /// Receiver's current epoch.
        expected: u32,
        /// Message epoch.
        actual: u32,
    },
    /// The message timestamp is outside the accepted window.
    #[error("stale or future message")]
    Stale,
    /// A cryptographic value inside the message is invalid.
    #[error("invalid content: {0}")]
    Crypto(#[from] ome_crypto::Error),
}
