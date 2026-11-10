//! The local APIs of an Ome member's services, defined once in Rust and generated as TypeScript (`@ome-labs/api`).
//!
//! - [`keeper`]: what a member's daemon calls on its own keeper.
//! - [`daemon`]: what the CLI and apps call on the member's daemon.
//!
//! Both are JSON-RPC 2.0 over an owner-only local socket ([`jsonrpc`]). Every method is a marker type implementing
//! [`Method`]; its params and result are the types here. Bytes are `0x` hex ([`Hex32`], [`HexBytes`]), points are
//! `{ x, y }`, amounts are decimal strings ([`Amount`]), times are Unix seconds, and a missing value is `null`.

mod amount;
#[cfg(feature = "core")]
pub mod core;
pub mod daemon;
mod error;
mod error_details;
mod hex;
pub mod jsonrpc;
pub mod keeper;
pub mod typescript;

pub use amount::Amount;
pub use error::{ErrorData, ErrorKind, RpcError};
pub use error_details::{ErrorDetails, Funds, NotReadyReason};
pub use hex::{Hex16, Hex32, Hex64, HexBytes, HexError, Point};
pub use jsonrpc::{AuthParams, Empty, Method, MethodInfo};

/// API version served in `keeper.info` and `daemon.info`. Adding a method, an optional param or a result field keeps
/// it; anything else that changes a shape needs the next version.
pub const API_VERSION: u32 = 0;

/// The Ome release these crates belong to: `ome`, `ome-keeper` and `omed` of one release report the same version.
/// release-please sets it, which leaves the crates' own Cargo version (and `Cargo.lock`) alone: the crates are not
/// published on their own.
pub const RELEASE_VERSION: &str = "0.2.0"; // x-release-please-version
