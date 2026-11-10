//! What a review and a dry run tell a member before anyone signs: the transaction's effects on the vault, failures as
//! kinds with their numbers, and governance changes as before and after.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{DecodedArg, TokenInfo};
use crate::{Amount, Hex32};

/// Whether a balance is private (notes) or public (storage).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum Visibility {
    /// Private notes.
    Private,
    /// Public storage.
    Public,
}

/// A tracked token's balance in the vault, before and after the transaction.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct BalanceChange {
    /// The token.
    pub token: TokenInfo,
    /// Before, in base units.
    pub before: Amount,
    /// After, in base units.
    pub after: Amount,
    /// Which balance.
    pub visibility: Visibility,
}

/// What one call returns.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct CallResult {
    /// Index into the transaction's calls.
    pub call: u32,
    /// Its return values, decoded when the member has the contract's artifact.
    pub returns: Vec<DecodedArg>,
}

/// Notes the transaction creates and spends, against Aztec's per-transaction limits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct NoteCounts {
    /// Note hashes created.
    pub created: u32,
    /// Nullifiers emitted.
    pub nullified: u32,
}

/// What a simulation shows the transaction does to the vault.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Effects {
    /// Tracked tokens whose vault balance changes.
    pub balances: Vec<BalanceChange>,
    /// Calls that return something.
    pub returns: Vec<CallResult>,
    /// Notes created and spent.
    pub notes: NoteCounts,
    /// False when some change to the vault couldn't be attributed (a note of an untracked contract, or of a shape the
    /// daemon doesn't read): the transaction may change other balances.
    pub complete: bool,
}

/// Why a review's checks failed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ReviewFailure {
    /// The payload rebuilt here doesn't hash to the proposal's outer hash.
    PayloadMismatch,
    /// The vault's config changed since the proposal was made.
    ConfigChanged {
        /// Config version in the proposal.
        made: u32,
        /// The vault's current config version.
        current: u32,
    },
    /// The deadline passed.
    Expired {
        /// Unix seconds.
        at: u64,
    },
    /// A contract's code is still on its way from the member who has it.
    ArtifactMissing {
        /// The contract class.
        class_id: Option<Hex32>,
    },
    /// The proposer isn't a member.
    NotMember,
    /// The proposal would make the vault pay fees in public.
    FeeOnVault,
    /// Anything else the checks refused.
    Invalid,
}

/// A review failure with its message.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ReviewError {
    /// What failed.
    pub failure: ReviewFailure,
    /// In words, for people.
    pub message: String,
}

/// Why a simulation failed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum SimulationFailure {
    /// The vault holds less of a token than a call moves.
    InsufficientBalance {
        /// The token, when tracked.
        token: Option<TokenInfo>,
        /// The token contract.
        contract: Hex32,
    },
    /// A contract asked the vault for an authwit the proposal doesn't carry.
    AuthwitRequired {
        /// The contract that asked.
        contract: Option<Hex32>,
    },
    /// The proposal grants an authwit the transaction never uses: no contract asked the vault about it.
    UnusedAuthwit {
        /// Its position in the proposal's authwits.
        index: u32,
        /// The contract that would have consumed it.
        contract: Hex32,
    },
    /// The proposal's nonce is spent: it was executed or cancelled.
    NonceUsed,
    /// A call reverted.
    Reverted {
        /// The contract, when known.
        contract: Option<Hex32>,
        /// The function, when known.
        function: Option<String>,
    },
    /// The simulation couldn't run: node, keepers or prover not ready.
    Unavailable,
    /// A failure a newer release reports; its message says what.
    #[serde(other)]
    Unknown,
}

/// A simulation failure with its message.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct SimulationError {
    /// What failed.
    pub failure: SimulationFailure,
    /// In words, for people; for `reverted`, the contract's own message.
    pub message: String,
}

/// A threshold choice members should make knowingly (`ome-protocol`'s policy warnings).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum PolicyWarning {
    /// Any one member can spend alone.
    SingleSigner,
    /// Every member must approve: one lost key stops spending for good.
    NoSpareSigner,
    /// Any two members can read what arrived while they were members.
    TwoCanRead,
    /// Every keeper is needed to read: losing one makes the vault unreadable for good.
    NoSpareViewer,
}

/// A change to the vault's own policy, as a review shows it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum GovernanceChange {
    /// Approvals to spend.
    Threshold {
        /// Now.
        before: u32,
        /// After.
        after: u32,
        /// Members.
        members: u32,
        /// Risks of the new shape.
        warnings: Vec<PolicyWarning>,
    },
    /// Every approval collected so far stops counting.
    InvalidatePending {
        /// Config version now.
        version: u32,
    },
}
