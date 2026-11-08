//! `member_count` and `removed_members`: the counts behind the rule that a rotation removes fewer than `t_v` members.

use crate::fixtures::{points::point_from_scalar, policy::{FOUNDING_KEYS, signer_set}};
use ome_vault::{
    constants::MAX_SIGNERS,
    governance::{member_count, removed_members},
    point::Point,
    signer_set::SignerSet,
};

#[test]
fn member_count_counts_occupied_slots() {
    for count in 0..MAX_SIGNERS + 1 {
        assert_eq(member_count(signer_set(count, FOUNDING_KEYS).keys), count);
    }
}

#[test]
fn member_count_is_zero_for_an_empty_list() {
    assert_eq(member_count([Point::empty(); MAX_SIGNERS]), 0);
}

#[test]
fn nobody_is_removed_when_the_set_is_unchanged() {
    let set = signer_set(3, FOUNDING_KEYS);

    assert_eq(removed_members(set, set), 0);
}

#[test]
fn nobody_is_removed_when_members_are_reordered() {
    let current = signer_set(3, FOUNDING_KEYS);
    let mut reordered = current;
    reordered.keys[0] = current.keys[2];
    reordered.keys[2] = current.keys[0];

    assert_eq(removed_members(current, reordered), 0);
}

#[test]
fn nobody_is_removed_when_members_join() {
    assert_eq(removed_members(signer_set(3, FOUNDING_KEYS), signer_set(5, FOUNDING_KEYS)), 0);
}

#[test]
fn counts_members_dropped_from_the_end() {
    assert_eq(removed_members(signer_set(3, FOUNDING_KEYS), signer_set(2, FOUNDING_KEYS)), 1);
    assert_eq(removed_members(signer_set(3, FOUNDING_KEYS), signer_set(1, FOUNDING_KEYS)), 2);
}

#[test]
fn counts_a_replaced_key_as_a_removal() {
    let current = signer_set(3, FOUNDING_KEYS);
    let mut next = current;
    next.keys[1] = point_from_scalar(500);

    assert_eq(removed_members(current, next), 1);
}

#[test]
fn counts_everyone_when_the_set_is_replaced() {
    assert_eq(
        removed_members(
            signer_set(4, FOUNDING_KEYS),
            signer_set(4, FOUNDING_KEYS + 100),
        ),
        4,
    );
}

#[test]
fn ignores_keys_past_the_next_count() {
    // A key left in a slot beyond `next.count` does not keep its member.
    let current = signer_set(3, FOUNDING_KEYS);
    let next = SignerSet { keys: current.keys, count: 2 };

    assert_eq(removed_members(current, next), 1);
}

#[test]
fn ignores_keys_past_the_current_count() {
    let mut current = signer_set(2, FOUNDING_KEYS);
    current.keys[5] = point_from_scalar(600);

    assert_eq(removed_members(current, signer_set(2, FOUNDING_KEYS)), 0);
}
