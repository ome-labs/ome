//! `SignerSet`: validation of the ordered key list and its commitment.

use crate::{constants::MAX_SIGNERS, point::Point, signer_set::SignerSet};
use crate::test::fixtures::{
    points::{off_curve_point, point_from_scalar},
    policy::{FOUNDING_KEYS, signer_set},
};

#[test]
fn accepts_one_member() {
    let _ = SignerSet::new(signer_set(1, FOUNDING_KEYS).keys, 1);
}

#[test]
fn accepts_max_members() {
    let _ = SignerSet::new(signer_set(MAX_SIGNERS, FOUNDING_KEYS).keys, MAX_SIGNERS);
}

#[test]
fn new_keeps_keys_and_count() {
    let expected = signer_set(3, FOUNDING_KEYS);

    assert_eq(SignerSet::new(expected.keys, 3), expected);
}

#[test(should_fail_with = "signer count out of range")]
fn rejects_zero_members() {
    let _ = SignerSet::new([Point::empty(); MAX_SIGNERS], 0);
}

#[test(should_fail_with = "signer count out of range")]
fn rejects_more_than_max_members() {
    let _ = SignerSet::new(signer_set(MAX_SIGNERS, FOUNDING_KEYS).keys, MAX_SIGNERS + 1);
}

#[test(should_fail_with = "empty signer key")]
fn rejects_an_empty_key_inside_count() {
    let mut keys = signer_set(3, FOUNDING_KEYS).keys;
    keys[1] = Point::empty();

    let _ = SignerSet::new(keys, 3);
}

#[test(should_fail_with = "signer key not on curve")]
fn rejects_an_off_curve_key() {
    let mut keys = signer_set(3, FOUNDING_KEYS).keys;
    keys[2] = off_curve_point();

    let _ = SignerSet::new(keys, 3);
}

#[test(should_fail_with = "duplicate signer")]
fn rejects_a_duplicate_key() {
    let mut keys = signer_set(3, FOUNDING_KEYS).keys;
    keys[2] = keys[0];

    let _ = SignerSet::new(keys, 3);
}

#[test(should_fail_with = "non-empty signer slot past count")]
fn rejects_a_key_past_count() {
    let mut keys = signer_set(2, FOUNDING_KEYS).keys;
    keys[MAX_SIGNERS - 1] = point_from_scalar(42);

    let _ = SignerSet::new(keys, 2);
}

#[test]
fn commitment_is_deterministic() {
    assert_eq(signer_set(3, FOUNDING_KEYS).commitment(), signer_set(3, FOUNDING_KEYS).commitment());
}

#[test]
fn commitment_depends_on_every_key() {
    let base = signer_set(MAX_SIGNERS, FOUNDING_KEYS);

    for slot in 0..MAX_SIGNERS {
        let mut changed = base;
        changed.keys[slot] = point_from_scalar(1000 + slot as Field);

        assert(changed.commitment() != base.commitment());
    }
}

#[test]
fn commitment_depends_on_order() {
    let base = signer_set(2, FOUNDING_KEYS);
    let mut swapped = base;
    swapped.keys[0] = base.keys[1];
    swapped.keys[1] = base.keys[0];

    assert(swapped.commitment() != base.commitment());
}

#[test]
fn commitment_distinguishes_member_counts() {
    assert(signer_set(2, FOUNDING_KEYS).commitment() != signer_set(3, FOUNDING_KEYS).commitment());
}

#[test]
fn commitment_covers_keys_only() {
    // The count is not hashed: validation forces zero padding past it, so the keys alone determine the set.
    let set = signer_set(3, FOUNDING_KEYS);
    let mut miscounted = set;
    miscounted.count = 2;

    assert_eq(miscounted.commitment(), set.commitment());
}
