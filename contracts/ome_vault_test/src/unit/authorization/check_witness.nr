//! `check_witness`: the witness must name the stored signer set, be within its deadline, and reach the threshold.

use crate::fixtures::{
    approvals::encode_witness,
    points::point_from_scalar,
    policy::{FOUNDING_KEYS, signer_set, vault_config},
};
use aztec::protocol::address::AztecAddress;
use ome_vault::{
    authorization::check_witness,
    constants::{MAX_SIGNERS, SIGNATURE_LEN},
    witness::ApprovalWitness,
};

global VAULT: AztecAddress = AztecAddress { inner: 0x1234 };
global OUTER_HASH: Field = 0x7ec9a2;
global DEADLINE: u64 = 1_700_000_000;

/// A witness for the founding 3-member signer set with no approvals.
fn unsigned_witness(keys_seed: Field, expires_at: u64) -> ApprovalWitness {
    let keys = signer_set(3, keys_seed).keys;
    let fields = encode_witness(keys, [[0; SIGNATURE_LEN]; MAX_SIGNERS], expires_at);
    ApprovalWitness::from_fields(fields, 3)
}

#[test]
fn falls_short_of_the_threshold_without_approvals() {
    let witness = unsigned_witness(FOUNDING_KEYS, DEADLINE);

    assert(!check_witness(vault_config(3, 1, 2), witness, VAULT, OUTER_HASH, DEADLINE));
}

#[test]
fn accepts_the_deadline_itself() {
    let witness = unsigned_witness(FOUNDING_KEYS, DEADLINE);

    // No assertion fires at `now == expires_at`; the result is only short of approvals.
    assert(!check_witness(vault_config(3, 2, 2), witness, VAULT, OUTER_HASH, DEADLINE));
}

#[test(should_fail_with = "approval expired")]
fn rejects_one_second_past_the_deadline() {
    let witness = unsigned_witness(FOUNDING_KEYS, DEADLINE);

    let _ = check_witness(
        vault_config(3, 2, 2),
        witness,
        VAULT,
        OUTER_HASH,
        DEADLINE + 1,
    );
}

#[test(should_fail_with = "signer set mismatch")]
fn rejects_a_witness_for_another_signer_set() {
    let witness = unsigned_witness(FOUNDING_KEYS + 100, DEADLINE);

    let _ = check_witness(vault_config(3, 2, 2), witness, VAULT, OUTER_HASH, DEADLINE);
}

#[test(should_fail_with = "signer set mismatch")]
fn rejects_the_right_keys_in_the_wrong_order() {
    let mut keys = signer_set(3, FOUNDING_KEYS).keys;
    let first = keys[0];
    keys[0] = keys[1];
    keys[1] = first;
    let fields = encode_witness(keys, [[0; SIGNATURE_LEN]; MAX_SIGNERS], DEADLINE);

    let _ = check_witness(
        vault_config(3, 2, 2),
        ApprovalWitness::from_fields(fields, 3),
        VAULT,
        OUTER_HASH,
        DEADLINE,
    );
}

#[test(should_fail_with = "signer set mismatch")]
fn rejects_an_extra_key_past_the_signer_count() {
    let mut keys = signer_set(3, FOUNDING_KEYS).keys;
    keys[3] = point_from_scalar(99);
    let fields = encode_witness(keys, [[0; SIGNATURE_LEN]; MAX_SIGNERS], DEADLINE);

    let _ = check_witness(
        vault_config(3, 2, 2),
        ApprovalWitness::from_fields(fields, 3),
        VAULT,
        OUTER_HASH,
        DEADLINE,
    );
}

#[test(should_fail_with = "signer set mismatch")]
fn checks_the_signer_set_before_the_deadline() {
    let witness = unsigned_witness(FOUNDING_KEYS + 100, DEADLINE);

    let _ = check_witness(
        vault_config(3, 2, 2),
        witness,
        VAULT,
        OUTER_HASH,
        DEADLINE + 1,
    );
}
