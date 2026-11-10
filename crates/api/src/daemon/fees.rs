//! Who pays a vault's fees and whether it can: the fee payer this member trusts, its Fee Juice, and what a
//! transaction is expected to cost.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::Gas;
use crate::{Amount, Hex32};

/// Whose fee payer it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum FeePayerKind {
    /// The network's canonical sponsored fee payer (salt zero).
    Sponsored,
    /// The member's own deployment, which they keep funded.
    Own,
}

/// The fee payer and its Fee Juice.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct FeesStatus {
    /// The fee payer contract.
    pub payer: Hex32,
    /// Whose it is.
    pub kind: FeePayerKind,
    /// Its Fee Juice, in base units; `None` when the node can't be read.
    pub fee_juice: Option<Amount>,
    /// It holds Fee Juice to pay with.
    pub healthy: bool,
}

/// What a transaction is expected to cost, from its simulation and the node's current fees.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct FeeEstimate {
    /// Gas the simulation used.
    pub gas: Gas,
    /// Its cost at the node's current minimum fees, in Fee Juice base units.
    pub fee: Amount,
    /// The most it may cost: Aztec's wallet pads the fee per gas by half against fee changes before inclusion.
    pub max_fee: Amount,
    /// Who pays.
    pub payer: Hex32,
}
