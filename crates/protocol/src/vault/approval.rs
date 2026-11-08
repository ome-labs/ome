use ome_crypto::{Base, curve, poseidon2};

use super::{
    constants::{APPROVAL_TAG, REJECTION_TAG},
    tag,
};

/// What each signer signs to approve `outer_hash` (`authorization::approval_message`):
/// `Poseidon2("ome.approval.v0", [vault, outer_hash, config_version, expires_at])`.
///
/// The vault address stops replay on another vault with the same signers; the version voids approvals collected
/// before a config change; the deadline bounds how long an approval lives.
#[must_use]
pub fn approval_message(
    vault: Base,
    outer_hash: Base,
    config_version: u32,
    expires_at: u64,
) -> Base {
    poseidon2::hash_with_separator(
        tag(APPROVAL_TAG),
        &[
            vault,
            outer_hash,
            Base::from(config_version),
            Base::from(expires_at),
        ],
    )
}

/// What a member signs to reject a proposal: `Poseidon2("ome.rejection.v0", [vault, id_hi, id_lo])`, with the 32-byte
/// proposal id split into big-endian 128-bit halves (it does not fit one field).
#[must_use]
pub fn rejection_message(vault: Base, proposal_id: &[u8; 32]) -> Base {
    poseidon2::hash_with_separator(
        tag(REJECTION_TAG),
        &[vault, half(&proposal_id[..16]), half(&proposal_id[16..])],
    )
}

fn half(bytes: &[u8]) -> Base {
    let mut padded = [0u8; 32];
    padded[16..].copy_from_slice(bytes);
    // 128 bits are always below the field modulus.
    curve::base_from_bytes(&padded).unwrap_or_else(|_| unreachable!("a 128-bit value is canonical"))
}
