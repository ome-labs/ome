//! Proposals: what a member asks the vault to do, how members review and vote, and execution.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{
    AbiValue, DecodedArg, Effects, FeeEstimate, FunctionKind, GovernanceChange, MemberRef,
    ReviewError, Secret, SimulationError, TokenInfo,
};
use crate::{Amount, Hex32};

/// One call of a `calls` intent.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct IntentCall {
    /// Contract.
    pub to: Hex32,
    /// Function name.
    #[serde(rename = "fn")]
    pub function: String,
    /// Arguments, one per parameter, each checked against its type (`contracts.functions`).
    pub args: Vec<AbiValue>,
}

/// A private authwit a `calls` intent grants: `caller`, one of the intent's call targets, may make `call` on the
/// vault's behalf inside the transaction (a swap pulling the vault's tokens, say).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct IntentAuthwit {
    /// The contract allowed to make the call.
    pub caller: Hex32,
    /// The private call it may make; its contract consumes the authwit.
    pub call: IntentCall,
}

/// What a proposal is for. Carried as JSON in the proposal's summary (at most 4 KiB).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Intent {
    /// A private token transfer.
    Transfer {
        /// Token.
        token: Hex32,
        /// Recipient.
        to: Hex32,
        /// Amount.
        amount: Amount,
        /// Note for the members, at most 280 characters.
        memo: Option<String>,
    },
    /// Up to `daemon.info.limits.maxCalls` contract calls.
    Calls {
        /// Calls.
        calls: Vec<IntentCall>,
        /// Private authwits granted with the calls, at most `daemon.info.limits.maxAuthwits`.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        #[ts(optional, as = "Option<Vec<IntentAuthwit>>")]
        authwits: Vec<IntentAuthwit>,
        /// Note.
        memo: Option<String>,
    },
    /// Cancels a proposal by spending its nonce.
    Cancel {
        /// The proposal cancelled.
        cancels: Hex32,
        /// Note.
        memo: Option<String>,
    },
    /// Changes the approvals needed to spend (the vault's own `set_threshold`).
    SetThreshold {
        /// The new threshold, from 1 to the member count.
        threshold: u32,
        /// Note.
        memo: Option<String>,
    },
    /// Voids every approval collected so far (the vault's own `invalidate_pending`).
    InvalidatePending {
        /// Note.
        memo: Option<String>,
    },
}

/// Where a proposal is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum ProposalStatus {
    /// Gathering approvals.
    Collecting,
    /// Enough approvals to execute.
    Ready,
    /// Enough rejections that it cannot pass.
    Blocked,
    /// Executed.
    Executed,
    /// Its nonce was spent by a cancel.
    Cancelled,
    /// Past its expiry.
    Expired,
    /// Made under an older config.
    Stale,
}

/// A member's approval or rejection.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Vote {
    /// Signer slot, 0-based.
    pub slot: u32,
    /// Member index.
    pub index: u32,
    /// Name.
    pub name: String,
    /// An approval, or a rejection.
    pub approve: bool,
    /// Whether the signature verifies.
    pub valid: bool,
    /// Reason, for rejections.
    pub reason: Option<String>,
    /// Unix seconds this member's keeper first stored it.
    pub at: u64,
}

/// A call as the payload encodes it, decoded with the contract's artifact when this member has it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DecodedCall {
    /// Contract.
    pub to: Hex32,
    /// Its label: a token, the vault, or an aztec-wallet alias.
    pub to_label: Option<String>,
    /// Function.
    #[serde(rename = "fn")]
    pub function: String,
    /// Selector, `0x` and 8 hex characters.
    pub selector: String,
    /// Where it runs.
    pub kind: FunctionKind,
    /// Arguments, named and typed when `decoded`.
    pub args: Vec<DecodedArg>,
    /// Static call.
    pub is_static: bool,
    /// Whether the arguments were decoded with the contract's artifact; until it arrives they are raw fields.
    pub decoded: bool,
}

