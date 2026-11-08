//! After a rotation only the new membership approves, under the new version and thresholds.

use crate::fixtures::{
    approvals::{first, slots},
    members::new_member,
    vault::{OUTER_HASH, TestVault},
};

#[test(should_fail_with = "invalid signature")]
unconstrained fn approvals_collected_before_a_rotation_stop_working() {
    let vault = TestVault::found(3, 2, 2);
    let collected = vault.approve(0, first(3));

    let _ = vault.rotate_to(vault.members, 2, 2);

    let _ = vault.authorize(OUTER_HASH, collected);
}

#[test]
unconstrained fn a_newcomer_counts_towards_the_threshold() {
    let mut vault = TestVault::found(3, 2, 2);
    let joined = vault.members.joined_by(new_member(&mut vault.env));

    let rotated = vault.rotate_to(joined, 3, 3);

    assert(rotated.authorize(OUTER_HASH, rotated.approve(1, slots([0, 1, 3]))));
}

#[test(should_fail_with = "signer set mismatch")]
unconstrained fn a_witness_that_still_lists_a_removed_member_is_refused() {
    let vault = TestVault::found(3, 2, 2);
    let _ = vault.rotate_to(vault.members.without(2), 2, 2);

    let _ = vault.authorize(OUTER_HASH, vault.approve(1, first(3)));
}

#[test]
unconstrained fn the_remaining_members_approve_after_a_removal() {
    let vault = TestVault::found(3, 2, 2);

    let rotated = vault.rotate_to(vault.members.without(0), 2, 2);

    assert(rotated.authorize(OUTER_HASH, rotated.approve(1, first(2))));
}

#[test(should_fail_with = "signer set mismatch")]
unconstrained fn an_old_device_key_cannot_approve_after_a_device_change() {
    let mut vault = TestVault::found(3, 2, 2);
    let _ = vault.rotate_to(vault.members.replacing(1, new_member(&mut vault.env)), 2, 2);

    let _ = vault.authorize(OUTER_HASH, vault.approve(1, first(2)));
}

#[test]
unconstrained fn a_new_device_approves_after_a_device_change() {
    let mut vault = TestVault::found(3, 2, 2);

    let rotated = vault.rotate_to(vault.members.replacing(1, new_member(&mut vault.env)), 2, 2);

    assert(rotated.authorize(OUTER_HASH, rotated.approve(1, slots([1, 2]))));
}

#[test]
unconstrained fn approvals_follow_the_new_seats_after_a_swap() {
    let vault = TestVault::found(3, 2, 2);

    let rotated = vault.rotate_to(vault.members.swapped(0, 2), 2, 2);

    assert(rotated.authorize(OUTER_HASH, rotated.approve(1, slots([0, 2]))));
}

#[test]
unconstrained fn the_new_spending_threshold_applies() {
    let vault = TestVault::found(3, 2, 2);

    let rotated = vault.rotate_to(vault.members, 3, 2);

    assert(!rotated.authorize(OUTER_HASH, rotated.approve(1, first(2))));
    assert(rotated.authorize(OUTER_HASH, rotated.approve(1, first(3))));
}
