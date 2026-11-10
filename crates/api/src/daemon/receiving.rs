//! Receiving and reading: balances as states, payment requests, and the paged lists a busy vault needs.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{ActivityEntry, AuditView, Counterparty, ProposalView, TokenInfo};
use crate::{Amount, Hex32};

/// A balance as this member could read it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "state",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum BalanceState {
    /// Read.
    Known {
        /// In base units.
        amount: Amount,
    },
    /// Too few keepers answered to decrypt the vault's notes.
    Unreadable {
        /// Keepers that answered with valid partials.
        answered: u32,
        /// The vault's viewing threshold.
        needed: u32,
    },
    /// Anything else, verbatim.
    Failed {
        /// Why.
        message: String,
    },
}

impl BalanceState {
    /// The amount, when known.
    #[must_use]
    pub const fn amount(&self) -> Option<Amount> {
        match self {
            Self::Known { amount } => Some(*amount),
            _ => None,
        }
    }
}

/// The vault's balance of a token, private (its notes) and public (the token's storage).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct TokenBalance {
    /// Token.
    pub token: TokenInfo,
    /// Private balance: needs the keepers to read the vault's notes.
    pub private: BalanceState,
    /// Public balance: needs no viewing key, so it shows even when the keepers can't answer.
    pub public: BalanceState,
}

/// What the Receive page can promise.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ReceivingStatus {
    /// Payments arriving with a non-interactive handshake (any Aztec wallet's default) are found.
    pub handshakes: bool,
    /// Senders registered from `aztec-wallet`'s accounts, found even without a handshake.
    pub senders: u32,
    /// The latest transfer received in a tracked token (unix seconds).
    pub last_received_at: Option<u64>,
}

/// Params of `vault.request`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct PaymentRequestParams {
    /// Vault.
    pub vault: Hex32,
    /// The token asked for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub token: Option<Hex32>,
    /// The amount asked for, in base units.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub amount: Option<Amount>,
    /// What it is for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub memo: Option<String>,
}

/// A payment request to hand a payer: `ome-pay:` link (`architecture/wire.md`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct PaymentRequest {
    /// The `ome-pay:` link.
    pub uri: String,
    /// The vault.
    pub vault: Hex32,
    /// The network: `aztec:<L1 chain id>:<rollup version>`.
    pub network: String,
    /// The token asked for, when the vault tracks it.
    pub token: Option<TokenInfo>,
    /// The amount, in base units.
    pub amount: Option<Amount>,
    /// What it is for.
    pub memo: Option<String>,
}

/// Which proposals `proposals.list` returns.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum ProposalFilter {
    /// Every proposal.
    All,
    /// Collecting, ready or blocked.
    Open,
    /// Ready to execute.
    Ready,
    /// Executed, cancelled, expired or stale.
    Done,
    /// Collecting or ready, and this member hasn't voted.
    NeedsMe,
}

/// Params of `proposals.list`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ProposalListParams {
    /// Vault.
    pub vault: Hex32,
    /// Which proposals; all when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub filter: Option<ProposalFilter>,
    /// At most this many, 1 to 500 (50 when absent).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub limit: Option<u32>,
    /// The previous page's `next`: items older than it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub before: Option<String>,
}

/// What `vault.activity` lists.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum ActivityKind {
    /// Proposals.
    Proposal,
    /// Received transfers.
    Received,
}

/// Params of `vault.activity`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ActivityParams {
    /// Vault.
    pub vault: Hex32,
    /// Which kinds; every kind when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub kinds: Option<Vec<ActivityKind>>,
    /// Only entries about this token.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub token: Option<Hex32>,
    /// At most this many, 1 to 500 (50 when absent).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub limit: Option<u32>,
    /// The previous page's `next`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub before: Option<String>,
}

/// Params of `vault.audit` and `vault.counterparties`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct PageParams {
    /// Vault.
    pub vault: Hex32,
    /// At most this many, 1 to 500 (50 when absent).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub limit: Option<u32>,
    /// The previous page's `next`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub before: Option<String>,
}

/// A page of proposals, newest first. `next` is the cursor for older ones, `None` on the last page.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ProposalPage {
    /// Proposals.
    pub items: Vec<ProposalView>,
    /// Cursor for the next page.
    pub next: Option<String>,
}

/// A page of activity, newest first.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ActivityPage {
    /// Entries.
    pub items: Vec<ActivityEntry>,
    /// Cursor for the next page.
    pub next: Option<String>,
}

/// A page of the ECDH audit log, newest first.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct AuditPage {
    /// Entries.
    pub items: Vec<AuditView>,
    /// Cursor for the next page.
    pub next: Option<String>,
}

/// A page of counterparties, most recent payer first.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct CounterpartyPage {
    /// Counterparties.
    pub items: Vec<Counterparty>,
    /// Cursor for the next page.
    pub next: Option<String>,
}

/// A vault's open proposals at a glance, for a badge.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ProposalCounts {
    /// Collecting, ready or blocked.
    pub open: u32,
    /// Ready to execute.
    pub ready: u32,
    /// Collecting or ready, and this member hasn't voted.
    pub needs_me: u32,
}
