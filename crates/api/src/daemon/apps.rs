//! App connections: Aztec apps using one of the member's vaults through wallet-sdk's wallet calls. Reads and
//! simulations are answered from the vault's PXE; a transaction becomes a proposal the members approve.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::Hex32;

/// An app the member allowed to use a vault.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AppView {
    /// The app's web origin, `https://app.example` (scheme, host and port).
    pub origin: String,
    /// The vault it sees as its only account.
    pub vault: Hex32,
    /// A name the member gave it.
    pub name: Option<String>,
    /// Unix seconds it was allowed.
    pub allowed_at: u64,
}

/// Params of `apps.allow`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct AppAllowParams {
    /// The app's origin.
    pub origin: String,
    /// The vault it may use.
    pub vault: Hex32,
    /// A name for it, at most 64 bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub name: Option<String>,
}

/// Params naming an app.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct AppParams {
    /// The app's origin.
    pub origin: String,
}

/// Params of `apps.call`: one wallet call from an allowed app.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct AppCallParams {
    /// The app's origin.
    pub origin: String,
    /// The `Wallet` method, e.g. `simulateTx`.
    pub method: String,
    /// Its arguments as aztec.js encodes them (`jsonStringify` of the argument list).
    pub args: String,
}

/// The result of `apps.call`, as aztec.js encodes it (`jsonStringify`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct AppCallResult {
    /// The method's result.
    pub result: String,
}
