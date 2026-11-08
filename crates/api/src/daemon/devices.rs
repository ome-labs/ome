//! The member's paired signing devices, as the daemon shows and manages them. See `architecture/mobile.md`.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{Hex32, Point};

/// A device paired with this member's keeper.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DeviceView {
    /// The device's identity.
    pub identity: Hex32,
    /// Its name.
    pub name: String,
    /// The key it signs approvals with.
    pub signer_key: Point,
    /// When it paired (unix seconds).
    pub paired_at: u64,
    /// Its libp2p peer id, for a private relay's allow list.
    pub peer_id: String,
    /// Vaults where its key holds this member's slot: proposals there are sent to it for review.
    pub vaults: Vec<Hex32>,
}

/// Params of `devices.enroll`: move this member's signing key in `vault` to a paired device.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct EnrollParams {
    /// Vault.
    pub vault: Hex32,
    /// The paired device's identity.
    pub identity: Hex32,
}

/// Params of `membership.swapSigner`: replace one slot's signing key, keeping the committee and thresholds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SwapSignerParams {
    /// Vault.
    pub vault: Hex32,
    /// Signer slot (member index - 1).
    pub slot: u32,
    /// The new key, from the member's new device.
    pub signer_key: Point,
}
