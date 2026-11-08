//! Rotations the contract refuses: stale views of the membership, too many removals at once, and policies that would
//! leave the vault unusable.

use crate::fixtures::{
    members::new_member,
    points::{distinct_points, off_curve_point, ROTATED_SHARES},
    vault::TestVault,
};
use ome_vault::OmeVault;

#[test(should_fail_with = "too many members removed")]
unconstrained fn removing_as_many_members_as_the_viewing_threshold_at_once() {
    let vault = TestVault::found(4, 2, 2);

    let _ = vault.rotate_to(vault.members.without(3).without(2), 2, 2);
}

#[test(should_fail_with = "too many members removed")]
unconstrained fn replacing_as_many_members_as_the_viewing_threshold_at_once() {
    let mut vault = TestVault::found(3, 2, 2);
    let replaced = vault.members.replacing(0, new_member(&mut vault.env)).replacing(
        1,
        new_member(&mut vault.env),
    );

    let _ = vault.rotate_to(replaced, 2, 2);
}

#[test]
unconstrained fn removing_fewer_than_the_viewing_threshold_is_allowed() {
    let vault = TestVault::found(5, 2, 3);

    let rotated = vault.rotate_to(vault.members.without(4).without(3), 2, 2);

    assert_eq(rotated.config().signer_count, 3);
}

#[test(should_fail_with = "current signer set mismatch")]
unconstrained fn a_rotation_based_on_a_stale_member_list() {
    let vault = TestVault::found(3, 2, 2);
    let stale = vault.members.swapped(0, 1);

    vault.self_call(OmeVault::at(vault.address).rotate_membership(
        stale.keys,
        vault.members.keys,
        3,
        2,
        2,
        distinct_points(3, ROTATED_SHARES),
    ));
}

#[test(should_fail_with = "current signer set mismatch")]
unconstrained fn replaying_a_rotation_after_it_landed() {
    let vault = TestVault::found(3, 2, 2);
    let _ = vault.rotate_to(vault.members.without(2), 2, 2);

    // The same call again names the founding members as current, who are no longer the stored set.
    let _ = vault.rotate_to(vault.members.without(2), 2, 2);
}

#[test(should_fail_with = "threshold out of range")]
unconstrained fn a_spending_threshold_above_the_new_member_count() {
    let vault = TestVault::found(3, 2, 2);

    let _ = vault.rotate_to(vault.members.without(2), 3, 2);
}

#[test(should_fail_with = "threshold out of range")]
unconstrained fn spending_threshold_zero() {
    let vault = TestVault::found(3, 2, 2);

    let _ = vault.rotate_to(vault.members, 0, 2);
}

#[test(should_fail_with = "viewing threshold out of range")]
unconstrained fn viewing_threshold_one() {
    let vault = TestVault::found(3, 2, 2);

    let _ = vault.rotate_to(vault.members, 2, 1);
}

#[test(should_fail_with = "viewing threshold out of range")]
unconstrained fn shrinking_below_the_minimum_committee() {
    let vault = TestVault::found(2, 2, 2);

    let _ = vault.rotate_to(vault.members.without(1), 1, 2);
}

#[test(should_fail_with = "duplicate signer")]
unconstrained fn listing_one_member_twice() {
    let vault = TestVault::found(3, 2, 2);
    let doubled = vault.members.replacing(2, vault.members.at(0));

    let _ = vault.rotate_to(doubled, 2, 2);
}

#[test(should_fail_with = "committee key not on curve")]
unconstrained fn a_committee_share_off_the_curve() {
    let vault = TestVault::found(3, 2, 2);
    let mut shares = distinct_points(3, ROTATED_SHARES);
    shares[1] = off_curve_point();

    vault.self_call(OmeVault::at(vault.address).rotate_membership(
        vault.members.keys,
        vault.members.keys,
        3,
        2,
        2,
        shares,
    ));
}
