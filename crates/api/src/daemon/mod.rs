//! The daemon's API: what the CLI and apps call on a member's `omed`.

mod apps;
mod contracts;
mod devices;
mod events;
mod fees;
mod founding;
mod membership;
mod proposals;
mod receiving;
mod review;
mod status;
mod vaults;

pub use apps::{AppAllowParams, AppCallParams, AppCallResult, AppParams, AppView};
pub use contracts::{
    AbiField, AbiType, AbiValue, ContractFunction, ContractFunctionsParams, ContractInstanceInput,
    ContractRegisterParams, ContractSource, ContractView, DecodedArg, FunctionKind, FunctionParam,
    KnownType, Limits,
};
pub use devices::{DeviceView, EnrollParams, SwapSignerParams};
pub use events::{DaemonEvent, EventEnvelope, EventSeq, SubscribeParams, Subscription};
pub use fees::{FeeEstimate, FeePayerKind, FeesStatus};
pub use founding::{
    FoundingError, FoundingState, FoundingStep, FoundingStepId, InviteHost, InvitePreview,
    LobbyCreateParams, LobbyView, LobbyViewMember, StartLobbyParams, StepStatus,
};
pub use membership::{
    CommitteeShape, Joiner, MembershipParams, MembershipPlan, RotationPlanText, RotationRef,
    RotationState, RotationStep, RotationStepId,
};
pub use proposals::{
    ApproveParams, DecodedAuthwit, DecodedCall, ExecuteParams, Execution, ExecutionPhase,
    ExecutionProgress, ExecutionView, Gas, Intent, IntentAuthwit, IntentCall, LabelSource,
    ProposalPreview, ProposalRef, ProposalStatus, ProposalView, ProposeParams, Proposer, Recipient,
    RejectParams, Review, ReviewChecks, Signs, Simulation, Vote,
};
pub use receiving::{
    ActivityKind, ActivityPage, ActivityParams, AuditPage, BalanceState, CounterpartyPage,
    PageParams, PaymentRequest, PaymentRequestParams, ProposalCounts, ProposalFilter,
    ProposalListParams, ProposalPage, ReceivingStatus, TokenBalance,
};
pub use review::{
    BalanceChange, CallResult, Effects, GovernanceChange, NoteCounts, PolicyWarning, ReviewError,
    ReviewFailure, SimulationError, SimulationFailure, Visibility,
};
use serde::{Deserialize, Serialize};
pub use status::{
    ChangePassphraseParams, DaemonInfo, KeeperConnectParams, KeeperState, NodeStatus,
    PassphraseParams, Profile, ProfileCreateParams, ProfileDeleteParams, ProfileRenameParams,
    RelaysParams, RemoteKeeper, RemoteKeeperParams, SealingKey, Secret, ServiceStatus, Settings,
    SettingsPatch, SignerStatus, Status, UnlockParams,
};
use ts_rs::TS;
pub use vaults::{
    ActivityEntry, AuditView, CommitteeMember, CommitteeView, ConfigView, ContractInfo,
    Counterparty, FoundingInfo, MemberReachability, MemberRef, NamedKey, TokenInfo, VaultDetails,
    VaultSummary,
};

use crate::{
    AuthParams, Empty, Hex32,
    jsonrpc::methods,
    keeper::{
        DeviceOffer, DeviceParams, DeviceReviewSent, InviteParams, LobbyParams, LobbyRemoveParams,
        LobbyUpdateParams, VaultParams,
    },
};

/// Params of `vault.rename`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct RenameParams {
    /// Vault.
    pub vault: Hex32,
    /// The vault's new name, 1 to 48 characters, for every member.
    pub name: String,
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
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct TokenParams {
    /// Vault.
    pub vault: Hex32,
    /// Token contract.
    pub token: Hex32,
    /// How the token was deployed, for a token whose instance was never published: the daemon derives the instance
    /// from it and refuses one that derives another address.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub deployment: Option<TokenDeployment>,
}

