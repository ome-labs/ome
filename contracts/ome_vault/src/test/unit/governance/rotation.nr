//! `rotate`: replaces the signer set, both thresholds and the viewing committee in one step.

use crate::committee::Committee;
use crate::{
    constants::MAX_SIGNERS, governance::rotate, notes::config_note::ConfigNote, point::Point,
};
use crate::test::fixtures::{
    points::{committee_holding, off_curve_point, point_from_scalar, ROTATED_SHARES, TestCommittee},
    policy::{CONFIG_VERSION, FOUNDING_KEYS, signer_set, vault_config},
};

/// The vault's viewing key in these tests.
fn vault_key() -> Point {
    point_from_scalar(4242)
}

/// The fresh committee of `count` members a reshare produces, holding the vault's key.
fn reshared(count: u32, viewing_threshold: u32) -> TestCommittee {
    committee_holding(vault_key(), count, viewing_threshold, ROTATED_SHARES)
}

/// Rotates the founding `member_count`-member vault (viewing threshold `current_t_v`) to `new_keys`.
fn rotate_from(
    member_count: u32,
    current_t_v: u32,
    new_keys: [Point; MAX_SIGNERS],
    new_count: u32,
    threshold: u32,
    viewing_threshold: u32,
) -> ConfigNote {
    rotate(
        vault_config(member_count, 1, current_t_v),
        signer_set(member_count, FOUNDING_KEYS).keys,
        new_keys,
        new_count,
        threshold,
        viewing_threshold,
        reshared(new_count, viewing_threshold).shares,
        reshared(new_count, viewing_threshold).polynomial,
        vault_key(),
    )
}

/// The founding keys of a `count`-member vault.
fn founding_keys(count: u32) -> [Point; MAX_SIGNERS] {
    signer_set(count, FOUNDING_KEYS).keys
}

#[test]
fn adding_a_member_installs_the_whole_new_policy() {
    let next = rotate_from(3, 2, founding_keys(4), 4, 3, 3);

    assert_eq(next.signer_count, 4);
    assert_eq(next.threshold, 3);
    assert_eq(next.viewing_threshold, 3);
    assert_eq(next.version, CONFIG_VERSION + 1);
    assert_eq(next.signer_set_commitment, signer_set(4, FOUNDING_KEYS).commitment());
    let installed =
        Committee { viewing_threshold: 3, shares: reshared(4, 3).shares, member_count: 4 };
    assert_eq(next.committee_commitment, installed.commitment());
}

#[test]
fn removes_one_member_when_the_viewing_threshold_is_two() {
    let next = rotate_from(3, 2, founding_keys(2), 2, 2, 2);

    assert_eq(next.signer_count, 2);
    assert_eq(next.signer_set_commitment, signer_set(2, FOUNDING_KEYS).commitment());
}

#[test]
fn removes_two_members_when_the_viewing_threshold_is_three() {
    let next = rotate_from(5, 3, founding_keys(3), 3, 2, 2);

    assert_eq(next.signer_count, 3);
}

#[test]
fn replaces_one_members_key() {
    let mut keys = founding_keys(3);
    keys[1] = point_from_scalar(500);

    assert_eq(rotate_from(3, 2, keys, 3, 2, 2).signer_count, 3);
}

#[test]
fn reorders_members_without_removing_anyone() {
    let mut keys = founding_keys(3);
    keys[0] = founding_keys(3)[2];
    keys[2] = founding_keys(3)[0];

    let next = rotate_from(3, 2, keys, 3, 2, 2);

    assert(next.signer_set_commitment != signer_set(3, FOUNDING_KEYS).commitment());
}

#[test]
fn may_keep_the_same_members_to_rotate_the_committee_only() {
    let next = rotate_from(3, 2, founding_keys(3), 3, 2, 2);

    assert_eq(next.signer_set_commitment, signer_set(3, FOUNDING_KEYS).commitment());
    let installed =
        Committee { viewing_threshold: 2, shares: reshared(3, 2).shares, member_count: 3 };
    assert_eq(next.committee_commitment, installed.commitment());
    assert_eq(next.version, CONFIG_VERSION + 1);
}

#[test]
fn grows_to_the_maximum_size() {
    assert_eq(
        rotate_from(2, 2, founding_keys(MAX_SIGNERS), MAX_SIGNERS, 5, 4).signer_count,
        MAX_SIGNERS,
    );
}

#[test(should_fail_with = "too many members removed")]
fn rejects_removing_as_many_members_as_the_viewing_threshold() {
    let _ = rotate_from(4, 2, founding_keys(2), 2, 2, 2);
}

#[test(should_fail_with = "too many members removed")]
fn rejects_replacing_as_many_keys_as_the_viewing_threshold() {
    let mut keys = founding_keys(3);
    keys[0] = point_from_scalar(500);
    keys[1] = point_from_scalar(501);

    let _ = rotate_from(3, 2, keys, 3, 2, 2);
}

