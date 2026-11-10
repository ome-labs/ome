use ome_crypto::threshold::shamir::MemberIndex;

/// Why a vault rule refused something. Each variant names what callers need to show or act on.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /// A signer set or committee has no members or more than the vault allows.
    #[error("member count out of range: {0}")]
    MemberCount(usize),
    /// The same key appears twice in a signer set or committee.
    #[error("duplicate key in slot {0}")]
    DuplicateKey(usize),
    /// The spending threshold is outside `1..=n`.
    #[error("spending threshold {threshold} out of range for {members} members")]
    Threshold {
        /// Requested threshold.
        threshold: u32,
        /// Member count.
        members: u32,
    },
    /// The viewing threshold is outside `2..=n`.
    #[error("viewing threshold {threshold} out of range for {members} members")]
    ViewingThreshold {
        /// Requested threshold.
        threshold: u32,
        /// Member count.
        members: u32,
    },
    /// A rotation removes at least the current viewing threshold of members.
    #[error("{removed} members removed; at most {allowed} may leave in one rotation")]
    TooManyRemoved {
        /// Members of the current set absent from the new one.
        removed: u32,
        /// `t_v - 1`.
        allowed: u32,
    },
    /// The keys do not match the stored signer-set commitment.
    #[error("signer set does not match the vault's config")]
    SignerSetMismatch,
    /// The verification shares or viewing threshold do not match the stored committee commitment.
    #[error("viewing committee does not match the vault's config")]
    CommitteeMismatch,
    /// A manifest field disagrees with the vault's config.
    #[error("manifest {0} does not match the vault's config")]
    ManifestMismatch(&'static str),
    /// The verification shares do not lie on one polynomial of degree `t_v - 1`.
    #[error("verification share of member {0} is not on the committee's polynomial")]
    ShareOffPolynomial(MemberIndex),
    /// The verification shares interpolate to another key than the vault's.
    #[error("verification shares do not belong to the vault's viewing key")]
    GroupKeyMismatch,
    /// A rotation reuses the current committee's shares, so removed members could still decrypt.
    #[error("rotation keeps the old viewing shares; the committee must be reshared")]
    SharesNotRefreshed,
    /// A call cannot be part of a vault transaction.
    #[error("call {index} is invalid: {reason}")]
    InvalidCall {
        /// Position of the call.
        index: usize,
        /// What is wrong with it.
        reason: &'static str,
    },
    /// A `rotate_membership` call's arguments are not a rotation of this vault.
    #[error("rotate_membership call is invalid: {0}")]
    InvalidRotationCall(&'static str),
    /// A rotation is not a signer swap: it changes more than one signer key.
    #[error("not a signer swap: {0}")]
    NotASignerSwap(&'static str),
    /// No call of an executed proposal sets this vault's threshold.
    #[error("no call sets this vault's threshold")]
    NotAThresholdChange,
    /// More calls than Aztec's account entrypoint executes.
    #[error("{0} calls; an entrypoint executes at most 5")]
    TooManyCalls(usize),
    /// A call's arguments do not split by its function signature.
    #[error("cannot decode the call's arguments: {0}")]
    UndecodableArguments(&'static str),
    /// A function signature contains whitespace.
    #[error("function signature contains whitespace")]
    InvalidSignatureText,
    /// The payload is not the one the proposal references.
    #[error("payload does not match the proposal")]
    PayloadMismatch,
    /// The payload's calls do not hash to the proposal's outer hash.
    #[error("payload does not hash to the proposed outer hash")]
    OuterHashMismatch,
    /// The proposal was made under another config version.
    #[error("proposed under config version {proposed}; the vault is at {current}")]
    StaleConfig {
        /// Version in the proposal.
        proposed: u32,
        /// The vault's current version.
        current: u32,
    },
    /// The approval deadline has passed.
    #[error("approval deadline has passed")]
    Expired {
        /// The deadline, Unix seconds.
        at: u64,
    },
    /// The proposal would make the vault pay fees publicly.
    #[error("fee method {0} pays fees from the vault in public")]
    PublicFeePayment(u8),
    /// No member has this index or signer slot.
    #[error("no member in slot {0}")]
    UnknownSlot(u32),
    /// An approval or rejection is for another proposal.
    #[error("vote is for another proposal")]
    WrongProposal,
    /// An approval or rejection signature does not verify for its slot.
    #[error("invalid signature for slot {0}")]
    InvalidSignature(u32),
    /// A join proof does not match the invite and the joiner's keys.
    #[error("join proof does not verify")]
    InvalidJoinProof,
    /// A lobby message's signer is not the member it describes.
    #[error("joiner is not the message signer")]
    UnexpectedSigner,
    /// A wire value is malformed or out of bounds.
    #[error(transparent)]
    Wire(#[from] ome_wire::Error),
    /// A cryptographic value is invalid.
    #[error(transparent)]
    Crypto(#[from] ome_crypto::Error),
}