/// A standard token's deployment: the `Token` constructor's arguments, its salt and its deployer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct TokenDeployment {
    /// Deployment salt.
    pub salt: Hex32,
    /// Deployer address (zero for a universal deployment).
    pub deployer: Hex32,
    /// The constructor's `admin`.
    pub admin: Hex32,
    /// Name.
    pub name: String,
    /// Symbol.
    pub symbol: String,
    /// Decimals.
    pub decimals: u8,
}

/// An alias in `aztec-wallet`'s store, such as `accounts:alice` or `contracts:usdc`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct AztecAlias {
    /// `<type>:<name>`.
    pub key: String,
    /// What it stands for: an address, or an artifact path.
    pub value: String,
}

/// Params of `tokens.remove`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct TokenRefParams {
    /// Vault.
    pub vault: Hex32,
    /// Token contract.
    pub token: Hex32,
}

/// Params naming a contract (`contracts.remove`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ContractParams {
    /// Contract.
    pub contract: Hex32,
}

/// A method the daemon serves.
pub trait DaemonMethod: crate::Method {}

methods! {
    /// Every daemon method, in table order.
    METHODS: DaemonMethod;
    /// Presents the session token. Must be the first call.
    SessionAuth = "session.auth" (AuthParams) -> ();
    /// Streams [`EventEnvelope`]s on this session, first replaying those after `since` when the daemon still has them.
    EventsSubscribe = "events.subscribe" (SubscribeParams) -> Subscription;
    /// Who this daemon is.
    DaemonInfoGet = "daemon.info" (Empty) -> DaemonInfo;
    /// Stops the daemon and its keeper.
    DaemonStop = "daemon.stop" (Empty) -> ();
    /// Everything `ome status` shows.
    StatusGet = "status" (Empty) -> Status;
    /// Creates the profile: the sealed signer and the member's account.
    ProfileCreate = "profile.create" (ProfileCreateParams) -> Status;
    /// Renames the member for every vault's members.
    ProfileRename = "profile.rename" (ProfileRenameParams) -> Profile;
    /// Deletes this profile and stops the daemon; refused while it is a member of a vault.
    ProfileDelete = "profile.delete" (ProfileDeleteParams) -> ();
    /// Settings.
    ConfigGet = "config.get" (Empty) -> Settings;
    /// Changes settings.
    ConfigSet = "config.set" (SettingsPatch) -> Settings;
    /// Replaces the relays.
    RelaysSet = "relays.set" (RelaysParams) -> Settings;
    /// Uses a keeper on another machine, or the daemon's own again.
    KeeperConnect = "keeper.connect" (KeeperConnectParams) -> KeeperState;
    /// Whether the signer is locked.
    SignerStatusGet = "signer.status" (Empty) -> SignerStatus;
    /// Unlocks the signer for a window.
    SignerUnlock = "signer.unlock" (UnlockParams) -> SignerStatus;
    /// Locks the signer.
    SignerLock = "signer.lock" (Empty) -> SignerStatus;
    /// Derives the key that unseals the signer in place of the passphrase.
    SignerSealingKey = "signer.sealingKey" (PassphraseParams) -> SealingKey;
    /// Re-seals the signing key (and the keeper's sealed master key) under a new passphrase; the key is unchanged.
    SignerChangePassphrase = "signer.changePassphrase" (ChangePassphraseParams) -> SignerStatus;
    /// Checks the node now.
    NodeStatusGet = "node.status" (Empty) -> NodeStatus;
    /// The fee payer this member trusts and its Fee Juice.
    FeesStatusGet = "fees.status" (Empty) -> FeesStatus;
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
    /// Removes a joiner from a lobby this member hosts; its invite stops working and a new one is in the view.
    LobbyRemove = "lobby.remove" (LobbyRemoveParams) -> LobbyView;
    /// Changes a lobby's size or thresholds before it starts.
    LobbyUpdate = "lobby.update" (LobbyUpdateParams) -> LobbyView;
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
    VaultAudit = "vault.audit" (PageParams) -> AuditPage;
    /// Proposals and received transfers, newest first, a page at a time.
    VaultActivity = "vault.activity" (ActivityParams) -> ActivityPage;
    /// Who has paid the vault, and whether a member removed since could still notice their payments.
    VaultCounterparties = "vault.counterparties" (PageParams) -> CounterpartyPage;
    /// What payments the vault finds, and when it last received one.
    VaultReceiving = "vault.receiving" (VaultParams) -> ReceivingStatus;
    /// A payment request: an `ome-pay:` link for a payer.
    VaultRequest = "vault.request" (PaymentRequestParams) -> PaymentRequest;
    /// Sets a vault's alias.
    VaultAlias = "vault.alias" (AliasParams) -> Vec<VaultSummary>;
    /// Renames the vault for every member: no proposal, the newest rename wins.
    VaultRename = "vault.rename" (RenameParams) -> VaultSummary;
    /// Retires the viewing share the keeper kept for a vault whose newer committee it could not verify: after a
    /// rotation that removed this member without its keeper taking part.
    VaultRetire = "vault.retire" (VaultParams) -> Vec<VaultSummary>;
    /// Makes a vault the default.
    VaultUse = "vault.use" (VaultParams) -> Vec<VaultSummary>;
    /// The vault's token balances.
    TokensList = "tokens.list" (VaultParams) -> Vec<TokenBalance>;
    /// Tracks a token.
    TokensAdd = "tokens.add" (TokenParams) -> TokenInfo;
    /// Stops tracking a token.
    TokensRemove = "tokens.remove" (TokenRefParams) -> Vec<TokenInfo>;
    /// Deploys a test token and mints to the vault (local network only).
    TokensDeployTest = "tokens.deployTest" (VaultParams) -> TokenInfo;
    /// The vault's proposals, newest first, a page at a time.
    ProposalsList = "proposals.list" (ProposalListParams) -> ProposalPage;
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
    /// Executes a ready proposal from this device; refuses with `conflict` while another member is executing it.
    ProposalsExecute = "proposals.execute" (ExecuteParams) -> ProposalView;
    /// Proposes cancelling a proposal.
    ProposalsCancel = "proposals.cancel" (ProposalRef) -> ProposalView;
    /// A contract's functions.
    ContractsFunctions = "contracts.functions" (ContractFunctionsParams) -> Vec<ContractFunction>;
    /// Registers a contract after checking its artifact is the code of the class at its address.
    ContractsRegister = "contracts.register" (ContractRegisterParams) -> ContractView;
    /// Every contract this member can call.
    ContractsList = "contracts.list" (Empty) -> Vec<ContractView>;
    /// Forgets a registered contract.
    ContractsRemove = "contracts.remove" (ContractParams) -> ();
    /// `aztec-wallet`'s aliases (`accounts:…`, `contracts:…`, `artifacts:…`), read from its store.
    AztecAliases = "aztec.aliases" (Empty) -> Vec<AztecAlias>;
    /// Writes an alias into `aztec-wallet`'s store, when `aztecWalletSync` is on.
    AztecAliasSet = "aztec.alias" (AztecAlias) -> ();
    /// A join request for this member, to hand to a member of a vault it should join.
    MembershipRequest = "membership.request" (Empty) -> crate::keeper::JoinRequestText;
    /// Plans a membership change.
    MembershipPlanGet = "membership.plan" (MembershipParams) -> MembershipPlan;
    /// Takes part in a planned rotation: reshare through the keeper, then propose or approve `rotate_membership`.
    MembershipBegin = "membership.begin" (RotationPlanText) -> RotationState;
    /// A rotation's state.
    MembershipGet = "membership.get" (RotationRef) -> Option<RotationState>;
    /// The vault's rotations.
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
    /// The apps allowed to use a vault.
    AppsList = "apps.list" (Empty) -> Vec<AppView>;
    /// Allows an app (by origin) to use a vault; app connections must be on (`config.set { appsEnabled }`).
    AppsAllow = "apps.allow" (AppAllowParams) -> AppView;
    /// Stops an app from using any vault.
    AppsRevoke = "apps.revoke" (AppParams) -> ();
    /// One wallet call from an allowed app: reads and simulations are answered, a transaction becomes a proposal and
    /// the call returns once it executes.
    AppsCall = "apps.call" (AppCallParams) -> AppCallResult;
}