#[test(should_fail_with = "too many members removed")]
fn rejects_dropping_two_and_adding_two() {
    let mut keys = founding_keys(2);
    keys[1] = point_from_scalar(500);
    keys[2] = point_from_scalar(501);

    let _ = rotate_from(3, 2, keys, 3, 2, 2);
}

#[test(should_fail_with = "too many members removed")]
fn limits_removals_by_the_current_viewing_threshold_not_the_new_one() {
    // Raising t_v in the same rotation does not let more of the current committee leave.
    let _ = rotate_from(5, 2, founding_keys(3), 3, 2, 3);
}

#[test]
fn allows_lowering_the_viewing_threshold_while_removing_up_to_the_current_limit() {
    let next = rotate_from(5, 3, founding_keys(3), 3, 2, 2);

    assert_eq(next.viewing_threshold, 2);
}

#[test(should_fail_with = "current signer set mismatch")]
fn rejects_a_stale_current_signer_set() {
    let _ = rotate(
        vault_config(3, 2, 2),
        signer_set(3, FOUNDING_KEYS + 100).keys,
        founding_keys(3),
        3,
        2,
        2,
        reshared(3, 2).shares,
        reshared(3, 2).polynomial,
        vault_key(),
    );
}

#[test(should_fail_with = "current signer set mismatch")]
fn rejects_the_current_signers_in_another_order() {
    let mut current = founding_keys(3);
    current[0] = founding_keys(3)[1];
    current[1] = founding_keys(3)[0];

    let _ = rotate(
        vault_config(3, 2, 2),
        current,
        founding_keys(3),
        3,
        2,
        2,
        reshared(3, 2).shares,
        reshared(3, 2).polynomial,
        vault_key(),
    );
}

#[test(should_fail_with = "signer count out of range")]
fn rejects_an_empty_new_set() {
    let _ = rotate_from(3, 2, [Point::empty(); MAX_SIGNERS], 0, 1, 2);
}

#[test(should_fail_with = "duplicate signer")]
fn rejects_a_duplicate_new_signer() {
    let mut keys = founding_keys(3);
    keys[2] = keys[0];

    let _ = rotate_from(3, 2, keys, 3, 2, 2);
}

#[test(should_fail_with = "signer key not on curve")]
fn rejects_an_off_curve_new_signer() {
    let mut keys = founding_keys(4);
    keys[3] = off_curve_point();

    let _ = rotate_from(3, 2, keys, 4, 2, 2);
}

#[test(should_fail_with = "threshold out of range")]
fn rejects_threshold_zero() {
    let _ = rotate_from(3, 2, founding_keys(3), 3, 0, 2);
}

#[test(should_fail_with = "threshold out of range")]
fn rejects_threshold_above_the_new_member_count() {
    let _ = rotate_from(3, 2, founding_keys(3), 3, 4, 2);
}

#[test(should_fail_with = "viewing threshold out of range")]
fn rejects_viewing_threshold_one() {
    let _ = rotate_from(3, 2, founding_keys(3), 3, 2, 1);
}

#[test(should_fail_with = "viewing threshold out of range")]
fn rejects_viewing_threshold_above_the_new_member_count() {
    let _ = rotate_from(3, 2, founding_keys(3), 3, 2, 4);
}

#[test(should_fail_with = "empty committee key")]
fn rejects_fewer_committee_shares_than_members() {
    let _ = rotate(
        vault_config(3, 2, 2),
        founding_keys(3),
        founding_keys(4),
        4,
        2,
        2,
        reshared(3, 2).shares,
        reshared(3, 2).polynomial,
        vault_key(),
    );
}

#[test(should_fail_with = "non-empty committee slot past count")]
fn rejects_more_committee_shares_than_members() {
    let _ = rotate(
        vault_config(3, 2, 2),
        founding_keys(3),
        founding_keys(3),
        3,
        2,
        2,
        reshared(4, 2).shares,
        reshared(4, 2).polynomial,
        vault_key(),
    );
}

#[test(should_fail_with = "committee key not on curve")]
fn rejects_an_off_curve_committee_share() {
    let mut shares = reshared(3, 2).shares;
    shares[1] = off_curve_point();

    let _ = rotate(
        vault_config(3, 2, 2),
        founding_keys(3),
        founding_keys(3),
        3,
        2,
        2,
        shares,
        reshared(3, 2).polynomial,
        vault_key(),
    );
}

#[test(should_fail_with = "duplicate committee")]
fn rejects_a_duplicate_committee_share() {
    let mut shares = reshared(3, 2).shares;
    shares[2] = shares[0];

    let _ = rotate(
        vault_config(3, 2, 2),
        founding_keys(3),
        founding_keys(3),
        3,
        2,
        2,
        shares,
        reshared(3, 2).polynomial,
        vault_key(),
    );
}
