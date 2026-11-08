//! How many valid approvals authorize a transaction.

use crate::fixtures::{approvals::{first, nobody, slots}, vault::{OUTER_HASH, TestVault}};
use ome_vault::constants::MAX_SIGNERS;

#[test]
unconstrained fn authorizes_with_exactly_k_approvals() {
    let vault = TestVault::found(3, 2, 2);

    assert(vault.authorize(OUTER_HASH, vault.approve(0, first(2))));
}

#[test]
unconstrained fn authorizes_with_more_than_k_approvals() {
    let vault = TestVault::found(3, 2, 2);

    assert(vault.authorize(OUTER_HASH, vault.approve(0, first(3))));
}

#[test]
unconstrained fn any_k_members_may_approve() {
    let vault = TestVault::found(3, 2, 2);

    assert(vault.authorize(OUTER_HASH, vault.approve(0, slots([0, 2]))));
    assert(vault.authorize(OUTER_HASH, vault.approve(0, slots([1, 2]))));
}

#[test]
unconstrained fn one_approval_is_enough_when_k_is_one() {
    let vault = TestVault::found(3, 1, 2);

    assert(vault.authorize(OUTER_HASH, vault.approve(0, slots([2]))));
}

#[test]
unconstrained fn authorizes_a_full_vault_with_every_member() {
    let vault = TestVault::found(MAX_SIGNERS, MAX_SIGNERS, 2);

    assert(vault.authorize(OUTER_HASH, vault.approve(0, first(MAX_SIGNERS))));
}

#[test]
unconstrained fn the_last_slot_of_a_full_vault_counts() {
    let vault = TestVault::found(MAX_SIGNERS, 2, 2);

    assert(vault.authorize(OUTER_HASH, vault.approve(0, slots([0, MAX_SIGNERS - 1]))));
}

#[test]
unconstrained fn does_not_authorize_with_one_approval_short() {
    let vault = TestVault::found(3, 2, 2);

    assert(!vault.authorize(OUTER_HASH, vault.approve(0, first(1))));
}

#[test]
unconstrained fn does_not_authorize_without_approvals() {
    let vault = TestVault::found(3, 1, 2);

    assert(!vault.authorize(OUTER_HASH, vault.approve(0, nobody())));
}

#[test]
unconstrained fn a_unanimous_vault_needs_every_member() {
    let vault = TestVault::found(4, 4, 2);

    assert(!vault.authorize(OUTER_HASH, vault.approve(0, first(3))));
    assert(vault.authorize(OUTER_HASH, vault.approve(0, first(4))));
}
