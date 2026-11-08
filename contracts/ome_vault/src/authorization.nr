//! k-of-n Schnorr authorization shared by `entrypoint`, `verify_private_authwit` and `lookup_validity`.

use crate::{
    constants::{CONFIG_MAX_AGE, DOM_SEP__APPROVAL, MAX_SIGNERS, WITNESS_LEN},
    notes::config_note::ConfigNote,
    signer_set::SignerSet,
    witness::{ApprovalWitness, Signature},
};
use aztec::{
    context::PrivateContext,
    history::note::assert_local_note_was_not_nullified_by,
    note::note_getter::get_note,
    oracle::auth_witness::get_auth_witness,
    protocol::{address::AztecAddress, hash::poseidon2_hash_with_separator, traits::ToField},
};
use std::embedded_curve_ops::EmbeddedCurvePoint;

/// The message each signer signs to approve `outer_hash` for `vault` under config `config_version` until
/// `expires_at`.
///
/// Binding the vault address stops replay on another vault with the same signers (the authwit hash binds only the
/// consumer); binding the version invalidates approvals collected before a config change.
pub fn approval_message(
    vault: AztecAddress,
    outer_hash: Field,
    config_version: u32,
    expires_at: u64,
) -> Field {
    poseidon2_hash_with_separator(
        [vault.to_field(), outer_hash, config_version as Field, expires_at as Field],
        DOM_SEP__APPROVAL,
    )
}

/// Counts valid approvals of `message`. Empty signature slots are skipped; any non-empty signature must belong to an
/// occupied slot and be valid.
pub fn count_approvals(
    signers: SignerSet,
    signatures: [Signature; MAX_SIGNERS],
    message: Field,
) -> u32 {
    let mut approvals: u32 = 0;
    for i in 0..MAX_SIGNERS {
        let signature = signatures[i];
        let has_signature = !signature.is_empty();
        if has_signature {
            assert(i < signers.count, "signature for empty signer slot");
        }

        // Every slot is verified so the circuit has a fixed shape. Empty slots hold the zero key, which is not a curve
        // point, so they are verified against the generator instead.
        let key = signers.keys[i];
        let point = if key.is_empty() {
            EmbeddedCurvePoint::generator()
        } else {
            key.to_embedded()
        };
        let valid = schnorr::verify_signature(point, (signature.s, signature.e), message);
        assert(!has_signature | valid, "invalid signature");

        approvals += has_signature as u32;
    }
    approvals
}

/// Whether `witness` carries at least `config.threshold` valid approvals of `outer_hash` for `vault`, given the
/// current time `now`.
pub fn check_witness(
    config: ConfigNote,
    witness: ApprovalWitness,
    vault: AztecAddress,
    outer_hash: Field,
    now: u64,
) -> bool {
    assert(witness.signers.commitment() == config.signer_set_commitment, "signer set mismatch");
    assert(now <= witness.expires_at, "approval expired");
    let message = approval_message(vault, outer_hash, config.version, witness.expires_at);
    count_approvals(witness.signers, witness.signatures, message) >= config.threshold
}

/// Constrained authorization against the config note stored at `config_slot`.
///
/// Reads the config without nullifying it, so it is valid inside static calls, and proves the note was current at the
/// anchor block. The transaction must be included before the approval deadline and within `CONFIG_MAX_AGE` of the
/// anchor block, which bounds how long a superseded config can still authorize.
pub fn is_authorized(context: &mut PrivateContext, config_slot: Field, outer_hash: Field) -> bool {
    let anchor = context.get_anchor_block_header();
    let config = get_note::<ConfigNote>(context, Option::none(), config_slot);
    assert_local_note_was_not_nullified_by(anchor, config, context);

    // Safety: the witness is only a hint. Its signer keys are checked against the stored commitment, its deadline is
    // range checked and bound into every signature, and every counted signature is verified, so a malicious oracle can
    // only make authorization fail.
    let fields: [Field; WITNESS_LEN] = unsafe { get_auth_witness(outer_hash) };
    let witness = ApprovalWitness::from_fields(fields, config.note.signer_count);

    let now = anchor.timestamp();
    let authorized = check_witness(
        config.note,
        witness,
        context.this_address(),
        outer_hash,
        now,
    );

    let config_deadline = now + CONFIG_MAX_AGE;
    let expiration = if witness.expires_at < config_deadline {
        witness.expires_at
    } else {
        config_deadline
    };
    context.set_expiration_timestamp(expiration);

    authorized
}
