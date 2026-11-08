//! Proposals, payloads, votes and receipts the keeper stores and replicates. Bodies are hex of their canonical
//! `ome-wire` encoding; the keeper verifies them before storing.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{Hex32, HexBytes};

/// Params of `proposals.put`, `payloads.put`, `approvals.put` and `rejections.put`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct BodyParams {
    /// Vault.
    pub vault: Hex32,
    /// Canonical encoding.
    pub body: HexBytes,
}

/// The result of `proposals.put`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ProposalStored {
    /// Proposal id.
    pub id: Hex32,
}

/// The result of `payloads.put`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct PayloadStored {
    /// Payload hash, as the proposal references it.
    pub hash: Hex32,
}

/// Params naming one proposal.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct IdParams {
    /// Proposal id.
    pub id: Hex32,
}

/// Params naming a proposal whose votes are listed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ProposalParams {
    /// Proposal id.
    pub proposal: Hex32,
}

/// Params of `payloads.get`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct PayloadParams {
    /// Vault.
    pub vault: Hex32,
    /// Payload hash.
    pub hash: Hex32,
}

/// Params of `receipts.put`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ReceiptParams {
    /// Vault.
    pub vault: Hex32,
    /// Executed proposal.
    pub proposal: Hex32,
    /// Transaction that executed it.
    pub tx_hash: Hex32,
}

/// An execution receipt.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Receipt {
    /// Executed proposal.
    pub proposal: Hex32,
    /// Transaction that executed it.
    pub tx_hash: Hex32,
    /// Unix seconds it was stored.
    pub at: u64,
}

/// Params of `vault.observeTx`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TxParams {
    /// Vault.
    pub vault: Hex32,
    /// A transaction the vault sent.
    pub tx_hash: Hex32,
}

/// The result of `vault.observeTx`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct TxObserved {
    /// Whether a receipt names it; if not, the keeper raised `unexpectedSpend`.
    pub known: bool,
}

/// The result of `mailbox.poll`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct MailboxPolled {
    /// Envelopes delivered.
    pub delivered: u32,
}
