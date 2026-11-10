//! The daemon itself: its profile, settings, services, node and signer.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{FeesStatus, VaultSummary};
use crate::{Hex32, Point};

/// What `daemon.info` reports.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DaemonInfo {
    /// The API version this daemon serves, [`crate::API_VERSION`].
    pub api_version: u32,
    /// Daemon release.
    pub version: String,
    /// Process id.
    pub pid: u32,
    /// Profile name.
    pub profile: String,
    /// Profile directory.
    pub profile_dir: String,
    /// Unix seconds the daemon started.
    pub started_at: u64,
    /// Aztec node URL.
    pub node: String,
    /// What the daemon accepts.
    pub limits: super::Limits,
    /// The features this daemon has (`authwits`, `apps`, …): a client checks for one instead of comparing versions,
    /// and a client newer than its daemon hides what the daemon lacks.
    pub capabilities: Vec<String>,
}

/// Daemon settings.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    /// Aztec node JSON-RPC URL.
    pub node_url: String,
    /// Relay multiaddrs, each ending in `/p2p/<peer id>`.
    pub relays: Vec<String>,
    /// Prove transactions on this device (off for a local network, which accepts fake proofs).
    pub prover_enabled: bool,
    /// Longest signer unlock window.
    pub unlock_timeout_secs: u32,
    /// Salt of the `SponsoredFPC` instance that pays this member's transactions: zero for the canonical one (local
    /// network, Testnet), another for a member's own deployment where none is sponsored (Alpha).
    pub fee_payer_salt: Hex32,
    /// The keeper on another machine this daemon uses instead of running its own.
    pub remote_keeper: Option<RemoteKeeper>,
    /// `aztec-wallet`'s data directory, when not `~/.aztec/wallet`.
    pub aztec_wallet_dir: Option<String>,
    /// Also write vault aliases into `aztec-wallet`'s store (`accounts:<alias>`), so it can pay a vault by name.
    pub aztec_wallet_sync: bool,
    /// Answer app connections (`apps.*`); off by default.
    pub apps_enabled: bool,
}

/// Params of `config.set`: the settings to change.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SettingsPatch {
    /// Aztec node URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub node_url: Option<String>,
    /// Prove on this device.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub prover_enabled: Option<bool>,
    /// Longest unlock window, 60 to 86,400 seconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub unlock_timeout_secs: Option<u32>,
    /// Salt of the fee payer's `SponsoredFPC` instance.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub fee_payer_salt: Option<Hex32>,
    /// `aztec-wallet`'s data directory; empty for the default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub aztec_wallet_dir: Option<String>,
    /// Write vault aliases into `aztec-wallet`'s store.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub aztec_wallet_sync: Option<bool>,
    /// Answer app connections.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub apps_enabled: Option<bool>,
}

/// Params of `relays.set`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct RelaysParams {
    /// At most 8 relay multiaddrs.
    pub relays: Vec<String>,
}

/// The member this profile is.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    /// Display name.
    pub name: String,
    /// Public half of the approval signing key; the secret stays sealed in the profile.
    pub signer_key: Point,
    /// The member's own Aztec account, which pays for deployments.
    pub account: Hex32,
    /// Unix seconds.
    pub created_at: u64,
}

/// Params of `profile.create`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ProfileCreateParams {
    /// Display name, 1 to 48 characters.
    pub name: String,
    /// Aztec node URL.
    pub node_url: String,
    /// Passphrase that seals the signer, at least 8 characters.
    pub passphrase: String,
}

/// A supervised service's state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum ServiceStatus {
    /// Not running.
    Stopped,
    /// Starting.
    Starting,
    /// Running.
    Running,
    /// Restarting after a crash.
    Restarting,
    /// Gave up; see the error.
    Failed,
}

