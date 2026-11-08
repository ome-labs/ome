//! `rotate_membership`: members join, leave, change devices and swap seats, and the stored policy and signer notes
//! follow in one step.

use crate::{committee::Committee, constants::MAX_SIGNERS, signer_set::SignerSet};
use crate::test::fixtures::{members::new_member, vault::TestVault};

#[test]
unconstrained fn a_member_joins() {
    let mut vault = TestVault::found(3, 2, 2);
    let joined = vault.members.joined_by(new_member(&mut vault.env));

    let rotated = vault.rotate_to(joined, 3, 3);

    let config = rotated.config();
    assert_eq(config.signer_count, 4);
    assert_eq(config.threshold, 3);
    assert_eq(config.viewing_threshold, 3);
    assert_eq(config.version, 1);
    assert_eq(config.signer_set_commitment, SignerSet { keys: joined.keys, count: 4 }.commitment());
}

#[test]
unconstrained fn the_last_member_leaves() {
    let vault = TestVault::found(3, 2, 2);

    let rotated = vault.rotate_to(vault.members.without(2), 2, 2);

    assert_eq(rotated.signer_set(), SignerSet { keys: rotated.members.keys, count: 2 });
}

#[test]
unconstrained fn the_first_member_leaves_and_the_others_move_up() {
    let vault = TestVault::found(3, 2, 2);
    let remaining = vault.members.without(0);

    let rotated = vault.rotate_to(remaining, 2, 2);

    assert_eq(rotated.signer_set().keys[0], vault.members.keys[1]);
    assert_eq(rotated.signer_set().keys[1], vault.members.keys[2]);
}

#[test]
unconstrained fn a_middle_member_leaves() {
    let vault = TestVault::found(4, 2, 2);

    let rotated = vault.rotate_to(vault.members.without(1), 2, 2);

    assert_eq(rotated.signer_set(), SignerSet { keys: rotated.members.keys, count: 3 });
}

#[test]
unconstrained fn a_member_moves_to_a_new_device() {
    let mut vault = TestVault::found(3, 2, 2);
    let moved = vault.members.replacing(1, new_member(&mut vault.env));

    let rotated = vault.rotate_to(moved, 2, 2);

    assert_eq(rotated.signer_set().keys[1], moved.keys[1]);
    assert(rotated.signer_set().keys[1] != vault.members.keys[1]);
}

#[test]
unconstrained fn members_swap_seats() {
    let vault = TestVault::found(3, 2, 2);

    let rotated = vault.rotate_to(vault.members.swapped(0, 2), 2, 2);

    assert_eq(rotated.signer_set().keys[0], vault.members.keys[2]);
    assert_eq(rotated.signer_set().keys[2], vault.members.keys[0]);
}

#[test]
unconstrained fn only_the_committee_and_thresholds_change() {
    let vault = TestVault::found(3, 2, 2);

    let rotated = vault.rotate_to(vault.members, 3, 3);

    let config = rotated.config();
    assert_eq(config.signer_set_commitment, vault.config().signer_set_commitment);
    assert_eq(config.threshold, 3);
    assert_eq(config.viewing_threshold, 3);
    assert_eq(
        config.committee_commitment,
        Committee { viewing_threshold: 3, shares: rotated.committee_shares, member_count: 3 }
            .commitment(),
    );
}

#[test]
unconstrained fn a_full_vault_changes_two_devices_within_call_limits() {
    let mut vault = TestVault::found(MAX_SIGNERS, 5, 3);
    let first_device = new_member(&mut vault.env);
    let last_device = new_member(&mut vault.env);
    let moved = vault.members.replacing(0, first_device).replacing(MAX_SIGNERS - 1, last_device);

    let rotated = vault.rotate_to(moved, 5, 3);

    assert_eq(rotated.signer_set(), SignerSet { keys: moved.keys, count: MAX_SIGNERS });
}

#[test]
unconstrained fn a_vault_grows_to_eight_members() {
    let mut vault = TestVault::found(2, 2, 2);
    let mut members = vault.members;
    for _ in 2..MAX_SIGNERS {
        members = members.joined_by(new_member(&mut vault.env));
    }

    let rotated = vault.rotate_to(members, 5, 4);

    assert_eq(rotated.config().signer_count, MAX_SIGNERS);
    assert_eq(rotated.signer_set().count, MAX_SIGNERS);
}

#[test]
unconstrained fn successive_rotations_advance_the_version() {
    let vault = TestVault::found(3, 2, 2);

    let once = vault.rotate_to(vault.members, 3, 2);
    let twice = once.rotate_to(once.members, 2, 3);

    let config = twice.config();
    assert_eq(config.version, 2);
    assert_eq(config.threshold, 2);
    assert_eq(config.viewing_threshold, 3);
}
