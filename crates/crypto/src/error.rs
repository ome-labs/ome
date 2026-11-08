/// Errors returned for invalid or adversarial input. Library code never panics on untrusted data.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /// Bytes or coordinates do not describe a point on Grumpkin.
    #[error("point is not on the Grumpkin curve")]
    PointNotOnCurve,
    /// The identity is never a valid key, share commitment or ECDH input.
    #[error("point is the identity")]
    IdentityPoint,
    /// A field element encoding is not canonical (not below the modulus).
    #[error("non-canonical field element encoding")]
    NonCanonicalEncoding,
    /// More members than the vault supports.
    #[error("too many members: at most {max}, got {actual}")]
    TooManyMembers {
        /// Maximum number of members.
        max: usize,
        /// Actual number.
        actual: usize,
    },
    /// Member indices start at one.
    #[error("invalid member index")]
    InvalidIndex,
    /// The same member index appears twice.
    #[error("duplicate member index {0}")]
    DuplicateIndex(u32),
    /// A threshold must be at least one and at most the number of members.
    #[error("invalid threshold")]
    InvalidThreshold,
    /// Fewer shares or partials than the threshold.
    #[error("insufficient shares: need {needed}, got {actual}")]
    InsufficientShares {
        /// Threshold.
        needed: usize,
        /// Provided.
        actual: usize,
    },
    /// A member's DLEQ proof (or other proof) does not verify.
    #[error("invalid proof from member {0}")]
    InvalidProof(u32),
    /// A dealer's commitments have the wrong number of terms for the agreed threshold.
    #[error("dealer {0} committed to a polynomial of the wrong degree")]
    InvalidCommitments(u32),
    /// A dealer's share does not match its Feldman commitments (raise a complaint against the dealer).
    #[error("invalid share from dealer {0}")]
    InvalidShare(u32),
    /// A reshare's new commitments do not reconstruct the unchanged group key.
    #[error("reshare does not preserve the group key")]
    GroupKeyMismatch,
    /// A transport key or encapsulated key is malformed.
    #[error("invalid envelope key")]
    EnvelopeKey,
    /// Sealing an envelope failed.
    #[error("sealing failed")]
    EnvelopeSeal,
    /// The envelope is not authentic for this sender, recipient and context.
    #[error("envelope does not open")]
    EnvelopeOpen,
    /// Input has the wrong length.
    #[error("invalid length: expected {expected}, got {actual}")]
    InvalidLength {
        /// Expected length.
        expected: usize,
        /// Actual length.
        actual: usize,
    },
}
