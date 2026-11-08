//! An approval authorizes one transaction, for one vault, under one config version and deadline, from one member.

use crate::{authorization::approval_message, constants::{MAX_SIGNERS, SIGNATURE_LEN}};
use crate::test::fixtures::{
    approvals::{encode_witness, first},
    members::new_member,
    vault::{ONE_DAY, OUTER_HASH, TestVault},
};
use aztec::protocol::{address::AztecAddress, traits::FromField};

#[test(should_fail_with = "invalid signature")]
unconstrained fn rejects_approvals_of_another_transaction() {
    let vault = TestVault::found(3, 2, 2);
    let expires_at = vault.now() + ONE_DAY;
    let witness =
        vault.witness(vault.approval_message(OUTER_HASH + 1, 0, expires_at), first(2), expires_at);

    let _ = vault.authorize(OUTER_HASH, witness);
}

#[test(should_fail_with = "invalid signature")]
unconstrained fn rejects_approvals_given_to_another_vault_with_the_same_members() {
    let vault = TestVault::found(3, 2, 2);
    let expires_at = vault.now() + ONE_DAY;
    let elsewhere = approval_message(AztecAddress::from_field(0x0dd), OUTER_HASH, 0, expires_at);

    let _ = vault.authorize(OUTER_HASH, vault.witness(elsewhere, first(2), expires_at));
}

#[test(should_fail_with = "invalid signature")]
unconstrained fn rejects_approvals_for_another_config_version() {
    let vault = TestVault::found(3, 2, 2);

    let _ = vault.authorize(OUTER_HASH, vault.approve(1, first(2)));
}

#[test(should_fail_with = "invalid signature")]
unconstrained fn rejects_a_deadline_other_than_the_signed_one() {
    let vault = TestVault::found(3, 2, 2);
    let signed = vault.now() + ONE_DAY;
    let witness =
        vault.witness(vault.approval_message(OUTER_HASH, 0, signed), first(2), signed + ONE_DAY);

    let _ = vault.authorize(OUTER_HASH, witness);
}

#[test(should_fail_with = "invalid signature")]
unconstrained fn rejects_one_members_signature_copied_into_another_slot() {
    let vault = TestVault::found(3, 2, 2);
    let expires_at = vault.now() + ONE_DAY;
    let signature =
        vault.sign(vault.members.accounts[0], vault.approval_message(OUTER_HASH, 0, expires_at));

    let mut signatures = [[0; SIGNATURE_LEN]; MAX_SIGNERS];
    signatures[0] = signature;
    signatures[1] = signature;

    let _ = vault.authorize(OUTER_HASH, encode_witness(vault.members.keys, signatures, expires_at));
}

#[test(should_fail_with = "signer set mismatch")]
unconstrained fn rejects_an_outsider_listed_in_place_of_a_member() {
    let mut vault = TestVault::found(3, 2, 2);
    let outsider = new_member(&mut vault.env);
    let expires_at = vault.now() + ONE_DAY;
    let message = vault.approval_message(OUTER_HASH, 0, expires_at);

    let mut keys = vault.members.keys;
    keys[2] = outsider.key;
    let mut signatures = [[0; SIGNATURE_LEN]; MAX_SIGNERS];
    signatures[0] = vault.sign(vault.members.accounts[0], message);
    signatures[2] = vault.sign(outsider.account, message);

    let _ = vault.authorize(OUTER_HASH, encode_witness(keys, signatures, expires_at));
}

#[test(should_fail_with = "signer set mismatch")]
unconstrained fn rejects_members_listed_in_another_order() {
    let vault = TestVault::found(3, 2, 2);
    let expires_at = vault.now() + ONE_DAY;
    let reordered = vault.members.swapped(0, 1);
    let signatures = vault.signatures(vault.approval_message(OUTER_HASH, 0, expires_at), first(2));

    let _ = vault.authorize(OUTER_HASH, encode_witness(reordered.keys, signatures, expires_at));
}

#[test(should_fail_with = "signature for empty signer slot")]
unconstrained fn rejects_a_signature_in_a_slot_with_no_member() {
    let vault = TestVault::found(2, 2, 2);
    let expires_at = vault.now() + ONE_DAY;

    let mut signatures =
        vault.signatures(vault.approval_message(OUTER_HASH, 0, expires_at), first(2));
    signatures[2] = [1, 0, 0, 0];

    let _ = vault.authorize(OUTER_HASH, encode_witness(vault.members.keys, signatures, expires_at));
}

#[test(should_fail_with = "invalid signature")]
unconstrained fn one_bad_signature_fails_the_check_even_when_k_others_are_valid() {
    let vault = TestVault::found(3, 2, 2);
    let expires_at = vault.now() + ONE_DAY;

    let mut signatures =
        vault.signatures(vault.approval_message(OUTER_HASH, 0, expires_at), first(2));
    signatures[2] = [1, 2, 3, 4];

    let _ = vault.authorize(OUTER_HASH, encode_witness(vault.members.keys, signatures, expires_at));
}
