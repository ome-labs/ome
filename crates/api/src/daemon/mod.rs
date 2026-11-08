//! The daemon's API: what the CLI and apps call on a member's `omed`.

mod devices;
mod events;
mod founding;
mod membership;
mod proposals;
mod status;
mod vaults;

pub use devices::{DeviceView, EnrollParams, SwapSignerParams};
pub use events::DaemonEvent;
pub use founding::{
    FoundingError, FoundingState, FoundingStep, FoundingStepId, InviteHost, InvitePreview,
    LobbyCreateParams, LobbyView, LobbyViewMember, StartLobbyParams, StepStatus,
};
pub use membership::{
    CommitteeShape, Joiner, MembershipParams, MembershipPlan, RotationPlanText, RotationRef,
    RotationState, RotationStep, RotationStepId,
};
pub use proposals::{
    ApproveParams, ContractFunction, DecodedCall, Execution, ExecutionPhase, ExecutionProgress,
    FunctionKind, FunctionParam, Gas, Intent, IntentCall, LabelSource, ProposalPreview,
    ProposalRef, ProposalStatus, ProposalView, ProposeParams, Proposer, Recipient, RejectParams,
    Review, ReviewChecks, Signs, Simulation, Vote,
};
use serde::{Deserialize, Serialize};
pub use status::{
    DaemonInfo, KeeperState, NodeStatus, PassphraseParams, Profile, ProfileCreateParams,
    RelaysParams, SealingKey, Secret, ServiceStatus, Settings, SettingsPatch, SignerStatus, Status,
    UnlockParams,
};
use ts_rs::TS;
pub use vaults::{
    ActivityEntry, AuditView, CommitteeMember, CommitteeView, ConfigView, Contact, ContractInfo,
    FoundingInfo, MemberReachability, NamedKey, TokenBalance, TokenInfo, VaultDetails,
    VaultSummary,
};

use crate::{
    AuthParams, Empty, Hex32,
    jsonrpc::methods,
    keeper::{DeviceOffer, DeviceParams, DeviceReviewSent, InviteParams, LobbyParams, VaultParams},
};

/// Params of `vault.audit`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct VaultAuditParams {
    /// Vault.
    pub vault: Hex32,
    /// Most recent entries, 1 to 500.
    pub limit: u32,
}

/// Params of `vault.alias`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct AliasParams {
    /// Vault.
    pub vault: Hex32,
    /// Lowercase letters, digits, `-` and `_`, at most 32.
    pub alias: String,
}

/// Params of `tokens.add`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct TokenParams {
    /// Vault.
    pub vault: Hex32,
    /// Token contract.
    pub token: Hex32,
}

/// Params of `contracts.functions`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ContractParams {
    /// Contract.
    pub contract: Hex32,
}

/// Params of `contacts.put`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ContactParams {
    /// Address.
    pub address: Hex32,
    /// Label, the alias rules.
    pub label: String,
}

/// Params of `contacts.remove`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct AddressParams {
    /// Address.
    pub address: Hex32,
}

/// A method the daemon serves.
pub trait DaemonMethod: crate::Method {}