/// A private authwit a proposal grants, decoded from its payload.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DecodedAuthwit {
    /// The contract allowed to make the call.
    pub caller: Hex32,
    /// Its label, as for a call's target.
    pub caller_label: Option<String>,
    /// The call it may make.
    pub call: DecodedCall,
    /// The authwit's message hash, which the vault approves.
    pub message_hash: Hex32,
    /// The message an approval signs for it.
    pub approval_message: Hex32,
}

/// The proposer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Proposer {
    /// Member index.
    pub index: u32,
    /// Name.
    pub name: String,
}

/// An execution.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Execution {
    /// Transaction.
    pub tx_hash: Hex32,
    /// Unix seconds.
    pub at: u64,
    /// The member who executed it, from the receipt their keeper signed; `None` when they are no longer a member.
    pub executor: Option<MemberRef>,
    /// Block that included it.
    pub block: Option<u64>,
}

/// A member executing a proposal right now.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionView {
    /// Who.
    pub member: MemberRef,
    /// How far they are.
    pub phase: ExecutionPhase,
    /// Unix seconds they started.
    pub since: u64,
    /// Whether it is this member.
    pub mine: bool,
}

/// A proposal as this member sees it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ProposalView {
    /// Proposal id.
    pub id: Hex32,
    /// Vault.
    pub vault: Hex32,
    /// What it is for.
    pub intent: Intent,
    /// Who proposed it.
    pub proposer: Proposer,
    /// Where it is.
    pub status: ProposalStatus,
    /// Spending threshold.
    pub threshold: u32,
    /// Members.
    pub members: u32,
    /// Approvals, then valid rejections from slots that have not also approved, each with its time.
    pub votes: Vec<Vote>,
    /// This member's slot.
    pub my_slot: u32,
    /// Whether this member approved.
    pub approved_by_me: bool,
    /// Whether this member rejected.
    pub rejected_by_me: bool,
    /// Config version it was made under.
    pub config_version: u32,
    /// The vault's current config version.
    pub current_config_version: u32,
    /// Unix seconds the proposer made it (the clock `expires_at` uses).
    pub created_at: u64,
    /// Unix seconds it expires.
    pub expires_at: u64,
    /// Transaction nonce.
    pub tx_nonce: Hex32,
    /// Outer hash the approvals sign over.
    pub outer_hash: Hex32,
    /// The message an approval signs.
    pub approval_message: Hex32,
    /// How fees are paid.
    pub fee_method: String,
    /// Decoded calls.
    pub calls: Vec<DecodedCall>,
    /// The private authwits it grants, each signed with the transaction.
    pub authwits: Vec<DecodedAuthwit>,
    /// For transfers, the token as this member knows it.
    pub token: Option<TokenInfo>,
    /// The execution, once executed.
    pub execution: Option<Execution>,
    /// A member executing it right now.
    pub executing: Option<ExecutionView>,
    /// The cancel proposal targeting it, or the proposal a cancel targets.
    pub cancelled_by: Option<Hex32>,
}

impl ProposalView {
    /// The approvals among `votes`.
    pub fn approvals(&self) -> impl Iterator<Item = &Vote> {
        self.votes.iter().filter(|v| v.approve)
    }

    /// The rejections among `votes`.
    pub fn rejections(&self) -> impl Iterator<Item = &Vote> {
        self.votes.iter().filter(|v| !v.approve)
    }
}

/// Gas a simulation used.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Gas {
    /// DA gas.
    pub da: u64,
    /// L2 gas.
    pub l2: u64,
}

/// Checks a review ran on this device.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ReviewChecks {
    /// The payload was rebuilt from the intent and matches.
    pub rebuilt: bool,
    /// The recomputed outer hash.
    pub outer_hash: Hex32,
    /// Made under the current config.
    pub config_current: bool,
    /// Not expired.
    pub not_expired: bool,
    /// What failed, if anything.
    pub failure: Option<ReviewError>,
}

/// Where a recipient's label came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum LabelSource {
    /// An alias in `aztec-wallet`'s store (`accounts:<name>`), which the member set and nobody verified.
    AztecWallet,
    /// A vault member.
    Member,
}

