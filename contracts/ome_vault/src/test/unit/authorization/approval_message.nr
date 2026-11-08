//! `approval_message`: what every member signs, and what it binds.

use crate::authorization::approval_message;
use aztec::protocol::{address::AztecAddress, traits::FromField};

global VAULT: AztecAddress = AztecAddress { inner: 0x1234 };

#[test]
fn is_deterministic() {
    assert_eq(approval_message(VAULT, 2, 3, 4), approval_message(VAULT, 2, 3, 4));
}

#[test]
fn binds_the_vault_so_approvals_do_not_replay_on_a_vault_with_the_same_signers() {
    assert(
        approval_message(VAULT, 2, 3, 4)
            != approval_message(AztecAddress::from_field(0x9999), 2, 3, 4),
    );
}

#[test]
fn binds_the_outer_hash() {
    assert(approval_message(VAULT, 2, 3, 4) != approval_message(VAULT, 9, 3, 4));
}

#[test]
fn binds_the_config_version_so_a_policy_change_voids_approvals() {
    assert(approval_message(VAULT, 2, 3, 4) != approval_message(VAULT, 2, 4, 4));
}

#[test]
fn binds_the_deadline() {
    assert(approval_message(VAULT, 2, 3, 4) != approval_message(VAULT, 2, 3, 5));
}

#[test]
fn keeps_inputs_in_their_positions() {
    // Swapping two values must not produce the same message.
    assert(approval_message(VAULT, 3, 2, 4) != approval_message(VAULT, 2, 3, 4));
    assert(approval_message(VAULT, 2, 4, 3) != approval_message(VAULT, 2, 3, 4));
}

#[test]
fn accepts_extreme_versions_and_deadlines() {
    let largest = approval_message(VAULT, 2, 0xffffffff, 0xffffffffffffffff);

    assert(largest != approval_message(VAULT, 2, 0, 0));
}
