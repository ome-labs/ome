//! Deadlines: an approval expires at its signed deadline, and an authorized transaction must land before both that
//! deadline and `CONFIG_MAX_AGE` after its anchor block, which bounds how long a superseded config can authorize.

use crate::constants::CONFIG_MAX_AGE;
use crate::test::fixtures::{approvals::first, vault::{ONE_DAY, OUTER_HASH, TestVault}};

/// A witness approving `OUTER_HASH` until `expires_at`.
unconstrained fn approved_until(vault: TestVault, expires_at: u64) -> [Field; 49] {
    vault.witness(vault.approval_message(OUTER_HASH, 0, expires_at), first(2), expires_at)
}

#[test]
unconstrained fn authorizes_at_the_deadline_itself() {
    let vault = TestVault::found(3, 2, 2);

    assert(vault.authorize(OUTER_HASH, approved_until(vault, vault.now())));
}

#[test(should_fail_with = "approval expired")]
unconstrained fn rejects_an_approval_one_second_past_its_deadline() {
    let vault = TestVault::found(3, 2, 2);

    let _ = vault.authorize(OUTER_HASH, approved_until(vault, vault.now() - 1));
}

#[test(should_fail_with = "approval expired")]
unconstrained fn approvals_expire_as_blocks_advance() {
    let vault = TestVault::found(3, 2, 2);
    let witness = approved_until(vault, vault.now() + 60);

    vault.env.advance_next_block_timestamp_by(120);
    vault.env.mine_block();

    let _ = vault.authorize(OUTER_HASH, witness);
}

#[test]
unconstrained fn the_transaction_expires_at_a_deadline_sooner_than_the_config_window() {
    let vault = TestVault::found(3, 2, 2);
    let expires_at = vault.now() + 600;

    assert_eq(
        vault.authorized_expiration(OUTER_HASH, approved_until(vault, expires_at)),
        expires_at,
    );
}

#[test]
unconstrained fn the_config_window_caps_a_later_deadline() {
    let vault = TestVault::found(3, 2, 2);
    let witness = approved_until(vault, vault.now() + ONE_DAY);

    assert_eq(vault.authorized_expiration(OUTER_HASH, witness), vault.now() + CONFIG_MAX_AGE);
}

#[test]
unconstrained fn a_deadline_exactly_at_the_config_window_is_kept() {
    let vault = TestVault::found(3, 2, 2);
    let expires_at = vault.now() + CONFIG_MAX_AGE;

    assert_eq(
        vault.authorized_expiration(OUTER_HASH, approved_until(vault, expires_at)),
        expires_at,
    );
}

#[test]
unconstrained fn an_approval_at_the_deadline_expires_the_transaction_immediately() {
    let vault = TestVault::found(3, 2, 2);

    assert_eq(
        vault.authorized_expiration(OUTER_HASH, approved_until(vault, vault.now())),
        vault.now(),
    );
}
