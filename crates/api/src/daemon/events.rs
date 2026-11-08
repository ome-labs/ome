//! Events the daemon streams to its clients after `events.subscribe`.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{
    ExecutionProgress, FoundingState, KeeperState, LobbyView, NodeStatus, RotationState,
    SignerStatus,
};
use crate::{Hex32, keeper::KeeperEvent};

/// Something a client should redraw.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum DaemonEvent {
    /// The keeper's service state changed.
    Keeper {
        /// New state.
        keeper: KeeperState,
    },
    /// The keeper reported something; forwarded as is.
    KeeperEvent {
        /// The keeper's event.
        event: KeeperEvent,
    },
    /// The signer locked or unlocked.
    Signer {
        /// New status.
        signer: SignerStatus,
    },
    /// The node check changed.
    Node {
        /// New status.
        node: NodeStatus,
    },
    /// A lobby changed.
    Lobby {
        /// The lobby.
        lobby: LobbyView,
    },
    /// A founding advanced.
    Founding {
        /// Its state.
        founding: FoundingState,
    },
    /// A vault's proposals, votes or balances changed.
    VaultActivity {
        /// Vault.
        vault: Hex32,
    },
    /// An execution on this device advanced.
    Execution {
        /// Progress.
        execution: ExecutionProgress,
    },
    /// A rotation advanced (draft).
    Rotation {
        /// Its state.
        rotation: Box<RotationState>,
    },
}
