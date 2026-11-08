//! A member notices approvals gathering for a payment they did not expect: the vault voids everything pending and
//! starts over with fresh approvals.

use crate::fixtures::{approvals::first, vault::{OUTER_HASH, TestVault}};
use ome_vault::OmeVault;

#[test]
unconstrained fn pending_approvals_are_voided_and_fresh_ones_work() {
    let vault = TestVault::found(3, 2, 2);

    vault.self_call(OmeVault::at(vault.address).invalidate_pending());

    assert(vault.authorize(OUTER_HASH, vault.approve(1, first(2))));
    assert_eq(vault.config().threshold, 2);
}

#[test(should_fail_with = "invalid signature")]
unconstrained fn the_voided_approvals_never_verify_again() {
    let vault = TestVault::found(3, 2, 2);
    let suspicious = vault.approve(0, first(2));

    vault.self_call(OmeVault::at(vault.address).invalidate_pending());

    let _ = vault.authorize(OUTER_HASH, suspicious);
}
