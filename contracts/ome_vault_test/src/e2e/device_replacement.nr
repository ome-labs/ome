//! A member loses their device: the others rotate the new device in, the lost one can no longer approve, and the
//! member is back to paying.

use crate::fixtures::{
    account::{empty_payload, entrypoint_hash, run_entrypoint},
    approvals::slots,
    members::new_member,
    vault::{ONE_DAY, OUTER_HASH, TestVault},
};
use aztec::authwit::account::AccountFeePaymentMethodOptions;

#[test]
unconstrained fn a_lost_device_is_replaced_without_moving_funds() {
    let mut vault = TestVault::found(3, 2, 2);
    let lost = vault;

    let replaced = vault.rotate_to(vault.members.replacing(1, new_member(&mut vault.env)), 2, 2);

    // Whoever holds the lost device cannot rebuild a valid witness: its key is no longer in the signer set.
    assert_eq(replaced.address, lost.address);
    assert(replaced.signer_set().keys[1] != lost.members.keys[1]);

    // The member's new device signs a payment together with another member.
    let payment = empty_payload(9);
    let expires_at = replaced.now() + ONE_DAY;
    let approval = replaced.approval_message(entrypoint_hash(replaced, payment), 1, expires_at);

    let _ = run_entrypoint(
        replaced,
        payment,
        AccountFeePaymentMethodOptions.EXTERNAL,
        false,
        replaced.witness(approval, slots([1, 2]), expires_at),
    );
    assert(replaced.authorize(OUTER_HASH, replaced.approve(1, slots([0, 1]))));
}

#[test(should_fail_with = "signer set mismatch")]
unconstrained fn the_lost_device_cannot_approve_anything() {
    let mut vault = TestVault::found(3, 2, 2);
    let _ = vault.rotate_to(vault.members.replacing(1, new_member(&mut vault.env)), 2, 2);

    // A witness built from the old member list, with the lost device's signature.
    let _ = vault.authorize(OUTER_HASH, vault.approve(1, slots([0, 1])));
}
