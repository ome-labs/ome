//! `assert_threshold` and `with_threshold`: the spending threshold `k` must satisfy `1 <= k <= n`.

use crate::{constants::MAX_SIGNERS, governance::{assert_threshold, with_threshold}};
use crate::test::fixtures::policy::{CONFIG_VERSION, vault_config};

#[test]
fn accepts_every_threshold_from_one_to_the_signer_count() {
    for signer_count in 1..MAX_SIGNERS + 1 {
        for threshold in 1..MAX_SIGNERS + 1 {
            if threshold <= signer_count {
                assert_threshold(threshold, signer_count);
            }
        }
    }
}

#[test(should_fail_with = "threshold out of range")]
fn rejects_threshold_zero() {
    assert_threshold(0, 3);
}

#[test(should_fail_with = "threshold out of range")]
fn rejects_threshold_above_the_signer_count() {
    assert_threshold(4, 3);
}

#[test(should_fail_with = "threshold out of range")]
fn rejects_any_threshold_for_zero_signers() {
    assert_threshold(1, 0);
}

#[test]
fn with_threshold_sets_k_and_bumps_the_version() {
    let next = with_threshold(vault_config(3, 2, 2), 3);

    assert_eq(next.threshold, 3);
    assert_eq(next.version, CONFIG_VERSION + 1);
}

#[test]
fn with_threshold_keeps_signers_and_committee() {
    let config = vault_config(3, 2, 2);
    let next = with_threshold(config, 1);

    assert_eq(next.signer_count, config.signer_count);
    assert_eq(next.signer_set_commitment, config.signer_set_commitment);
    assert_eq(next.viewing_threshold, config.viewing_threshold);
    assert_eq(next.committee_commitment, config.committee_commitment);
}

#[test]
fn with_threshold_accepts_the_current_threshold_as_a_version_bump() {
    let next = with_threshold(vault_config(3, 2, 2), 2);

    assert_eq(next.threshold, 2);
    assert_eq(next.version, CONFIG_VERSION + 1);
}

#[test(should_fail_with = "threshold out of range")]
fn with_threshold_rejects_zero() {
    let _ = with_threshold(vault_config(3, 2, 2), 0);
}

#[test(should_fail_with = "threshold out of range")]
fn with_threshold_rejects_more_than_the_signer_count() {
    let _ = with_threshold(vault_config(3, 2, 2), 4);
}
