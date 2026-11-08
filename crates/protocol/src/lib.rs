//! The rules of an Ome vault, in one place, so every client decides the same way: the keeper, the daemon through
//! WebAssembly, a mobile approver and future apps.
//!
//! - [`vault`] mirrors the `OmeVault` contract: signer-set and committee commitments, config transitions, the approval
//!   message and the 49-field auth witness.
//! - [`payload`] recomputes the hash a vault transaction is authorized under from its calls, so a member checks what
//!   they sign instead of trusting the proposer.
//! - [`proposals`] checks proposals against their payload, verifies approvals and rejections, and tallies them.
//! - [`membership`] checks a manifest against the on-chain config, checks rotations, and warns about risky policies.
//! - [`lobby`] holds the founding rules: join proofs, sealed founding secrets and the vault's protocol version.
//!
//! Pure: no I/O and no clock (callers pass the time). See `architecture/protocol.md`.

mod error;
pub mod lobby;
pub mod membership;
pub mod payload;
pub mod proposals;
pub mod vault;

pub use error::Error;
