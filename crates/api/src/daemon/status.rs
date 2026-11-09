//! The daemon itself: its profile, settings, services, node and signer.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::VaultSummary;
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
}
