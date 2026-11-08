use crate::{
    constants::{MAX_SIGNERS, POINT_LEN, SIGNATURE_LEN, WITNESS_LEN},
    point::Point,
    signer_set::SignerSet,
};
use std::embedded_curve_ops::EmbeddedCurveScalar;

/// Schnorr signature over Grumpkin, as produced by `Schnorr.constructSignature(...).toLimbFields()`.
pub struct Signature {
    pub s: EmbeddedCurveScalar,
    pub e: EmbeddedCurveScalar,
}

impl Signature {
    pub fn from_limbs(limbs: [Field; SIGNATURE_LEN]) -> Self {
        Self {
            s: EmbeddedCurveScalar::new(limbs[0], limbs[1]),
            e: EmbeddedCurveScalar::new(limbs[2], limbs[3]),
        }
    }

    /// An all-zero signature marks a signer that did not approve.
    pub fn is_empty(self) -> bool {
        (self.s.lo == 0) & (self.s.hi == 0) & (self.e.lo == 0) & (self.e.hi == 0)
    }
}

/// Aggregated auth witness for one message (`WITNESS_LEN` fields):
///
/// - `[0, MAX_SIGNERS * 2)`: signer keys in slot order, `x` then `y`, empty slots zero.
/// - `[MAX_SIGNERS * 2, MAX_SIGNERS * 6)`: one 4-limb signature per slot, zero where the signer did not approve.
/// - `MAX_SIGNERS * 6`: `expires_at`, the approval deadline (unix seconds) every signature commits to.
pub struct ApprovalWitness {
    pub signers: SignerSet,
    pub signatures: [Signature; MAX_SIGNERS],
    pub expires_at: u64,
}

impl ApprovalWitness {
    /// Decodes the witness, taking the signer count from the config it is checked against.
    pub fn from_fields(fields: [Field; WITNESS_LEN], signer_count: u32) -> Self {
        let signatures_offset = MAX_SIGNERS * POINT_LEN;

        let mut keys = [Point::empty(); MAX_SIGNERS];
        let mut signatures = [Signature::from_limbs([0; SIGNATURE_LEN]); MAX_SIGNERS];
        for i in 0..MAX_SIGNERS {
            keys[i] = Point { x: fields[i * POINT_LEN], y: fields[i * POINT_LEN + 1] };
            let base = signatures_offset + i * SIGNATURE_LEN;
            signatures[i] = Signature::from_limbs([
                fields[base],
                fields[base + 1],
                fields[base + 2],
                fields[base + 3],
            ]);
        }

        let expires_at_field = fields[WITNESS_LEN - 1];
        let expires_at = expires_at_field as u64;
        assert(expires_at as Field == expires_at_field, "expires_at out of range");

        Self { signers: SignerSet { keys, count: signer_count }, signatures, expires_at }
    }
}
