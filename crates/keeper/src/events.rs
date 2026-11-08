//! Events the keeper streams to its member's daemon.

use serde::Serialize;

/// Something the member's daemon should know about.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Event {
    /// A ceremony changed status.
    Ceremony {
        /// Session id (hex).
        session: String,
        /// `dkg` or `reshare`.
        kind: String,
        /// `running`, `completed`, `aborted`, `activated` or `expired`.
        status: String,
    },
    /// A member's partials failed verification.
    InvalidPartials {
        /// Vault (hex).
        vault: String,
        /// Member index.
        member: u32,
    },
    /// A new committee became active for a vault.
    CommitteeActivated {
        /// Vault (hex).
        vault: String,
        /// Committee epoch.
        epoch: u32,
    },
    /// A verified rotation left this keeper out of the vault's committee; its share was destroyed.
    Removed {
        /// Vault (hex).
        vault: String,
    },
    /// The observed on-chain config names a committee this keeper cannot verify. The keeper keeps its share and acts on
    /// nothing until a person looks.
    ConfigMismatch {
        /// Vault (hex).
        vault: String,
        /// Observed config version.
        version: u32,
        /// Why the config was not accepted.
        reason: String,
    },
    /// A vault transaction that matches no known proposal execution.
    UnexpectedSpend {
        /// Vault (hex).
        vault: String,
        /// Transaction hash (hex).
        tx_hash: String,
    },
    /// A proposal arrived or was stored.
    Proposal {
        /// Vault (hex).
        vault: String,
        /// Proposal id (hex).
        id: String,
    },
    /// A proposal's payload arrived or was stored.
    Payload {
        /// Vault (hex).
        vault: String,
        /// Payload hash (hex).
        hash: String,
    },
    /// An approval arrived or was stored.
    Approval {
        /// Vault (hex).
        vault: String,
        /// Proposal id (hex).
        proposal: String,
        /// Signer slot.
        slot: u32,
    },
    /// A rejection arrived or was stored.
    Rejection {
        /// Vault (hex).
        vault: String,
        /// Proposal id (hex).
        proposal: String,
        /// Signer slot.
        slot: u32,
    },
    /// An execution receipt was stored, put locally or replicated from a member.
    Receipt {
        /// Vault (hex).
        vault: String,
        /// Proposal id (hex).
        proposal: String,
        /// Transaction hash (hex).
        tx_hash: String,
    },
    /// A lobby changed: opened or joined, roster changed, started or closed.
    Lobby {
        /// Lobby id (hex).
        lobby: String,
        /// `open`, `started` or `closed`.
        status: String,
        /// Members on the roster.
        members: u32,
    },
    /// The member retired a share the keeper had kept for a vault it no longer serves.
    Retired {
        /// Vault (hex).
        vault: String,
    },
    /// A reshare's committee never became active; its pending share was destroyed.
    PendingExpired {
        /// Vault (hex).
        vault: String,
        /// Reshare session (hex).
        session: String,
    },
}
