//! WebAssembly bindings of the Rust core for TypeScript, published as `@ome-labs/wasm` (Node and the browser).
//!
//! - Threshold reading ([`threshold`]): verify keepers' ECDH partials and combine them.
//! - Contract mirror ([`contract`]) and payload hashing ([`payload`]): what `OmeVault` computes.
//! - Proposals ([`proposals`]): build, check, decode, verify and tally.
//! - Signer ([`signer`]): generate and seal a key, and sign through an [`signer::UnlockedSigner`] that keeps the key
//!   in WebAssembly memory.
//! - Membership ([`membership`]): rotation checks and threshold warnings.
//!
//! Values cross as plain JavaScript objects in the `@ome-labs/api` shapes; failures throw an `OmeWasmError` carrying
//! the API error kind. The `testing` feature adds [`testing`], for tests only.

#![allow(
    clippy::missing_errors_doc,
    reason = "every export throws OmeWasmError; the docs become the JSDoc, where a per-function section is noise"
)]

pub mod contract;
mod convert;
mod error;
mod js;
pub mod membership;
pub mod payload;
pub mod proposals;
pub mod signer;
#[cfg(feature = "testing")]
pub mod testing;
pub mod threshold;
pub mod types;
pub mod typescript;

use wasm_bindgen::prelude::wasm_bindgen;

#[wasm_bindgen(typescript_custom_section)]
const TYPES: &str = include_str!("types.d.ts");
