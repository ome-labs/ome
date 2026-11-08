use ome_crypto::{Base, schnorr};

use super::{
    SignerSet,
    constants::{MAX_SIGNERS, POINT_LEN, SIGNATURE_LEN, WITNESS_LEN},
    padded,
};

/// The 49-field auth witness `is_authorized` decodes (`witness.nr`):
///
/// - `[0, 16)`: signer keys in slot order, `x` then `y`, empty slots zero;
/// - `[16, 48)`: one 4-limb signature per slot, zero where the signer did not approve;
/// - `48`: `expires_at`, the deadline every signature commits to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Witness(pub [Base; WITNESS_LEN]);

impl Witness {
    /// Encodes the witness. `signatures[slot]` is `None` for a signer that did not approve; slots past the signer
    /// count are ignored.
    #[must_use]
    pub fn encode(
        signers: &SignerSet,
        signatures: &[Option<schnorr::Signature>],
        expires_at: u64,
    ) -> Self {
        let mut fields = [Base::from(0u64); WITNESS_LEN];
        fields[..MAX_SIGNERS * POINT_LEN].copy_from_slice(&padded(signers.keys()));
        let offset = MAX_SIGNERS * POINT_LEN;
        for (slot, signature) in signatures.iter().take(signers.keys().len()).enumerate() {
            if let Some(signature) = signature {
                let start = offset + slot * SIGNATURE_LEN;
                fields[start..start + SIGNATURE_LEN].copy_from_slice(&signature.to_limbs());
            }
        }
        fields[WITNESS_LEN - 1] = Base::from(expires_at);
        Self(fields)
    }
}