/// A transfer's recipient.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Recipient {
    /// Address.
    pub address: Hex32,
    /// Label, if known.
    pub label: Option<String>,
    /// Where the label came from.
    pub source: Option<LabelSource>,
}

/// A simulation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Simulation {
    /// Succeeded.
    pub ok: bool,
    /// Gas used.
    pub gas: Option<Gas>,
    /// Why it failed.
    pub error: Option<SimulationError>,
    /// What it does to the vault, when it succeeded.
    pub effects: Option<Effects>,
    /// What it is expected to cost, when it succeeded.
    pub fee: Option<FeeEstimate>,
}

/// What a signature covers.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Signs {
    /// The message.
    pub message: Hex32,
    /// How it is computed.
    pub formula: String,
}

/// What `ome approve` shows before the member signs, all rebuilt and checked on this device.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Review {
    /// The proposal.
    pub proposal: ProposalView,
    /// Vault name.
    pub vault_name: String,
    /// This member's name.
    pub my_name: String,
    /// Checks.
    pub checks: ReviewChecks,
    /// One line, e.g. "transfer 25 JADE (25000000000000000000 base units)".
    pub action: String,
    /// For transfers.
    pub recipient: Option<Recipient>,
    /// For governance intents: the vault's policy before and after.
    pub change: Option<GovernanceChange>,
    /// Simulation.
    pub simulation: Simulation,
    /// What an approval signs.
    pub signs: Signs,
    /// What a rejection signs.
    pub rejects: Signs,
}

/// A dry run of a proposal before it is made.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ProposalPreview {
    /// Succeeded.
    pub ok: bool,
    /// Calls it would make.
    pub calls: Vec<DecodedCall>,
    /// Authwits it would grant.
    pub authwits: Vec<DecodedAuthwit>,
    /// Gas used.
    pub gas: Option<Gas>,
    /// Why it failed.
    pub error: Option<SimulationError>,
    /// What it would do to the vault, when it succeeded.
    pub effects: Option<Effects>,
    /// What it is expected to cost, when it succeeded.
    pub fee: Option<FeeEstimate>,
}

/// Params of `proposals.execute`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ExecuteParams {
    /// Vault.
    pub vault: Hex32,
    /// Proposal id.
    pub id: Hex32,
    /// Execute even while another member is: both prove, and the proposal's nonce lets one land.
    #[ts(optional)]
    pub force: Option<bool>,
}

/// Params naming a proposal.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ProposalRef {
    /// Vault.
    pub vault: Hex32,
    /// Proposal id.
    pub id: Hex32,
}

/// Params of `proposals.preview` and `proposals.create`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ProposeParams {
    /// Vault.
    pub vault: Hex32,
    /// What to do.
    pub intent: Intent,
    /// Validity, 1 to 720 hours.
    pub expires_in_hours: u32,
}

/// Params of `proposals.approve`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ApproveParams {
    /// Vault.
    pub vault: Hex32,
    /// Proposal id.
    pub id: Hex32,
    /// Unseals the signer for this call when it is locked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub secret: Option<Secret>,
}

/// Params of `proposals.reject`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct RejectParams {
    /// Vault.
    pub vault: Hex32,
    /// Proposal id.
    pub id: Hex32,
    /// Reason shown to the members, at most 280 characters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub reason: Option<String>,
    /// Unseals the signer for this call when it is locked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub secret: Option<Secret>,
}

/// Where an execution on this device is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum ExecutionPhase {
    /// Rebuilding the transaction.
    Rebuilding,
    /// Proving.
    Proving,
    /// Sending.
    Sending,
    /// Included in a block.
    Mined,
    /// Failed.
    Failed,
}

/// Progress of an execution on this device.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionProgress {
    /// Vault.
    pub vault: Hex32,
    /// Proposal.
    pub proposal: Hex32,
    /// Phase.
    pub phase: ExecutionPhase,
    /// Why it failed.
    pub error: Option<String>,
    /// Transaction, once sent.
    pub tx_hash: Option<Hex32>,
}
