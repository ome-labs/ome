//! Authwits: another contract acting for the vault (`verify_private_authwit`) needs k approvals of a message bound to
//! that consumer, the chain and the action.

use crate::OmeVault;
use crate::test::fixtures::{
    account::authwit_hash,
    approvals::first,
    members::new_member,
    vault::{ONE_DAY, TestVault},
};
use aztec::protocol::address::AztecAddress;

global INNER_HASH: Field = 0xac710;

/// A witness in which `approvers` let `consumer` act on `inner_hash` for the vault.
unconstrained fn approve_action(
    vault: TestVault,
    consumer: AztecAddress,
    inner_hash: Field,
    approvers: u32,
) -> [Field; 49] {
    let expires_at = vault.now() + ONE_DAY;
    let message = vault.approval_message(authwit_hash(vault, consumer, inner_hash), 0, expires_at);
    vault.witness(message, first(approvers), expires_at)
}

#[test]
unconstrained fn authorizes_an_action_approved_for_its_consumer() {
    let mut vault = TestVault::found(3, 2, 2);
    let consumer = new_member(&mut vault.env).account;
    let message_hash = authwit_hash(vault, consumer, INNER_HASH);

    assert(vault.authorize(message_hash, approve_action(vault, consumer, INNER_HASH, 2)));
}

#[test]
unconstrained fn does_not_authorize_an_action_with_too_few_approvals() {
    let mut vault = TestVault::found(3, 2, 2);
    let consumer = new_member(&mut vault.env).account;
    let message_hash = authwit_hash(vault, consumer, INNER_HASH);

    assert(!vault.authorize(message_hash, approve_action(vault, consumer, INNER_HASH, 1)));
}

#[test(should_fail_with = "invalid signature")]
unconstrained fn an_approval_for_one_consumer_does_not_authorize_another() {
    let mut vault = TestVault::found(3, 2, 2);
    let approved = new_member(&mut vault.env).account;
    let other = new_member(&mut vault.env).account;

    let _ = vault.authorize(
        authwit_hash(vault, other, INNER_HASH),
        approve_action(vault, approved, INNER_HASH, 2),
    );
}

#[test(should_fail_with = "invalid signature")]
unconstrained fn an_approval_for_one_action_does_not_authorize_another() {
    let mut vault = TestVault::found(3, 2, 2);
    let consumer = new_member(&mut vault.env).account;

    let _ = vault.authorize(
        authwit_hash(vault, consumer, INNER_HASH + 1),
        approve_action(vault, consumer, INNER_HASH, 2),
    );
}

#[test(should_fail)]
unconstrained fn a_real_call_without_any_approval_reverts() {
    let mut vault = TestVault::found(3, 2, 2);
    let consumer = new_member(&mut vault.env).account;

    let _ = vault.env.view_private(
        consumer,
        OmeVault::at(vault.address).verify_private_authwit(INNER_HASH),
    );
}
