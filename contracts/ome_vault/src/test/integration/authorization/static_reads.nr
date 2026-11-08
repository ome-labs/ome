//! Authorization only reads state: it must stay valid inside static calls such as `verify_private_authwit`, and the
//! same config must keep authorizing.

use crate::{authorization::is_authorized, OmeVault};
use crate::test::fixtures::{approvals::first, vault::{OUTER_HASH, TestVault}};
use std::test::OracleMock;

#[test]
unconstrained fn authorization_emits_no_nullifiers_notes_or_logs() {
    let vault = TestVault::found(3, 2, 2);
    let witness = vault.approve(0, first(2));
    let _ = OracleMock::mock("aztec_utl_getAuthWitness").returns(witness).times(1);

    let (nullifiers, note_hashes, logs) = vault.env.private_context_at(vault.address, |context| {
        assert(is_authorized(context, OmeVault::storage_layout().config.slot, OUTER_HASH));

        let inputs = context.finish();
        (inputs.nullifiers.length, inputs.note_hashes.length, inputs.private_logs.length)
    });

    assert_eq(nullifiers, 0);
    assert_eq(note_hashes, 0);
    assert_eq(logs, 0);
}

#[test]
unconstrained fn the_same_config_authorizes_repeatedly() {
    let vault = TestVault::found(3, 2, 2);

    for _ in 0..3 {
        assert(vault.authorize(OUTER_HASH, vault.approve(0, first(2))));
    }
}

#[test]
unconstrained fn authorizing_leaves_the_stored_policy_unchanged() {
    let vault = TestVault::found(3, 2, 2);
    let before = vault.config();

    assert(vault.authorize(OUTER_HASH, vault.approve(0, first(2))));

    assert_eq(vault.config(), before);
}
