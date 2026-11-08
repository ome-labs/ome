//! `Committee`: the viewing threshold bounds, share validation and the committee commitment.

use crate::{committee::Committee, constants::MAX_SIGNERS, point::Point};
use crate::test::fixtures::{
    points::{FOUNDING_SHARES, off_curve_point, point_from_scalar},
    policy::committee,
};

#[test]
fn accepts_the_minimum_viewing_threshold() {
    let _ = Committee::new(2, committee(2, 2, FOUNDING_SHARES).shares, 2);
}

#[test]
fn accepts_every_threshold_up_to_the_member_count() {
    for threshold in 2..MAX_SIGNERS + 1 {
        committee(threshold, MAX_SIGNERS, FOUNDING_SHARES).assert_valid();
    }
}

#[test]
fn new_keeps_threshold_shares_and_count() {
    let expected = committee(2, 3, FOUNDING_SHARES);
    let built = Committee::new(2, expected.shares, 3);

    assert_eq(built.viewing_threshold, 2);
    assert_eq(built.member_count, 3);
    assert_eq(built.shares, expected.shares);
}

#[test(should_fail_with = "viewing threshold out of range")]
fn rejects_threshold_zero() {
    committee(0, 3, FOUNDING_SHARES).assert_valid();
}

#[test(should_fail_with = "viewing threshold out of range")]
fn rejects_threshold_one_so_no_member_reads_alone() {
    committee(1, 3, FOUNDING_SHARES).assert_valid();
}

#[test(should_fail_with = "viewing threshold out of range")]
fn rejects_threshold_above_member_count() {
    committee(4, 3, FOUNDING_SHARES).assert_valid();
}

#[test(should_fail_with = "viewing threshold out of range")]
fn rejects_a_single_member_committee() {
    committee(1, 1, FOUNDING_SHARES).assert_valid();
}

#[test(should_fail_with = "empty committee key")]
fn rejects_an_empty_share_inside_count() {
    let mut c = committee(2, 3, FOUNDING_SHARES);
    c.shares[0] = Point::empty();

    c.assert_valid();
}

#[test(should_fail_with = "committee key not on curve")]
fn rejects_an_off_curve_share() {
    let mut c = committee(2, 3, FOUNDING_SHARES);
    c.shares[1] = off_curve_point();

    c.assert_valid();
}

#[test(should_fail_with = "duplicate committee")]
fn rejects_a_duplicate_share() {
    let mut c = committee(2, 3, FOUNDING_SHARES);
    c.shares[2] = c.shares[0];

    c.assert_valid();
}

#[test(should_fail_with = "non-empty committee slot past count")]
fn rejects_a_share_past_member_count() {
    let mut c = committee(2, 3, FOUNDING_SHARES);
    c.shares[MAX_SIGNERS - 1] = point_from_scalar(42);

    c.assert_valid();
}

#[test]
fn commitment_is_deterministic() {
    assert_eq(
        committee(2, 3, FOUNDING_SHARES).commitment(),
        committee(2, 3, FOUNDING_SHARES).commitment(),
    );
}

#[test]
fn commitment_binds_the_threshold() {
    assert(
        committee(2, 3, FOUNDING_SHARES).commitment()
            != committee(3, 3, FOUNDING_SHARES).commitment(),
    );
}

#[test]
fn commitment_depends_on_every_share() {
    let base = committee(2, MAX_SIGNERS, FOUNDING_SHARES);

    for slot in 0..MAX_SIGNERS {
        let mut changed = base;
        changed.shares[slot] = point_from_scalar(5000 + slot as Field);

        assert(changed.commitment() != base.commitment());
    }
}

#[test]
fn commitment_depends_on_share_order() {
    let base = committee(2, 3, FOUNDING_SHARES);
    let mut swapped = base;
    swapped.shares[0] = base.shares[1];
    swapped.shares[1] = base.shares[0];

    assert(swapped.commitment() != base.commitment());
}

#[test]
fn commitment_covers_threshold_and_shares_only() {
    // As with signer sets, zero padding makes the member count implicit.
    let base = committee(2, 3, FOUNDING_SHARES);
    let mut miscounted = base;
    miscounted.member_count = 4;

    assert_eq(miscounted.commitment(), base.commitment());
}
