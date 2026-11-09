//! Who is in a vault: a manifest checked against the on-chain config, rotation and signer-swap checks, policy warnings,
//! and the per-epoch tagging secret.

mod manifest;
pub mod policy;
mod rotation;
mod swap;
mod tagging;

pub use manifest::Vault;
pub use rotation::{Rotation, check_rotation, is_rotation_call, polynomial, verify_shares};
pub use swap::{check_signer_swap, signer_swap_slot, swapped_manifest};
pub use tagging::{EPOCH_TAGGING_TAG, epoch_tagging_secret};
