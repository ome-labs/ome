//! Who is in a vault: a manifest checked against the on-chain config, rotation checks, policy warnings, and the
//! per-epoch tagging secret.

mod manifest;
pub mod policy;
mod rotation;
mod tagging;

pub use manifest::Vault;
pub use rotation::{Rotation, check_rotation, is_rotation_call, verify_shares};
pub use tagging::{EPOCH_TAGGING_TAG, epoch_tagging_secret};
