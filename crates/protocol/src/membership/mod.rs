//! Who is in a vault: a manifest checked against the on-chain config, rotation checks, and policy warnings.

mod manifest;
pub mod policy;
mod rotation;

pub use manifest::Vault;
pub use rotation::{Rotation, check_rotation, verify_shares};
