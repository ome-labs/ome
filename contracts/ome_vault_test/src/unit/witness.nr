//! `ApprovalWitness::from_fields`: the 49-field auth witness layout shared with the TypeScript encoder.

use ome_vault::{
    constants::{MAX_SIGNERS, POINT_LEN, SIGNATURE_LEN, WITNESS_LEN},
    witness::{ApprovalWitness, Signature},
};

global SIGNATURES_OFFSET: u32 = MAX_SIGNERS * POINT_LEN;

/// Field `i` holds `i + 1`, so every decoded value names the position it came from.
fn numbered_fields() -> [Field; WITNESS_LEN] {
    let mut fields = [0; WITNESS_LEN];
    for i in 0..WITNESS_LEN {
        fields[i] = (i + 1) as Field;
    }
    fields
}

#[test]
fn reads_one_key_per_slot_x_then_y() {
    let witness = ApprovalWitness::from_fields(numbered_fields(), 3);

    for slot in 0..MAX_SIGNERS {
        assert_eq(witness.signers.keys[slot].x, (slot * POINT_LEN + 1) as Field);
        assert_eq(witness.signers.keys[slot].y, (slot * POINT_LEN + 2) as Field);
    }
}

#[test]
fn reads_one_signature_per_slot_as_s_lo_s_hi_e_lo_e_hi() {
    let witness = ApprovalWitness::from_fields(numbered_fields(), 3);

    for slot in 0..MAX_SIGNERS {
        let base = SIGNATURES_OFFSET + slot * SIGNATURE_LEN;
        let signature = witness.signatures[slot];

        assert_eq(signature.s.lo, (base + 1) as Field);
        assert_eq(signature.s.hi, (base + 2) as Field);
        assert_eq(signature.e.lo, (base + 3) as Field);
        assert_eq(signature.e.hi, (base + 4) as Field);
    }
}

#[test]
fn reads_the_deadline_from_the_last_field() {
    let witness = ApprovalWitness::from_fields(numbered_fields(), 3);

    assert_eq(witness.expires_at, WITNESS_LEN as u64);
}

#[test]
fn takes_the_signer_count_from_the_caller() {
    for count in 1..MAX_SIGNERS + 1 {
        assert_eq(ApprovalWitness::from_fields(numbered_fields(), count).signers.count, count);
    }
}

#[test]
fn does_not_validate_keys() {
    // Decoding only lays fields out; the keys are checked against the stored commitment afterwards.
    let witness = ApprovalWitness::from_fields([0; WITNESS_LEN], 3);

    assert(witness.signers.keys[0].is_empty());
}

#[test]
fn accepts_the_largest_u64_deadline() {
    let mut fields = [0; WITNESS_LEN];
    fields[WITNESS_LEN - 1] = 0xffffffffffffffff;

    assert_eq(ApprovalWitness::from_fields(fields, 1).expires_at, 0xffffffffffffffff);
}

#[test(should_fail_with = "expires_at out of range")]
fn rejects_a_deadline_of_two_to_the_64() {
    let mut fields = [0; WITNESS_LEN];
    fields[WITNESS_LEN - 1] = 0x10000000000000000;

    let _ = ApprovalWitness::from_fields(fields, 1);
}

#[test(should_fail_with = "expires_at out of range")]
fn rejects_a_deadline_that_wraps_around_the_field() {
    let mut fields = [0; WITNESS_LEN];
    fields[WITNESS_LEN - 1] = -1;

    let _ = ApprovalWitness::from_fields(fields, 1);
}

#[test]
fn all_zero_signature_is_empty() {
    assert(Signature::from_limbs([0; SIGNATURE_LEN]).is_empty());
}

#[test]
fn any_non_zero_limb_makes_a_signature_present() {
    for limb in 0..SIGNATURE_LEN {
        let mut limbs = [0; SIGNATURE_LEN];
        limbs[limb] = 1;

        assert(!Signature::from_limbs(limbs).is_empty());
    }
}

#[test]
fn from_limbs_maps_limbs_in_order() {
    let signature = Signature::from_limbs([1, 2, 3, 4]);

    assert_eq(signature.s.lo, 1);
    assert_eq(signature.s.hi, 2);
    assert_eq(signature.e.lo, 3);
    assert_eq(signature.e.hi, 4);
}
