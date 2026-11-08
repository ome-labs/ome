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
    /// A reshare's committee never became active; its pending share was destroyed.
    PendingExpired {
        /// Vault (hex).
        vault: String,
        /// Reshare session (hex).
        session: String,
    },
}
