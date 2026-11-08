//! The constructor refuses thresholds and viewing committees that would leave the vault unusable or readable by one
//! member.

use crate::fixtures::{
    members::Members,
    points::{distinct_points, FOUNDING_SHARES, off_curve_point},
    vault::{deploy_vault, TestVault},
};
use aztec::test::helpers::test_environment::TestEnvironment;
use ome_vault::{constants::MAX_SIGNERS, point::Point};

/// Deploys a 2-of-3 vault (t_v = 2) with `shares` as its committee.
unconstrained fn found_with_shares(shares: [Point; MAX_SIGNERS]) {
    let mut env = TestEnvironment::new();
    let members = Members::create(&mut env, 3);

    let _ = deploy_vault(&mut env, members.keys, 3, 2, 2, shares);
}

#[test(should_fail_with = "threshold out of range")]
unconstrained fn rejects_spending_threshold_zero() {
    let _ = TestVault::found(3, 0, 2);
}

#[test(should_fail_with = "threshold out of range")]
unconstrained fn rejects_a_spending_threshold_above_the_member_count() {
    let _ = TestVault::found(3, 4, 2);
}

#[test(should_fail_with = "viewing threshold out of range")]
unconstrained fn rejects_viewing_threshold_one() {
    let _ = TestVault::found(3, 2, 1);
}

#[test(should_fail_with = "viewing threshold out of range")]
unconstrained fn rejects_a_viewing_threshold_above_the_member_count() {
    let _ = TestVault::found(3, 2, 4);
}

#[test(should_fail_with = "viewing threshold out of range")]
unconstrained fn rejects_a_single_member_vault() {
    let _ = TestVault::found(1, 1, 1);
}

#[test(should_fail_with = "committee key not on curve")]
unconstrained fn rejects_a_committee_share_off_the_curve() {
    let mut shares = distinct_points(3, FOUNDING_SHARES);
    shares[0] = off_curve_point();

    found_with_shares(shares);
}

#[test(should_fail_with = "duplicate committee")]
unconstrained fn rejects_two_members_with_the_same_share() {
    let mut shares = distinct_points(3, FOUNDING_SHARES);
    shares[2] = shares[1];

    found_with_shares(shares);
}

#[test(should_fail_with = "empty committee key")]
unconstrained fn rejects_a_member_without_a_share() {
    found_with_shares(distinct_points(2, FOUNDING_SHARES));
}

#[test(should_fail_with = "non-empty committee slot past count")]
unconstrained fn rejects_a_share_for_a_member_who_does_not_exist() {
    found_with_shares(distinct_points(4, FOUNDING_SHARES));
}
