//! The phone core of an Ome member's signing device, exposed to Swift and Kotlin through `UniFFI`.
//!
//! A phone holds the member's signing key; the member's keeper and daemon keep running on a desktop or the member's
//! own server. The phone pairs with the keeper through a one-time offer, receives proposals to review through the
//! relay mailbox, checks everything it can itself (the committee, the calls it signs, each call's function, its own
//! slot, its policy, a rotation's shape) and signs only then. It never holds a viewing share, a PXE or a vault secret.
//!
//! The app implements [`DeviceStore`]: a few small files and two 32-byte sealing keys from the Keychain or the
//! Android Keystore. Everything else is [`OmeDevice`]. See `architecture/mobile.md`.

mod device;
mod error;
mod keys;
mod link;
mod policy;
mod review;
mod store;

pub use device::{
    DeviceInfo, OfferInfo, OmeDevice, PairingInfo, PublicKey, SyncResult, parse_pair_offer,
};
pub use error::FfiError;
pub use policy::{Pin, Policy, TokenLimit};
pub use review::{Argument, CallView, Check, Review, ReviewKind, ReviewStatus};
pub use store::{DeviceStore, KeyPurpose, StoreError};

uniffi::setup_scaffolding!();
