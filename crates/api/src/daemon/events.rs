//! Events the daemon streams to its clients after `events.subscribe`. Each says exactly what changed, so a client
//! refreshes one view; each arrives numbered, so a client that reconnects can ask for what it missed.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{
    ExecutionPhase, ExecutionProgress, FoundingState, KeeperState, LobbyView, MemberRef,
    NodeStatus, RotationState, SignerStatus, TokenInfo,
};
use crate::{Amount, Hex32, keeper::KeeperEvent};

/// Where an event sits in the daemon's stream: `epoch` changes each time the daemon starts, `n` counts up from 1.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct EventSeq {
    /// The daemon start this stream belongs to.
    pub epoch: u64,
    /// The event's number in that stream.
    pub n: u64,
}

/// An event as it is sent: numbered and timed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct EventEnvelope {
    /// Its place in the stream.
    pub seq: EventSeq,
    /// Unix seconds it happened.
    pub at: u64,
    /// What happened.
    pub event: DaemonEvent,
}

/// Params of `events.subscribe`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct SubscribeParams {
    /// The last event the client saw, to receive the ones after it first.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub since: Option<EventSeq>,
}

/// What `events.subscribe` answers before the stream starts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Subscription {
    /// The current stream.
    pub epoch: u64,
    /// The last event sent so far in it (0 before the first).
    pub latest: u64,
    /// The events after `since` can't be replayed (another epoch, or too old): refetch everything once.
    pub reset: bool,
}

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
    /// The keeper reported something; forwarded as is, for views that follow the keeper itself.
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
    /// A proposal arrived: made here, or replicated from a member.
    ProposalCreated {
        /// Vault.
        vault: Hex32,
        /// Proposal id.
        proposal: Hex32,
        /// One line: "Transfer 40 OMET to 0xf941…12b3".
        summary: String,
        /// Who proposed it.
        by: MemberRef,
    },
    /// A member approved or rejected a proposal.
    ProposalVoted {
        /// Vault.
        vault: Hex32,
        /// Proposal id.
        proposal: Hex32,
        /// One line.
        summary: String,
        /// Who voted.
        by: MemberRef,
        /// Approved, or rejected.
        approve: bool,
        /// The rejection's reason.
        reason: Option<String>,
    },
    /// A proposal has its approvals and can be executed.
    ProposalReady {
        /// Vault.
        vault: Hex32,
        /// Proposal id.
        proposal: Hex32,
        /// One line.
        summary: String,
    },
    /// Enough members rejected a proposal that it can't pass.
    ProposalBlocked {
        /// Vault.
        vault: Hex32,
        /// Proposal id.
        proposal: Hex32,
        /// One line.
        summary: String,
    },
    /// A member started executing a proposal, moved to another phase, or stopped (`mined` or `failed`).
    ProposalExecuting {
        /// Vault.
        vault: Hex32,
        /// Proposal id.
        proposal: Hex32,
        /// One line.
        summary: String,
        /// Who.
        by: MemberRef,
        /// How far they are.
        phase: ExecutionPhase,
    },
    /// A proposal's transaction was mined.
    ProposalExecuted {
        /// Vault.
        vault: Hex32,
        /// Proposal id.
        proposal: Hex32,
        /// One line.
        summary: String,
        /// Transaction.
        tx_hash: Hex32,
        /// Who executed it, when their keeper is a member.
        by: Option<MemberRef>,
    },
    /// A cancellation executed: the proposal's nonce is spent and it can never run.
    ProposalCancelled {
        /// Vault.
        vault: Hex32,
        /// The cancelled proposal.
        proposal: Hex32,
        /// One line.
        summary: String,
        /// The cancellation that ran.
        by_proposal: Hex32,
    },
    /// An open proposal expires within the hour: announced once.
    ProposalExpiring {
        /// Vault.
        vault: Hex32,
        /// Proposal id.
        proposal: Hex32,
        /// One line.
        summary: String,
        /// Unix seconds it expires.
        at: u64,
    },
    /// An open proposal expired without executing: announced once.
    ProposalExpired {
        /// Vault.
        vault: Hex32,
        /// Proposal id.
        proposal: Hex32,
        /// One line.
        summary: String,
    },
    /// An execution on this device advanced.
    Execution {
        /// Progress.
        execution: ExecutionProgress,
    },
    /// The vault received a transfer.
    Received {
        /// Vault.
        vault: Hex32,
        /// Token.
        token: TokenInfo,
        /// Amount, in base units.
        amount: Amount,
        /// Sender.
        from: Hex32,
        /// Transaction.
        tx_hash: Hex32,
    },
    /// A token's balance in the vault may have changed.
    BalanceChanged {
        /// Vault.
        vault: Hex32,
        /// Token contract.
        token: Hex32,
    },
    /// A contract's code arrived from another member and checked out: reviews of the vault's proposals that call it
    /// can now decode and simulate them.
    ContractAvailable {
        /// Vault.
        vault: Hex32,
        /// The contract class.
        class_id: Hex32,
    },
    /// The vault started tracking a token.
    TokenAdded {
        /// Vault.
        vault: Hex32,
        /// Token.
        token: TokenInfo,
    },
    /// The vault stopped tracking a token.
    TokenRemoved {
        /// Vault.
        vault: Hex32,
        /// Token contract.
        token: Hex32,
    },
    /// The member's own record of a vault changed: its alias, whether it is the default, its status.
    VaultUpdated {
        /// Vault.
        vault: Hex32,
    },
    /// The vault's on-chain config changed: thresholds, signers or committee.
    ConfigChanged {
        /// Vault.
        vault: Hex32,
        /// The config version now.
        version: u32,
    },
    /// The vault's members or this member's place among them changed.
    MemberChanged {
        /// Vault.
        vault: Hex32,
    },
    /// A rotation advanced.
    Rotation {
        /// Its state.
        rotation: Box<RotationState>,
    },
    /// A rotation removed members, and counterparties who paid the vault before it should handshake with the vault
    /// again so the removed members stop seeing when they pay (`vault.counterparties` lists them).
    HandshakeAdvised {
        /// Vault.
        vault: Hex32,
        /// Exposed counterparties.
        counterparties: u32,
    },
}

impl DaemonEvent {
    /// The vault the event is about, if it is about one.
    #[must_use]
    pub const fn vault(&self) -> Option<&Hex32> {
        match self {
            Self::ProposalCreated { vault, .. }
            | Self::ProposalVoted { vault, .. }
            | Self::ProposalReady { vault, .. }
            | Self::ProposalBlocked { vault, .. }
            | Self::ProposalExecuting { vault, .. }
            | Self::ProposalExecuted { vault, .. }
            | Self::ProposalCancelled { vault, .. }
            | Self::ProposalExpiring { vault, .. }
            | Self::ProposalExpired { vault, .. }
            | Self::Received { vault, .. }
            | Self::BalanceChanged { vault, .. }
            | Self::ContractAvailable { vault, .. }
            | Self::TokenAdded { vault, .. }
            | Self::TokenRemoved { vault, .. }
            | Self::VaultUpdated { vault }
            | Self::ConfigChanged { vault, .. }
            | Self::MemberChanged { vault }
            | Self::HandshakeAdvised { vault, .. } => Some(vault),
            Self::Execution { execution } => Some(&execution.vault),
            Self::Keeper { .. }
            | Self::KeeperEvent { .. }
            | Self::Signer { .. }
            | Self::Node { .. }
            | Self::Lobby { .. }
            | Self::Founding { .. }
            | Self::Rotation { .. } => None,
        }
    }
}
