//! `bump_version`: same policy, next version, so every approval collected so far stops verifying.

use crate::governance::bump_version;
use crate::test::fixtures::policy::{CONFIG_VERSION, vault_config};

#[test]
fn increments_the_version_by_one() {
    assert_eq(bump_version(vault_config(3, 2, 2)).version, CONFIG_VERSION + 1);
}

#[test]
fn keeps_every_policy_field() {
    let config = vault_config(3, 2, 2);
    let next = bump_version(config);

    assert_eq(next.threshold, config.threshold);
    assert_eq(next.signer_count, config.signer_count);
    assert_eq(next.signer_set_commitment, config.signer_set_commitment);
    assert_eq(next.viewing_threshold, config.viewing_threshold);
    assert_eq(next.committee_commitment, config.committee_commitment);
}

#[test]
fn successive_bumps_keep_counting() {
    let next = bump_version(bump_version(bump_version(vault_config(3, 2, 2))));

    assert_eq(next.version, CONFIG_VERSION + 3);
}
