//! The shapes only the bindings use. Shared shapes (`Hex32`, `Point`, `Manifest`, `VaultConfig`, `MemberPartials`,
//! `Secret`) are ome-api's, imported from `@ome-labs/api` in the declarations.

use ome_api::{
    Hex32, Hex64, HexBytes, Point,
    keeper::{Manifest, MemberPartials, VaultConfig},
};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// A vault as the bindings check proposals against it: its committee manifest and the config note read from the
/// chain.
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
pub struct VaultInput {
    /// The committee manifest the keeper serves.
    pub manifest: Manifest,
    /// The vault's config note.
    pub config: VaultConfig,
}

/// The chain a transaction is bound to, from the node.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Chain {
    /// L1 chain id, as a field.
    pub chain_id: Hex32,
    /// Rollup version, as a field.
    pub version: Hex32,
}

/// How a viewing share maps to a share of the address secret.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AddressContext {
    /// The vault address.
    pub address: Hex32,
    /// `1` or `-1`: the sign Aztec applies to the address secret.
    pub sign: i8,
    /// `preaddress · G + Ivpk`, normalised to positive `y`.
    pub address_point: Point,
}

/// A threshold ECDH batch to verify and combine: the keeper's `ecdh.batch` responses with what they answer.
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CombineRequest {
    /// Committee epoch the partials are bound to.
    pub epoch: u32,
    /// Vault preaddress.
    pub preaddress: Hex32,
    /// Group incoming viewing key `Ivpk`.
    pub ivpk: Point,
    /// Verification shares `Y_i` in member order.
    pub verification_shares: Vec<Point>,
    /// Viewing threshold `t_v`.
    pub threshold: u32,
    /// Input points, in the order the partials answer them.
    pub inputs: Vec<Point>,
    /// Members' partials.
    pub responses: Vec<MemberPartials>,
}

/// Combined points.
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CombineResult {
    /// `address_secret · input` for each input, in order.
    pub points: Vec<Point>,
    /// Members whose response was malformed or carried an invalid proof.
    pub invalid_members: Vec<u32>,
}

/// Where a call runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum CallKind {
    /// A private function call.
    Private,
    /// An enqueued public call.
    Public,
}

/// One call of a vault transaction.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Call {
    /// The function signature, when known; it must hash to `selector`.
    pub signature: Option<String>,
    /// Target contract.
    pub to: Hex32,
    /// Function selector.
    pub selector: u32,
    /// Private or public.
    pub kind: CallKind,
    /// The call may not modify state.
    pub is_static: bool,
    /// Hide the vault as `msg_sender` (public calls only).
    pub hide_msg_sender: bool,
    /// Encoded arguments.
    pub args: Vec<Hex32>,
}

/// The calls a payload carries, and the PXE's execution data.
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
pub struct ExecutionRequest {
    /// At most five calls; none is a cancellation.
    pub calls: Vec<Call>,
    /// Execution data the signatures do not cover.
    pub extras: HexBytes,
}

/// What a member proposes.
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Draft {
    /// The calls.
    pub calls: Vec<Call>,
    /// Execution data the signatures do not cover.
    pub extras: HexBytes,
    /// Entrypoint nonce; cancelling burns it.
    pub tx_nonce: Hex32,
    /// Approval deadline, Unix seconds.
    pub expires_at: u64,
    /// Summary shown to the members.
    pub summary: String,
    /// Proposing member (1-based).
    pub proposer: u32,
}

/// A proposal ready to store with the keeper (`proposals.put`, `payloads.put`).
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct BuiltProposal {
    /// Proposal id.
    pub id: Hex32,
    /// The proposal's canonical encoding.
    pub proposal: HexBytes,
    /// The payload's canonical encoding.
    pub payload: HexBytes,
    /// Outer hash the approvals sign over.
    pub outer_hash: Hex32,
    /// The message an approval signs.
    pub approval_message: Hex32,
}

/// A decoded proposal.
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ProposalInfo {
    /// Proposal id.
    pub id: Hex32,
    /// Config version it was made under.
    pub config_version: u32,
    /// Outer hash.
    pub outer_hash: Hex32,
    /// Deadline, Unix seconds.
    pub expires_at: u64,
    /// Entrypoint nonce.
    pub tx_nonce: Hex32,
    /// Fee method; Ome proposals use only `0` (external).
    pub fee_method: u8,
    /// Hash of the referenced payload.
    pub payload_hash: Hex32,
    /// Length of the referenced payload.
    pub payload_len: u32,
    /// Summary.
    pub summary: String,
    /// Proposing member.
    pub proposer: u32,
}

/// A decoded approval.
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
pub struct ApprovalInfo {
    /// Proposal id.
    pub proposal: Hex32,
    /// Signer slot, 0-based.
    pub slot: u32,
    /// Schnorr signature.
    pub signature: Hex64,
}

/// A decoded rejection.
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
pub struct RejectionInfo {
    /// Proposal id.
    pub proposal: Hex32,
    /// Signer slot, 0-based.
    pub slot: u32,
    /// Reason shown to the members.
    pub reason: String,
    /// Schnorr signature.
    pub signature: Hex64,
}

/// Where a proposal stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum TallyStatus {
    /// Collecting votes.
    Pending,
    /// At least `k` approvals: the witness is ready.
    Approved,
    /// Too many rejections for `k` approvals.
    Blocked,
    /// Past its deadline.
    Expired,
}

/// A vote the tally refused.
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
pub struct RefusedVote {
    /// Position in the approvals or rejections given.
    pub index: u32,
    /// Whether it was an approval.
    pub approval: bool,
    /// Why.
    pub reason: String,
}

/// The votes on a proposal.
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
pub struct TallyResult {
    /// Where it stands.
    pub status: TallyStatus,
    /// Counted approvals.
    pub approvals: u32,
    /// Counted rejections.
    pub rejections: u32,
    /// The spending threshold `k`.
    pub needed: u32,
    /// Votes that did not verify.
    pub refused: Vec<RefusedVote>,
    /// The 49-field auth witness, once approved.
    pub witness: Option<Vec<Hex32>>,
}

/// A proposed membership rotation: the new signers, spending threshold and committee from the reshare.
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Rotation {
    /// New signer keys in slot order.
    pub signers: Vec<Point>,
    /// New spending threshold `k`.
    pub threshold: u32,
    /// New viewing threshold `t_v`.
    pub viewing_threshold: u32,
    /// The verification shares the reshare produced.
    pub verification_shares: Vec<Point>,
}

/// A threshold choice members should make knowingly.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum Warning {
    /// `k = 1`: any single member can spend alone.
    SingleSigner,
    /// `k = n`: losing one member's key freezes the funds.
    NoSpareSigner,
    /// `t_v = 2` with three or more members: any two can read everything received while they were members.
    TwoCanRead,
    /// `t_v = n`: losing one keeper makes the vault unreadable for good.
    NoSpareViewer,
}

/// A committee dealt from one secret, for tests (`@ome-labs/wasm/testing`).
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DealtCommittee {
    /// Group key: the secret times the generator.
    pub group_key: Point,
    /// Members' viewing shares, in member order.
    pub shares: Vec<Hex32>,
    /// Members' verification shares, in member order.
    pub verification_shares: Vec<Point>,
}
