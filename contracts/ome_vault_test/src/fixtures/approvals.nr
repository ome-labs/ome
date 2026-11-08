//! Which members approve, and how an approval set is laid out as the contract's auth witness.

use ome_vault::{constants::{MAX_SIGNERS, POINT_LEN, SIGNATURE_LEN, WITNESS_LEN}, point::Point};

/// Signature limbs of every slot; all-zero marks a member who did not approve.
pub type Signatures = [[Field; SIGNATURE_LEN]; MAX_SIGNERS];

/// No member approves.
pub fn nobody() -> [bool; MAX_SIGNERS] {
    [false; MAX_SIGNERS]
}

/// Members in slots `0..count` approve.
pub fn first(count: u32) -> [bool; MAX_SIGNERS] {
    let mut approving = nobody();
    for i in 0..MAX_SIGNERS {
        approving[i] = i < count;
    }
    approving
}

/// Exactly the members in `slots` approve.
pub fn slots<let N: u32>(slots: [u32; N]) -> [bool; MAX_SIGNERS] {
    let mut approving = nobody();
    for i in 0..N {
        approving[slots[i]] = true;
    }
    approving
}

/// Encodes signer keys, per-slot signatures and the deadline in the contract's witness layout: keys, then signatures,
/// then `expires_at`.
pub fn encode_witness(
    keys: [Point; MAX_SIGNERS],
    signatures: Signatures,
    expires_at: u64,
) -> [Field; WITNESS_LEN] {
    let signatures_offset = MAX_SIGNERS * POINT_LEN;
    let mut fields = [0; WITNESS_LEN];

    for i in 0..MAX_SIGNERS {
        fields[i * POINT_LEN] = keys[i].x;
        fields[i * POINT_LEN + 1] = keys[i].y;

        for j in 0..SIGNATURE_LEN {
            fields[signatures_offset + i * SIGNATURE_LEN + j] = signatures[i][j];
        }
    }

    fields[WITNESS_LEN - 1] = expires_at as Field;
    fields
}
