//! Cryptography for Ome vaults: the primitives behind threshold viewing, byte-compatible with Aztec 6.0.0.
//!
//! - [`curve`], [`poseidon2`]: Grumpkin points and scalars with Aztec's encodings, and Aztec's Poseidon2.
//! - [`aztec`]: preaddress, address context and the address-secret sign rule, app siloing.
//! - [`schnorr`]: Aztec's Grumpkin Schnorr signatures, the scheme vault members approve with.
//! - [`threshold`]: Shamir sharing with Feldman commitments, distributed key generation, threshold ECDH with DLEQ
//!   proofs, and resharing to a new committee.
//! - [`sealing`]: authenticated envelopes and relay mailboxes.
//!
//! Secret scalars are [`SecretScalar`]: zeroized on drop, never printed or cloned, and multiplied with a constant-time
//! ladder. Randomness is always a caller-provided [`rand_core::CryptoRng`], so the crate runs unchanged natively, in
//! WebAssembly and on mobile.

pub mod aztec;
pub mod curve;
mod error;
pub mod poseidon2;
pub mod schnorr;
pub mod sealing;
mod secret;
pub mod threshold;

pub use curve::{Base, Point, Scalar};
pub use error::Error;
pub use secret::SecretScalar;
