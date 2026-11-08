//! Shared by the integration tests: the golden vectors and a real 2-of-3 vault built from a DKG.
// Each test binary uses a different subset of these helpers.
#![allow(
    dead_code,
    unreachable_pub,
    clippy::unwrap_used,
    clippy::missing_panics_doc
)]

pub mod fixture;
pub mod vectors;

use rand_chacha::ChaCha20Rng;
use rand_core::SeedableRng;

/// A deterministic RNG.
pub fn seeded(seed: u64) -> ChaCha20Rng {
    ChaCha20Rng::seed_from_u64(seed)
}
