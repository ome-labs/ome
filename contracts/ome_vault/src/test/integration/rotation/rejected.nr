//! Rotations the contract refuses: stale views of the membership, too many removals at once, and policies that would
//! leave the vault unusable.

use crate::OmeVault;
use crate::test::fixtures::{
    members::new_member,
    points::{
        committee_holding, committee_of_degree, off_curve_point, point_from_scalar, ROTATED_SHARES,
    },
    vault::TestVault,
};

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

    let committee = vault.fresh_committee(3, 2);

    vault.self_call(OmeVault::at(vault.address).rotate_membership(
        stale.keys,
        vault.members.keys,
        3,
        2,
        2,
        committee.shares,
        committee.polynomial,
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
    let mut committee = vault.fresh_committee(3, 2);
    committee.shares[1] = off_curve_point();

    let _ = vault.rotate_with(vault.members, 2, 2, committee);
}

// The committee must hold the vault's viewing key: approvers check it, and so does the contract, so no set of
// approvals can install a committee that cannot read for the vault.

#[test(should_fail_with = "committee does not hold the vault's key")]
unconstrained fn a_committee_of_another_key() {
    let vault = TestVault::found(3, 2, 2);
    let other_key = point_from_scalar(77);
    let committee = committee_holding(other_key, 3, 2, ROTATED_SHARES);

    let _ = vault.rotate_with(vault.members, 2, 2, committee);
}

#[test(should_fail_with = "share off the committee polynomial")]
unconstrained fn a_share_off_the_polynomial() {
    let vault = TestVault::found(3, 2, 2);
    let mut committee = vault.fresh_committee(3, 2);
    committee.shares[2] = point_from_scalar(77);

    let _ = vault.rotate_with(vault.members, 2, 2, committee);
}

#[test(should_fail_with = "committee polynomial degree above t_v - 1")]
unconstrained fn a_polynomial_of_higher_degree_than_the_threshold() {
    // Degree 2 would need 3 members to read, while the config says 2 can.
    let vault = TestVault::found(3, 2, 2);
    let committee = committee_of_degree(vault.viewing_key, 3, 2, ROTATED_SHARES);

    let _ = vault.rotate_with(vault.members, 2, 2, committee);
}

#[test(should_fail_with = "committee polynomial coefficient missing or off curve")]
unconstrained fn a_polynomial_of_lower_degree_than_the_threshold() {
    // Degree 1 lets any 2 members read, while the config says 3 are needed.
    let vault = TestVault::found(4, 2, 3);
    let committee = committee_of_degree(vault.viewing_key, 4, 1, ROTATED_SHARES);

    let _ = vault.rotate_with(vault.members, 2, 3, committee);
}

#[test(should_fail_with = "committee polynomial coefficient missing or off curve")]
unconstrained fn a_polynomial_coefficient_off_the_curve() {
    let vault = TestVault::found(3, 2, 3);
    let mut committee = vault.fresh_committee(3, 3);
    committee.polynomial[2] = off_curve_point();

    let _ = vault.rotate_with(vault.members, 2, 3, committee);
}
