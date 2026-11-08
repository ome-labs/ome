//! An Ome member's approval signing key: the Grumpkin key whose Schnorr signatures the vault counts toward `k`.
//!
//! - [`SigningKey`] is generated on the device and never exported; it cannot be cloned, printed or serialized.
//! - [`SealedKey`] is its at-rest form: ChaCha20-Poly1305 under a key derived from a passphrase (Argon2id) or held by
//!   the platform (keychain, keystore), as a small JSON document the caller stores wherever the device keeps secrets.
//! - Signing goes only through [`approve`], [`approve_rotation`] and [`reject`], which run `ome-protocol`'s checks
//!   first. There is no way to sign an arbitrary hash.
//!
//! See `architecture/signer.md`.

mod error;
mod kdf;
mod key;
mod sealed;
mod sign;
mod signer;

pub use error::Error;
pub use kdf::{Argon2Params, SealingKey};
pub use key::SigningKey;
pub use sealed::{SEALED_VERSION, SealedKey, Secret};
pub use sign::{approve, approve_rotation, reject};
pub use signer::{Message, Signer};