methods! {
    /// Every daemon method, in table order.
    METHODS: DaemonMethod;
    /// Presents the session token. Must be the first call.
    SessionAuth = "session.auth" (AuthParams) -> ();
    /// Streams [`DaemonEvent`]s on this session.
    EventsSubscribe = "events.subscribe" (Empty) -> ();
    /// Who this daemon is.
    DaemonInfoGet = "daemon.info" (Empty) -> DaemonInfo;
    /// Stops the daemon and its keeper.
    DaemonStop = "daemon.stop" (Empty) -> ();
    /// Everything `ome status` shows.
    StatusGet = "status" (Empty) -> Status;
    /// Creates the profile: the sealed signer and the member's account.
    ProfileCreate = "profile.create" (ProfileCreateParams) -> Status;
    /// Settings.
    ConfigGet = "config.get" (Empty) -> Settings;
    /// Changes settings.
    ConfigSet = "config.set" (SettingsPatch) -> Settings;
    /// Replaces the relays.
    RelaysSet = "relays.set" (RelaysParams) -> Settings;
    /// Whether the signer is locked.
    SignerStatusGet = "signer.status" (Empty) -> SignerStatus;
    /// Unlocks the signer for a window.
    SignerUnlock = "signer.unlock" (UnlockParams) -> SignerStatus;
    /// Locks the signer.
    SignerLock = "signer.lock" (Empty) -> SignerStatus;
    /// Derives the key that unseals the signer in place of the passphrase.
    SignerSealingKey = "signer.sealingKey" (PassphraseParams) -> SealingKey;
    /// Checks the node now.
    NodeStatusGet = "node.status" (Empty) -> NodeStatus;
    /// Decodes an invite.
    LobbyInspect = "lobby.inspect" (InviteParams) -> InvitePreview;
    /// Opens a lobby as host.
    LobbyCreate = "lobby.create" (LobbyCreateParams) -> LobbyView;
    /// Joins a lobby.
    LobbyJoin = "lobby.join" (InviteParams) -> LobbyView;
    /// One lobby.
    LobbyGet = "lobby.get" (LobbyParams) -> LobbyView;
    /// Every lobby.
    LobbyList = "lobby.list" (Empty) -> Vec<LobbyView>;
    /// Fixes the plan (host).
    LobbyStart = "lobby.start" (StartLobbyParams) -> LobbyView;
    /// Leaves or cancels a lobby.
    LobbyLeave = "lobby.leave" (LobbyParams) -> ();
    /// Runs the founding of a started lobby.
    FoundingBegin = "founding.begin" (LobbyParams) -> FoundingState;
    /// A founding's state.
    FoundingGet = "founding.get" (LobbyParams) -> Option<FoundingState>;
    /// The member's vaults.
    VaultList = "vault.list" (Empty) -> Vec<VaultSummary>;
    /// One vault in full.
    VaultDetailsGet = "vault.details" (VaultParams) -> VaultDetails;
    /// Members' reachability.
    VaultPeers = "vault.peers" (VaultParams) -> Vec<MemberReachability>;
    /// The ECDH audit log.
    VaultAudit = "vault.audit" (VaultAuditParams) -> Vec<AuditView>;
    /// Proposals and received transfers, newest first.
    VaultActivity = "vault.activity" (VaultParams) -> Vec<ActivityEntry>;
    /// Sets a vault's alias.
    VaultAlias = "vault.alias" (AliasParams) -> Vec<VaultSummary>;
    /// Retires the viewing share the keeper kept for a vault whose newer committee it could not verify: after a
    /// rotation that removed this member without its keeper taking part.
    VaultRetire = "vault.retire" (VaultParams) -> Vec<VaultSummary>;
    /// Makes a vault the default.
    VaultUse = "vault.use" (VaultParams) -> Vec<VaultSummary>;
    /// The vault's token balances.
    TokensList = "tokens.list" (VaultParams) -> Vec<TokenBalance>;
    /// Tracks a token.
    TokensAdd = "tokens.add" (TokenParams) -> TokenInfo;
    /// Deploys a test token and mints to the vault (local network only).
    TokensDeployTest = "tokens.deployTest" (VaultParams) -> TokenInfo;
    /// The vault's proposals.
    ProposalsList = "proposals.list" (VaultParams) -> Vec<ProposalView>;
    /// One proposal.
    ProposalsGet = "proposals.get" (ProposalRef) -> ProposalView;
    /// Simulates a proposal before making it.
    ProposalsPreview = "proposals.preview" (ProposeParams) -> ProposalPreview;
    /// Makes a proposal.
    ProposalsCreate = "proposals.create" (ProposeParams) -> ProposalView;
    /// Rebuilds and checks a proposal before voting.
    ProposalsReview = "proposals.review" (ProposalRef) -> Review;
    /// Approves.
    ProposalsApprove = "proposals.approve" (ApproveParams) -> ProposalView;
    /// Rejects.
    ProposalsReject = "proposals.reject" (RejectParams) -> ProposalView;
    /// Executes a ready proposal from this device.
    ProposalsExecute = "proposals.execute" (ProposalRef) -> ProposalView;
    /// Proposes cancelling a proposal.
    ProposalsCancel = "proposals.cancel" (ProposalRef) -> ProposalView;
    /// A contract's functions.
    ContractsFunctions = "contracts.functions" (ContractParams) -> Vec<ContractFunction>;
    /// The address book.
    ContactsList = "contacts.list" (Empty) -> Vec<Contact>;
    /// Adds or relabels a contact.
    ContactsPut = "contacts.put" (ContactParams) -> Vec<Contact>;
    /// Removes a contact.
    ContactsRemove = "contacts.remove" (AddressParams) -> Vec<Contact>;
    /// A join request for this member, to hand to a member of a vault it should join.
    MembershipRequest = "membership.request" (Empty) -> crate::keeper::JoinRequestText;
    /// Plans a membership change.
    MembershipPlanGet = "membership.plan" (MembershipParams) -> MembershipPlan;
    /// Takes part in a planned rotation: reshare through the keeper, then propose or approve `rotate_membership`.
    MembershipBegin = "membership.begin" (RotationPlanText) -> RotationState;
    /// A rotation's state .
    MembershipGet = "membership.get" (RotationRef) -> Option<RotationState>;
    /// The vault's rotations .
    MembershipList = "membership.list" (VaultParams) -> Vec<RotationState>;
    /// Proposes replacing one slot's signing key with a new device's, keeping the committee: no reshare.
    MembershipSwapSigner = "membership.swapSigner" (SwapSignerParams) -> ProposalView;
    /// A one-time offer to pair a signing device, to show as a QR code.
    DevicesPair = "devices.pair" (Empty) -> DeviceOffer;
    /// Paired devices.
    DevicesList = "devices.list" (Empty) -> Vec<DeviceView>;
    /// Unpairs a device.
    DevicesRevoke = "devices.revoke" (DeviceParams) -> ();
    /// Proposes moving this member's signing key in a vault to a paired device.
    DevicesEnroll = "devices.enroll" (EnrollParams) -> ProposalView;
    /// Sends a proposal to the paired device that holds this member's key, now.
    DevicesForward = "devices.forward" (ProposalRef) -> DeviceReviewSent;
}
