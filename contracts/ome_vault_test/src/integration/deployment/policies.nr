//! Founding a vault: the constructor stores the agreed policy and every member can read it back.

use crate::fixtures::vault::TestVault;
use ome_vault::{committee::Committee, constants::MAX_SIGNERS, signer_set::SignerSet};

#[test]
unconstrained fn stores_the_founding_policy_at_version_zero() {
    let vault = TestVault::found(3, 2, 2);
    let config = vault.config();

    assert_eq(config.threshold, 2);
    assert_eq(config.signer_count, 3);
    assert_eq(config.viewing_threshold, 2);
    assert_eq(config.version, 0);
}

#[test]
unconstrained fn commits_to_the_signer_keys_in_member_order() {
    let vault = TestVault::found(3, 2, 2);
    let expected = SignerSet { keys: vault.members.keys, count: 3 };

    assert_eq(vault.config().signer_set_commitment, expected.commitment());
}

#[test]
unconstrained fn commits_to_the_viewing_committee() {
    let vault = TestVault::found(3, 2, 2);
    let expected =
        Committee { viewing_threshold: 2, shares: vault.committee_shares, member_count: 3 };

    assert_eq(vault.config().committee_commitment, expected.commitment());
}

#[test]
unconstrained fn members_can_rebuild_the_ordered_signer_set() {
    let vault = TestVault::found(3, 2, 2);

    assert_eq(vault.signer_set(), SignerSet { keys: vault.members.keys, count: 3 });
}

#[test]
unconstrained fn founds_the_smallest_vault_two_of_two() {
    let config = TestVault::found(2, 2, 2).config();

    assert_eq(config.signer_count, 2);
    assert_eq(config.threshold, 2);
}

#[test]
unconstrained fn founds_a_vault_any_single_member_can_spend_from() {
    // k = 1 is a spending choice; reading still needs two members.
    let config = TestVault::found(3, 1, 2).config();

    assert_eq(config.threshold, 1);
    assert_eq(config.viewing_threshold, 2);
}

#[test]
unconstrained fn founds_a_vault_with_a_spending_threshold_below_the_viewing_threshold() {
    let config = TestVault::found(5, 2, 4).config();

    assert_eq(config.threshold, 2);
    assert_eq(config.viewing_threshold, 4);
}

#[test]
unconstrained fn founds_a_full_vault() {
    let vault = TestVault::found(MAX_SIGNERS, MAX_SIGNERS, MAX_SIGNERS);
    let config = vault.config();

    assert_eq(config.signer_count, MAX_SIGNERS);
    assert_eq(config.threshold, MAX_SIGNERS);
    assert_eq(config.viewing_threshold, MAX_SIGNERS);
    assert_eq(vault.signer_set().count, MAX_SIGNERS);
}
