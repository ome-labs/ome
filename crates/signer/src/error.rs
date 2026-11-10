/// Why the signer refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /// The sealed key does not open with this secret, or was modified.
    #[error("wrong passphrase or key, or the sealed key was modified")]
    WrongSecret,
    /// A passphrase was given for a key sealed under a platform key, or the reverse.
    #[error("this key is sealed under a {expected}")]
    WrongSecretKind {
        /// What the sealed key needs.
        expected: &'static str,
    },
    /// The sealed key uses a format this build does not read.
    #[error("unsupported sealed key version {0:?}")]
    UnsupportedVersion(String),
    /// The sealed key asks for key-derivation parameters weaker than the minimum, or absurdly expensive ones.
    #[error("key derivation parameters out of range: {0}")]
    KdfOutOfRange(&'static str),
    /// The sealed key is not valid JSON of the expected shape.
    #[error("malformed sealed key: {0}")]
    Malformed(&'static str),
    /// Key derivation failed.
    #[error("key derivation failed")]
    Kdf,
    /// This key is not one of the vault's signers.
    #[error("this key is not a signer of the vault")]
    NotASigner,
    /// The payload calls `rotate_membership`; approve it with `approve_rotation` so its shares are checked.
    #[error("this proposal rotates the vault's membership; review it as a rotation")]
    RotationNeedsReview,
    /// `approve_rotation` was given a proposal without exactly one `rotate_membership` call.
    #[error("this proposal is not a single rotate_membership call")]
    NotARotation,
    /// A rotation proposal carries a call to this contract (hex), which is neither the rotation nor a trusted fee payer.
    #[error("a rotation may only also pay its fee; it calls 0x{0}")]
    UnexpectedCall(String),
    /// A rotation proposal also grants an authwit, which no rotation needs.
    #[error("a rotation grants no authwit")]
    RotationGrantsAuthwit,
    /// The proposal performs a different rotation than the one reviewed.
    #[error("the proposal performs a different rotation than the one reviewed")]
    RotationMismatch,
    /// A rejection reason is longer than the wire allows.
    #[error("rejection reason longer than {0} bytes")]
    ReasonTooLong(usize),
    /// A vault rule refused the proposal or rotation.
    #[error(transparent)]
    Protocol(#[from] ome_protocol::Error),
    /// A cryptographic operation failed.
    #[error(transparent)]
    Crypto(#[from] ome_crypto::Error),
}

impl From<ome_wire::Error> for Error {
    fn from(error: ome_wire::Error) -> Self {
        Self::Protocol(error.into())
    }
}