/// The member's keeper, as the daemon supervises it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct KeeperState {
    /// Service state.
    pub status: ServiceStatus,
    /// Keeper identity, once running.
    pub identity: Option<Hex32>,
    /// libp2p peer id, once running.
    pub peer_id: Option<String>,
    /// Advertised addresses.
    pub addresses: Vec<String>,
    /// Keeper release.
    pub version: Option<String>,
    /// Last failure.
    pub error: Option<String>,
    /// The remote keeper's `host:port`, when the keeper runs on another machine; `None` for the daemon's own.
    pub remote: Option<String>,
}

/// A keeper on another machine (`ome-keeper run --rpc-listen`), reached over TLS.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct RemoteKeeper {
    /// `host:port`.
    pub address: String,
    /// SHA-256 fingerprint of its certificate (hex), as the keeper prints it.
    pub fingerprint: String,
}

/// A remote keeper with its RPC token (`<data>/rpc.token` on the keeper's machine).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct RemoteKeeperParams {
    /// `host:port`.
    pub address: String,
    /// SHA-256 fingerprint of its certificate (hex).
    pub fingerprint: String,
    /// The keeper's RPC token; the daemon keeps it sealed.
    pub token: String,
}

/// Params of `keeper.connect`: a remote keeper, or `null` for the daemon's own.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct KeeperConnectParams {
    /// The keeper to use from now on.
    pub remote: Option<RemoteKeeperParams>,
}

/// The Aztec node the daemon talks to.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct NodeStatus {
    /// Node URL.
    pub url: String,
    /// Whether the last check reached it.
    pub reachable: bool,
    /// Node version.
    pub node_version: Option<String>,
    /// L1 chain id.
    pub l1_chain_id: Option<u64>,
    /// Rollup version.
    pub rollup_version: Option<u64>,
    /// Latest block.
    pub block_number: Option<u64>,
    /// PXE's synced block.
    pub synced_block: Option<u64>,
    /// Unix seconds of the last check.
    pub checked_at: u64,
    /// Why it is unreachable.
    pub error: Option<String>,
}

/// Whether the signer can sign without a secret.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SignerStatus {
    /// Locked.
    pub locked: bool,
    /// Unix seconds an unlock window ends.
    pub unlocked_until: Option<u64>,
}

/// How a request unseals the signer: the passphrase, or the sealing key derived from it (`signer.sealingKey`), which
/// lets a client cache the key in a keychain instead of the passphrase.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(untagged)]
pub enum Secret {
    /// The passphrase.
    Passphrase {
        /// Passphrase.
        passphrase: String,
    },
    /// The sealing key.
    Key {
        /// 32-byte sealing key.
        key: Hex32,
    },
}

/// Params of `signer.unlock`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct UnlockParams {
    /// How to unseal.
    pub secret: Secret,
    /// Window, at most the configured timeout.
    pub ttl_secs: u32,
}

/// Params of `signer.changePassphrase`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ChangePassphraseParams {
    /// The current passphrase, or the sealing key.
    pub current: Secret,
    /// The new passphrase, at least 8 characters.
    pub next: String,
}

/// Params of `profile.rename`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ProfileRenameParams {
    /// The member's display name, 1 to 48 characters; every vault's members see it.
    pub name: String,
}

/// Params of `profile.delete`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ProfileDeleteParams {
    /// The profile's directory name, typed again to confirm.
    pub confirm: String,
}

/// Params of `signer.sealingKey`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct PassphraseParams {
    /// Passphrase.
    pub passphrase: String,
}

/// The result of `signer.sealingKey`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct SealingKey {
    /// Key that unseals the signer in place of the passphrase.
    pub key: Hex32,
}

/// Everything `ome status` shows.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    /// The profile, once created.
    pub profile: Option<Profile>,
    /// Settings.
    pub settings: Settings,
    /// The keeper.
    pub keeper: KeeperState,
    /// The node.
    pub node: NodeStatus,
    /// The signer.
    pub signer: SignerStatus,
    /// The member's vaults.
    pub vaults: Vec<VaultSummary>,
    /// The vault commands use when none is named.
    pub default_vault: Option<Hex32>,
    /// The fee payer and its Fee Juice, once the daemon has a wallet.
    pub fees: Option<FeesStatus>,
}
