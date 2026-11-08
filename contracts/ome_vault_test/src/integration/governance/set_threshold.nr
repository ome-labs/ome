//! `set_threshold`: changes `k`, keeps members and committee, and voids every approval collected before.

use crate::fixtures::{approvals::first, vault::{OUTER_HASH, TestVault}};
use ome_vault::OmeVault;

#[test]
unconstrained fn raises_the_threshold() {
    let vault = TestVault::found(3, 2, 2);
    vault.self_call(OmeVault::at(vault.address).set_threshold(3));

    let config = vault.config();
    assert_eq(config.threshold, 3);
    assert_eq(config.version, 1);
}

#[test]
unconstrained fn lowers_the_threshold() {
    let vault = TestVault::found(3, 3, 2);
    vault.self_call(OmeVault::at(vault.address).set_threshold(1));

    assert_eq(vault.config().threshold, 1);
}

#[test]
unconstrained fn keeps_members_and_committee() {
    let vault = TestVault::found(3, 2, 2);
    let before = vault.config();

    vault.self_call(OmeVault::at(vault.address).set_threshold(3));

    let after = vault.config();
    assert_eq(after.signer_count, before.signer_count);
    assert_eq(after.signer_set_commitment, before.signer_set_commitment);
    assert_eq(after.viewing_threshold, before.viewing_threshold);
    assert_eq(after.committee_commitment, before.committee_commitment);
    assert_eq(vault.signer_set().keys, vault.members.keys);
}

#[test]
unconstrained fn enforces_the_new_threshold() {
    let vault = TestVault::found(3, 2, 2);
    vault.self_call(OmeVault::at(vault.address).set_threshold(3));

    assert(!vault.authorize(OUTER_HASH, vault.approve(1, first(2))));
    assert(vault.authorize(OUTER_HASH, vault.approve(1, first(3))));
}

#[test(should_fail_with = "invalid signature")]
unconstrained fn voids_approvals_collected_before_the_change() {
    let vault = TestVault::found(3, 2, 2);
    let collected = vault.approve(0, first(3));

    vault.self_call(OmeVault::at(vault.address).set_threshold(2));

    let _ = vault.authorize(OUTER_HASH, collected);
}

#[test(should_fail_with = "threshold out of range")]
unconstrained fn rejects_threshold_zero() {
    let vault = TestVault::found(3, 2, 2);
    vault.self_call(OmeVault::at(vault.address).set_threshold(0));
}

#[test(should_fail_with = "threshold out of range")]
unconstrained fn rejects_a_threshold_above_the_member_count() {
    let vault = TestVault::found(3, 2, 2);
    vault.self_call(OmeVault::at(vault.address).set_threshold(4));
}
