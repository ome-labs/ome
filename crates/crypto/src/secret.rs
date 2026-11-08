use rand_core::CryptoRng;
use subtle::ConstantTimeEq;
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::Scalar;

/// A secret Grumpkin scalar (share, polynomial coefficient, nonce, signing key).
///
/// Zeroized on drop, compared in constant time, never printed (`Debug` is redacted), and only multiplied through the
/// constant-time ladder. It is deliberately not `Clone`: a copy is a second place the secret lives, so code that needs
/// one builds it explicitly from [`SecretScalar::expose`].
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct SecretScalar(Scalar);

impl SecretScalar {
    /// Wraps a scalar that must be kept secret.
    #[must_use]
    pub fn new(value: Scalar) -> Self {
        Self(value)
    }

    /// A uniform scalar, from 64 random bytes reduced modulo the group order (negligible bias).
    pub fn random(rng: &mut impl CryptoRng) -> Self {
        let mut wide = [0u8; 64];
        rng.fill_bytes(&mut wide);
        let value = <Scalar as ark_ff::PrimeField>::from_le_bytes_mod_order(&wide);
        wide.zeroize();
        Self(value)
    }

    /// Access to the raw value for arithmetic. Keep results secret too.
    #[must_use]
    pub fn expose(&self) -> &Scalar {
        &self.0
    }
}

impl ConstantTimeEq for SecretScalar {
    fn ct_eq(&self, other: &Self) -> subtle::Choice {
        let a = crate::curve::scalar_to_bytes(&self.0);
        let b = crate::curve::scalar_to_bytes(&other.0);
        a.ct_eq(&b)
    }
}

impl core::fmt::Debug for SecretScalar {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("SecretScalar(<redacted>)")
    }
}
