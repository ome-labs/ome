//! A member's paired signing devices: pairing offers, the device list, reviews sent to a device, and adopting a signer
//! swap. See `architecture/mobile.md`.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::VaultConfig;
use crate::{Hex32, Point};

/// Params of `devices.offer`: what the pairing offer tells the device about its member.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DeviceOfferParams {
    /// The member's display name.
    pub member: String,
    /// Network label, as in invites.
    pub network: String,
    /// Fee payer contracts the member's configuration trusts; the device allows a rotation to also call these.
    pub fee_payers: Vec<Hex32>,
}

/// A pairing offer: the `ome-pair-v0:` string to show as a QR code. It holds a one-time secret.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DeviceOffer {
    /// The offer string.
    pub offer: String,
    /// Unix time after which the keeper refuses it.
    pub expires_at: u64,
}

/// A paired signing device.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    /// The device's Ed25519 identity.
    pub identity: Hex32,
    /// Its X25519 transport key.
    pub transport: Hex32,
    /// The Grumpkin key it signs approvals with.
    pub signer_key: Point,
    /// Its display name.
    pub name: String,
    /// When it paired (unix seconds).
    pub paired_at: u64,
}

/// Params naming a device.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DeviceParams {
    /// The device's identity.
    pub identity: Hex32,
}

/// The chain a vault lives on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Chain {
    /// L1 chain id, as a field.
    pub chain_id: Hex32,
    /// Rollup version, as a field.
    pub version: Hex32,
}

/// A name the daemon knows for an address.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DeviceLabel {
    /// Address.
    pub address: Hex32,
    /// Label.
    pub label: String,
}

/// The rotation this member's keeper reshared, sent with a `rotate_membership` proposal.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DeviceRotation {
    /// New signer keys in slot order.
    pub signers: Vec<Point>,
    /// New spending threshold.
    pub threshold: u32,
    /// New viewing threshold.
    pub viewing_threshold: u32,
    /// The reshared committee's verification shares.
    pub verification_shares: Vec<Point>,
}

/// Params of `devices.review`: a stored proposal to send to the device that holds this member's signing key in the
/// vault, with what only the daemon knows.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DeviceReviewParams {
    /// Vault.
    pub vault: Hex32,
    /// Proposal id; the keeper must hold the proposal and its payload.
    pub proposal: Hex32,
    /// The vault's display name.
    pub vault_name: String,
    /// The config note as the daemon read it.
    pub config: VaultConfig,
    /// The vault's chain.
    pub chain: Chain,
    /// Names for the addresses involved.
    pub labels: Vec<DeviceLabel>,
    /// The daemon's simulation outcome.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub simulation: Option<String>,
    /// For a reshare rotation, the rotation this member's keeper reshared.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub rotation: Option<DeviceRotation>,
}

/// The result of `devices.review`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DeviceReviewSent {
    /// Devices the review was sent to.
    pub devices: u32,
}

/// Params of `vault.updateSigners`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct UpdateSignersParams {
    /// Vault.
    pub vault: Hex32,
    /// The executed signer-swap proposal.
    pub proposal: Hex32,
}
