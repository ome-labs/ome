//! What a client needs to fix a failure: the numbers behind an error kind. Each variant carries its kind as a tag, so
//! TypeScript narrows on `details.kind` the same way it narrows on `data.kind`.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{
    Amount, ErrorKind, Hex32,
    daemon::{ExecutionPhase, MemberRef, TokenInfo},
};

/// The details of an error, one variant per kind that has them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub enum ErrorDetails {
    /// Not yet possible, and what it waits for.
    NotReady {
        /// What isn't ready.
        reason: NotReadyReason,
    },
    /// Less is held than is needed.
    InsufficientFunds {
        /// What is short.
        funds: Funds,
        /// What the transaction needs, in base units.
        needed: Amount,
        /// What is held, in base units.
        held: Amount,
    },
    /// The deadline that passed.
    Expired {
        /// Unix seconds.
        at: u64,
    },
    /// The config the proposal was made under, and the vault's now.
    Stale {
        /// Config version in the proposal.
        made: u32,
        /// The vault's current config version.
        current: u32,
    },
    /// Who is already doing it.
    Conflict {
        /// The member.
        member: MemberRef,
        /// How far they are.
        phase: ExecutionPhase,
        /// Unix seconds they started.
        since: u64,
    },
    /// How many the lobby takes.
    Full {
        /// Members it takes.
        members: u32,
    },
    /// How many keepers answered, out of how many are needed.
    Unreadable {
        /// Keepers that answered with valid partials.
        answered: u32,
        /// The vault's viewing threshold.
        needed: u32,
    },
    /// The contract this member can't call.
    UnknownContract {
        /// Its address.
        address: Hex32,
        /// Its class, when the node knows the instance.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        class_id: Option<Hex32>,
    },
    /// The network the thing belongs to, and this profile's.
    Network {
        /// Where it belongs, e.g. `aztec:11155111:679261766`.
        expected: String,
        /// This profile's network.
        actual: String,
    },
}

impl ErrorDetails {
    /// The kind these details belong to.
    #[must_use]
    pub const fn kind(&self) -> ErrorKind {
        match self {
            Self::NotReady { .. } => ErrorKind::NotReady,
            Self::InsufficientFunds { .. } => ErrorKind::InsufficientFunds,
            Self::Expired { .. } => ErrorKind::Expired,
            Self::Stale { .. } => ErrorKind::Stale,
            Self::Conflict { .. } => ErrorKind::Conflict,
            Self::Full { .. } => ErrorKind::Full,
            Self::Unreadable { .. } => ErrorKind::Unreadable,
            Self::UnknownContract { .. } => ErrorKind::UnknownContract,
            Self::Network { .. } => ErrorKind::Network,
        }
    }
}

/// What a `not-ready` call waits for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum NotReadyReason {
    /// The daemon is still starting.
    Starting,
    /// There is no profile yet.
    Profile,
    /// The signer must be unlocked first.
    Signer,
    /// The node can't be reached.
    Node,
    /// The keeper isn't running.
    Keeper,
    /// The wallet is still syncing.
    Sync,
    /// The vault isn't loaded yet.
    Vault,
    /// A proposal's payload hasn't arrived from the member who made it.
    Payload,
    /// A rotation's reshare hasn't finished.
    Reshare,
    /// A lobby hasn't been joined by every member it plans for.
    Members,
}

/// What a transaction is short of.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Funds {
    /// A token the vault holds.
    Token {
        /// The token.
        token: TokenInfo,
    },
    /// Fee Juice of the fee payer this member trusts.
    FeeJuice {
        /// The fee payer.
        payer: Hex32,
    },
}
