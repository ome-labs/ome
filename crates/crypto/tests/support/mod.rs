//! Shared by the integration tests: the Aztec golden vectors and seeded ceremonies.
// Each test binary uses a different subset of these helpers.
#![allow(
    dead_code,
    unreachable_pub,
    clippy::unwrap_used,
    clippy::missing_panics_doc,
    clippy::similar_names
)]

pub mod ceremony;
pub mod vectors;

use rand_chacha::ChaCha20Rng;
use rand_core::SeedableRng;

/// A deterministic random source, so a failing test fails the same way every run.
pub fn seeded(seed: u64) -> ChaCha20Rng {
    ChaCha20Rng::seed_from_u64(seed)
}
