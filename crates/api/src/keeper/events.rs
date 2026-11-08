//! Events the keeper streams to its member's daemon after `events.subscribe`.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{CeremonyKind, CeremonyStatus, LobbyStatus};
use crate::{Hex16, Hex32};

/// Something the member's daemon should know about.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum KeeperEvent {
    /// A ceremony changed status.
    Ceremony {
        /// Session id.
        session: Hex32,
        /// Which ceremony.
        kind: CeremonyKind,
        /// Its new status.
        status: CeremonyStatus,
    },
    /// A member's ECDH partials failed verification.
    InvalidPartials {
        /// Vault.
        vault: Hex32,
        /// Member index.
        member: u32,
    },
    /// A new committee became active for a vault.
    CommitteeActivated {
        /// Vault.
        vault: Hex32,
        /// Committee epoch.
        epoch: u32,
    },
    /// A verified rotation left this keeper out of a vault's committee; its share was destroyed.
    Removed {
        /// Vault.
        vault: Hex32,
    },
    /// The observed config names a committee the keeper cannot verify. It keeps its share and acts on nothing until a
    /// person looks.
    ConfigMismatch {
        /// Vault.
        vault: Hex32,
        /// Observed config version.
        version: u32,
        /// Why the config was not accepted.
        reason: String,
    },
    /// A vault transaction matches no execution receipt.
    UnexpectedSpend {
        /// Vault.
        vault: Hex32,
        /// Transaction hash.
        tx_hash: Hex32,
    },
    /// A proposal was stored, put locally or replicated from a member.
    Proposal {
        /// Vault.
        vault: Hex32,
        /// Proposal id.
        id: Hex32,
    },
    /// A proposal's payload was stored.
    Payload {
        /// Vault.
        vault: Hex32,
        /// Payload hash.
        hash: Hex32,
    },
    /// An approval was stored.
    Approval {
        /// Vault.
        vault: Hex32,
        /// Proposal id.
        proposal: Hex32,
        /// Signer slot.
        slot: u32,
    },
    /// A rejection was stored.
    Rejection {
        /// Vault.
        vault: Hex32,
        /// Proposal id.
        proposal: Hex32,
        /// Signer slot.
        slot: u32,
    },
    /// An execution receipt was stored.
    Receipt {
        /// Vault.
        vault: Hex32,
        /// Proposal id.
        proposal: Hex32,
        /// Transaction hash.
        tx_hash: Hex32,
    },
    /// A lobby changed: opened or joined, roster changed, started or closed.
    Lobby {
        /// Lobby id.
        lobby: Hex16,
        /// Its status.
        status: LobbyStatus,
        /// Members on the roster.
        members: u32,
    },
    /// The member retired a share the keeper kept for a vault it no longer serves.
    Retired {
        /// Vault.
        vault: Hex32,
    },
    /// A reshare's committee never became active; its pending share was destroyed.
    PendingExpired {
        /// Vault.
        vault: Hex32,
        /// Reshare session.
        session: Hex32,
    },
}
