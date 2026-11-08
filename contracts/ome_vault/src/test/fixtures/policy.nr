//! Signer sets, committees and config notes built from deterministic points, for tests that never sign.

use crate::{committee::Committee, notes::config_note::ConfigNote, signer_set::SignerSet};
use crate::test::fixtures::points::{distinct_points, FOUNDING_SHARES};

/// Seed of the signer keys a policy fixture is founded with.
pub global FOUNDING_KEYS: Field = 1;

/// The founding version of `vault_config`, deliberately non-zero so version arithmetic is visible.
pub global CONFIG_VERSION: u32 = 5;

/// `count` distinct signer keys derived from `seed`.
pub fn signer_set(count: u32, seed: Field) -> SignerSet {
    SignerSet { keys: distinct_points(count, seed), count }
}

/// A committee of `member_count` distinct shares derived from `seed`.
pub fn committee(viewing_threshold: u32, member_count: u32, seed: Field) -> Committee {
    Committee { viewing_threshold, shares: distinct_points(member_count, seed), member_count }
}

/// Policy of a `threshold`-of-`member_count` vault with viewing threshold `viewing_threshold`, at `CONFIG_VERSION`.
pub fn vault_config(member_count: u32, threshold: u32, viewing_threshold: u32) -> ConfigNote {
    ConfigNote {
        threshold,
        signer_count: member_count,
        version: CONFIG_VERSION,
        signer_set_commitment: signer_set(member_count, FOUNDING_KEYS).commitment(),
        viewing_threshold,
        committee_commitment: committee(viewing_threshold, member_count, FOUNDING_SHARES)
            .commitment(),
    }
}
