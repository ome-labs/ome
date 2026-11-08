//! `count_approvals`: the fixed-shape loop over all eight signature slots.
//!
//! Valid signatures need the TXE and are covered by the integration tests; these cases need no signer.

use crate::{
    authorization::count_approvals,
    constants::{MAX_SIGNERS, SIGNATURE_LEN},
    witness::Signature,
};
use crate::test::fixtures::policy::{FOUNDING_KEYS, signer_set};

global MESSAGE: Field = 0xabc;

fn no_signatures() -> [Signature; MAX_SIGNERS] {
    [Signature::from_limbs([0; SIGNATURE_LEN]); MAX_SIGNERS]
}

fn garbage_signature() -> Signature {
    Signature::from_limbs([1, 2, 3, 4])
}

#[test]
fn counts_zero_without_signatures() {
    assert_eq(count_approvals(signer_set(3, FOUNDING_KEYS), no_signatures(), MESSAGE), 0);
}

#[test]
fn counts_zero_for_a_full_vault_without_signatures() {
    assert_eq(
        count_approvals(
            signer_set(MAX_SIGNERS, FOUNDING_KEYS),
            no_signatures(),
            MESSAGE,
        ),
        0,
    );
}

#[test]
fn verifies_empty_slots_against_the_generator_without_failing() {
    // Unused slots hold the zero key, which is not a curve point; the loop must still run every slot.
    assert_eq(count_approvals(signer_set(1, FOUNDING_KEYS), no_signatures(), MESSAGE), 0);
}

#[test(should_fail_with = "invalid signature")]
fn rejects_a_garbage_signature_in_an_occupied_slot() {
    let mut signatures = no_signatures();
    signatures[0] = garbage_signature();

    let _ = count_approvals(signer_set(3, FOUNDING_KEYS), signatures, MESSAGE);
}

#[test(should_fail_with = "invalid signature")]
fn rejects_a_garbage_signature_in_the_last_occupied_slot() {
    let mut signatures = no_signatures();
    signatures[MAX_SIGNERS - 1] = garbage_signature();

    let _ = count_approvals(signer_set(MAX_SIGNERS, FOUNDING_KEYS), signatures, MESSAGE);
}

#[test(should_fail_with = "signature for empty signer slot")]
fn rejects_a_signature_right_after_the_last_signer() {
    let mut signatures = no_signatures();
    signatures[3] = garbage_signature();

    let _ = count_approvals(signer_set(3, FOUNDING_KEYS), signatures, MESSAGE);
}

#[test(should_fail_with = "signature for empty signer slot")]
fn rejects_a_signature_in_the_last_slot_of_a_small_vault() {
    let mut signatures = no_signatures();
    signatures[MAX_SIGNERS - 1] = garbage_signature();

    let _ = count_approvals(signer_set(2, FOUNDING_KEYS), signatures, MESSAGE);
}

#[test(should_fail_with = "signature for empty signer slot")]
fn rejects_a_single_non_zero_limb_in_an_empty_slot() {
    let mut signatures = no_signatures();
    signatures[5] = Signature::from_limbs([0, 0, 0, 1]);

    let _ = count_approvals(signer_set(2, FOUNDING_KEYS), signatures, MESSAGE);
}
