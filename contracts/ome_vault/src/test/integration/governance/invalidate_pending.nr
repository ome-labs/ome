//! `invalidate_pending`: the emergency stop. Bumps the version without changing the policy, so every approval
//! gathered so far stops verifying while new ones work.

use crate::OmeVault;
use crate::test::fixtures::{approvals::first, vault::{OUTER_HASH, TestVault}};

#[test]
unconstrained fn bumps_the_version_only() {
    let vault = TestVault::found(3, 2, 2);
    let before = vault.config();

    vault.self_call(OmeVault::at(vault.address).invalidate_pending());

    let after = vault.config();
    assert_eq(after.version, before.version + 1);
    assert_eq(after.threshold, before.threshold);
    assert_eq(after.signer_count, before.signer_count);
    assert_eq(after.signer_set_commitment, before.signer_set_commitment);
    assert_eq(after.viewing_threshold, before.viewing_threshold);
    assert_eq(after.committee_commitment, before.committee_commitment);
}

#[test(should_fail_with = "invalid signature")]
unconstrained fn voids_approvals_gathered_before() {
    let vault = TestVault::found(3, 2, 2);
    let gathered = vault.approve(0, first(3));

    vault.self_call(OmeVault::at(vault.address).invalidate_pending());

    let _ = vault.authorize(OUTER_HASH, gathered);
}

#[test]
unconstrained fn new_approvals_work_afterwards() {
    let vault = TestVault::found(3, 2, 2);
    vault.self_call(OmeVault::at(vault.address).invalidate_pending());

    assert(vault.authorize(OUTER_HASH, vault.approve(1, first(2))));
}

#[test]
unconstrained fn can_be_repeated() {
    let vault = TestVault::found(3, 2, 2);

    for _ in 0..3 {
        vault.self_call(OmeVault::at(vault.address).invalidate_pending());
    }

    assert_eq(vault.config().version, 3);
    assert(vault.authorize(OUTER_HASH, vault.approve(3, first(2))));
}
