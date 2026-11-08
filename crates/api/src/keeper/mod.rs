//! The keeper's API: what a member's daemon calls on its own keeper.

mod ceremonies;
mod events;
mod lobby;
mod records;
mod vaults;

pub use ceremonies::{
    Ceremony, CeremonyKind, CeremonyResult, CeremonyStatus, DkgParams, DkgResult, EcdhParams,
    EcdhPurpose, EcdhResult, Failure, MemberPartials, NewMember, Partial, ReshareParams,
    ReshareResult,
};
pub use events::KeeperEvent;
pub use lobby::{
    Invite, InviteParams, JoinRequestText, Lobby, LobbyJoinParams, LobbyJoined, LobbyMember,
    LobbyMemberParams, LobbyOpenParams, LobbyOpened, LobbyParams, LobbyPlan, LobbyRole,
    LobbyStartParams, LobbyStatus,
};
pub use records::{
    BodyParams, IdParams, MailboxPolled, PayloadParams, PayloadStored, ProposalParams,
    ProposalStored, Receipt, ReceiptParams, TxObserved, TxParams,
};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
pub use vaults::{
    AuditEntry, KeeperContact, KeeperInfo, Manifest, Member, Observation, Observed, Reachability,
    Vault, VaultConfig, VaultStatus,
};

use crate::{AuthParams, Empty, Hex32, HexBytes, jsonrpc::methods};

/// Params naming a vault.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct VaultParams {
    /// Vault address.
    pub vault: Hex32,
}

/// Params naming a ceremony.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct SessionParams {
    /// Session id.
    pub session: Hex32,
}

/// Params of `vault.register`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct RegisterParams {
    /// The founding DKG's session.
    pub session: Hex32,
    /// The deployed vault's manifest.
    pub manifest: Manifest,
}

/// Params of `vault.observeConfig`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ObserveConfigParams {
    /// Vault.
    pub vault: Hex32,
    /// Its config note as read from the chain.
    pub config: VaultConfig,
}

/// Params of `vault.retire`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct RetireParams {
    /// Vault.
    pub vault: Hex32,
    /// Retire even though this keeper has not seen a newer config: for a member removed by a rotation its keeper took
    /// no part in, which cannot read the new config.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub force: Option<bool>,
}

/// Params of `audit.list`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct AuditParams {
    /// Vault.
    pub vault: Hex32,
    /// Most recent entries to return; 100 by default, at most 10,000.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub limit: Option<u32>,
}

/// A method the keeper serves.
pub trait KeeperMethod: crate::Method {}

methods! {
    /// Every keeper method, in table order.
    METHODS: KeeperMethod;
    /// Presents the session token. Must be the first call.
    SessionAuth = "session.auth" (AuthParams) -> ();
    /// Streams [`KeeperEvent`]s on this session.
    EventsSubscribe = "events.subscribe" (Empty) -> ();
    /// Who this keeper is.
    KeeperInfoGet = "keeper.info" (Empty) -> KeeperInfo;
    /// A join request for an existing vault, naming this keeper and signed by it.
    KeeperJoinRequest = "keeper.joinRequest" (LobbyMemberParams) -> JoinRequestText;
    /// Every vault the keeper knows.
    VaultList = "vault.list" (Empty) -> Vec<Vault>;
    /// One vault.
    VaultStatusGet = "vault.status" (VaultParams) -> Vault;
    /// Registers a founded vault, binding the DKG's share to its manifest.
    VaultRegister = "vault.register" (RegisterParams) -> Vault;
    /// Tells the keeper what the chain says the vault's config is.
    VaultObserveConfig = "vault.observeConfig" (ObserveConfigParams) -> Observed;
    /// Destroys a kept share once the chain shows a newer committee.
    VaultRetire = "vault.retire" (RetireParams) -> Vault;
    /// Asks every reachable member for records this keeper lacks.
    VaultSync = "vault.sync" (VaultParams) -> ();
    /// Checks a vault transaction against the execution receipts.
    VaultObserveTx = "vault.observeTx" (TxParams) -> TxObserved;
    /// Reachability of every committee member.
    PeersStatus = "peers.status" (VaultParams) -> Vec<Reachability>;
    /// Fetches waiting envelopes from the relays now.
    MailboxPoll = "mailbox.poll" (Empty) -> MailboxPolled;
    /// Threshold ECDH over a batch of points.
    EcdhBatch = "ecdh.batch" (EcdhParams) -> EcdhResult;
    /// Starts or joins a founding DKG.
    CeremonyDkg = "ceremony.dkg" (DkgParams) -> ();
    /// Starts or joins a reshare.
    CeremonyReshare = "ceremony.reshare" (ReshareParams) -> ();
    /// A ceremony's status and outcome.
    CeremonyStatusGet = "ceremony.status" (SessionParams) -> Ceremony;
    /// Stores and replicates a proposal.
    ProposalsPut = "proposals.put" (BodyParams) -> ProposalStored;
    /// Every stored proposal of a vault.
    ProposalsList = "proposals.list" (VaultParams) -> Vec<HexBytes>;
    /// One proposal.
    ProposalsGet = "proposals.get" (IdParams) -> Option<HexBytes>;
    /// Stores a payload locally; members fetch it by hash.
    PayloadsPut = "payloads.put" (BodyParams) -> PayloadStored;
    /// One payload.
    PayloadsGet = "payloads.get" (PayloadParams) -> Option<HexBytes>;
    /// Stores and replicates an approval.
    ApprovalsPut = "approvals.put" (BodyParams) -> ();
    /// A proposal's approvals.
    ApprovalsList = "approvals.list" (ProposalParams) -> Vec<HexBytes>;
    /// Stores and replicates a rejection.
    RejectionsPut = "rejections.put" (BodyParams) -> ();
    /// A proposal's rejections.
    RejectionsList = "rejections.list" (ProposalParams) -> Vec<HexBytes>;
    /// Stores and replicates an execution receipt.
    ReceiptsPut = "receipts.put" (ReceiptParams) -> ();
    /// A vault's execution receipts.
    ReceiptsList = "receipts.list" (VaultParams) -> Vec<Receipt>;
    /// The vault's ECDH audit log, newest first.
    AuditList = "audit.list" (AuditParams) -> Vec<AuditEntry>;
    /// Opens a lobby as host.
    LobbyOpen = "lobby.open" (LobbyOpenParams) -> LobbyOpened;
    /// Joins a lobby from an invite.
    LobbyJoin = "lobby.join" (LobbyJoinParams) -> LobbyJoined;
    /// Decodes an invite.
    LobbyInspect = "lobby.inspect" (InviteParams) -> Invite;
    /// One lobby, refreshed from the host.
    LobbyStatusGet = "lobby.status" (LobbyParams) -> Lobby;
    /// Every lobby.
    LobbyList = "lobby.list" (Empty) -> Vec<Lobby>;
    /// Fixes the plan (host).
    LobbyStart = "lobby.start" (LobbyStartParams) -> Lobby;
    /// Cancels a lobby (host).
    LobbyClose = "lobby.close" (LobbyParams) -> Lobby;
    /// Leaves a lobby (guest).
    LobbyLeave = "lobby.leave" (LobbyParams) -> Lobby;
}
