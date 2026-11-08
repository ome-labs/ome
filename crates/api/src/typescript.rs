//! The TypeScript declarations of both APIs, published as `@ome-labs/api`. One file: every type, then a method map
//! per service, the error codes and the API version.

use std::fmt::Write as _;

use ts_rs::{Config, TS};

use crate::{
    API_VERSION, Amount, AuthParams, Empty, ErrorData, ErrorKind, Hex16, Hex32, HexBytes,
    MethodInfo, Point, RpcError, daemon, keeper,
};

/// A declared type: its TypeScript name and declaration.
struct Declared {
    name: String,
    decl: String,
}

fn declared<T: TS + 'static>(cfg: &Config) -> Declared {
    let docs = T::docs().unwrap_or_default();
    Declared {
        name: T::ident(cfg),
        decl: format!("{docs}export {}", T::decl(cfg)),
    }
}

macro_rules! declare {
    ($cfg:expr; $($ty:ty),* $(,)?) => {
        vec![$(declared::<$ty>($cfg)),*]
    };
}

fn config() -> Config {
    Config::new().with_large_int("number")
}

fn types(cfg: &Config) -> Vec<Declared> {
    use daemon as d;
    use keeper as k;
    declare![cfg;
        // Shared.
        Hex32, Hex16, HexBytes, Point, Amount, Empty, AuthParams, ErrorKind, ErrorData, RpcError,
        // Keeper.
        k::KeeperInfo, k::KeeperContact, k::Member, k::Manifest, k::VaultStatus, k::Vault, k::VaultConfig,
        k::Observation, k::Observed, k::Reachability, k::AuditEntry, k::VaultParams, k::SessionParams,
        k::RegisterParams, k::ObserveConfigParams, k::AuditParams, k::NewMember, k::CeremonyKind,
        k::CeremonyStatus, k::Failure, k::DkgResult, k::ReshareResult, k::CeremonyResult, k::Ceremony,
        k::DkgParams, k::ReshareParams, k::EcdhPurpose, k::EcdhParams, k::Partial, k::MemberPartials,
        k::EcdhResult, k::BodyParams, k::ProposalStored, k::PayloadStored, k::IdParams, k::ProposalParams,
        k::PayloadParams, k::ReceiptParams, k::Receipt, k::TxParams, k::TxObserved, k::MailboxPolled,
        k::LobbyMemberParams, k::LobbyOpenParams, k::LobbyOpened, k::LobbyJoinParams, k::LobbyJoined,
        k::InviteParams, k::Invite, k::LobbyParams, k::LobbyStartParams, k::LobbyRole, k::LobbyStatus,
        k::LobbyMember, k::LobbyPlan, k::Lobby, k::KeeperEvent,
        // Daemon.
        d::DaemonInfo, d::Settings, d::SettingsPatch, d::RelaysParams, d::Profile, d::ProfileCreateParams,
        d::ServiceStatus, d::KeeperState, d::NodeStatus, d::SignerStatus, d::Secret, d::UnlockParams,
        d::PassphraseParams, d::SealingKey, d::Status, d::VaultSummary, d::ContractInfo, d::NamedKey,
        d::ConfigView, d::CommitteeMember, d::CommitteeView, d::FoundingInfo, d::VaultDetails,
        d::MemberReachability, d::AuditView, d::TokenInfo, d::TokenBalance, d::ActivityEntry, d::Contact,
        d::InviteHost, d::InvitePreview, d::LobbyViewMember, d::LobbyView, d::LobbyCreateParams,
        d::StartLobbyParams, d::FoundingStepId, d::StepStatus, d::FoundingStep, d::FoundingError,
        d::FoundingState, d::IntentCall, d::Intent, d::ProposalStatus, d::Vote, d::DecodedCall, d::Proposer,
        d::Execution, d::ProposalView, d::Gas, d::ReviewChecks, d::LabelSource, d::Recipient, d::Simulation,
        d::Signs, d::Review, d::ProposalPreview, d::ProposalRef, d::ProposeParams, d::ApproveParams,
        d::RejectParams, d::ExecutionPhase, d::ExecutionProgress, d::ContractFunction, d::FunctionKind,
        d::FunctionParam, d::Joiner, d::MembershipParams, d::CommitteeShape, d::MembershipPlan,
        d::RotationRef, d::RotationStepId, d::RotationStep, d::RotationState, d::DaemonEvent,
        d::VaultAuditParams, d::AliasParams, d::TokenParams, d::ContractParams, d::ContactParams,
        d::AddressParams,
    ]
}

/// The name of every declared type, in file order.
#[must_use]
pub fn declared_names() -> Vec<String> {
    types(&config()).into_iter().map(|t| t.name).collect()
}

fn method_map(out: &mut String, cfg: &Config, name: &str, doc: &str, methods: &[MethodInfo]) {
    let _ = writeln!(out, "/** {doc} */\nexport interface {name} {{");
    for method in methods {
        let _ = writeln!(
            out,
            "  \"{}\": {{ params: {}; result: {} }};",
            method.name,
            (method.params)(cfg),
            (method.result)(cfg)
        );
    }
    out.push_str("}\n\n");
}

/// The generated file, `packages/api/src/generated/api.ts`.
#[must_use]
pub fn render() -> String {
    let cfg = config();
    let mut out = String::from(
        "// Generated from crates/api by `pnpm --filter @ome-labs/api generate`. Do not edit.\n\n",
    );
    for declared in types(&cfg) {
        out.push_str(&declared.decl);
        out.push_str("\n\n");
    }
    method_map(
        &mut out,
        &cfg,
        "KeeperMethods",
        "Every keeper method: what it takes and returns.",
        keeper::METHODS,
    );
    method_map(
        &mut out,
        &cfg,
        "DaemonMethods",
        "Every daemon method: what it takes and returns.",
        daemon::METHODS,
    );
    out.push_str("/** JSON-RPC error code of each kind. */\nexport const ERROR_CODES = {\n");
    for kind in ErrorKind::ALL {
        let label = serde_json::to_value(kind).unwrap_or_default();
        let _ = writeln!(out, "  {label}: {},", kind.code());
    }
    out.push_str("} as const satisfies Record<ErrorKind, number>;\n\n");
    let _ = writeln!(
        out,
        "/** API version both services report. */\nexport const API_VERSION = {API_VERSION};"
    );
    out
}
