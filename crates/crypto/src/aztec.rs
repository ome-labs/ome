//! Aztec 6.0.0 key derivation needed to operate an account whose incoming viewing secret is threshold-shared.
//!
//! An Aztec address is the `x` of `preaddress · G + Ivpk`. The account's address secret is `±(ivsk + preaddress)`,
//! negated when that point has a "negative" `y`; the sign depends only on public data, so every share holder applies
//! the same sign without learning `ivsk`.

use ark_ff::One;

use crate::{Base, Error, Point, Scalar, SecretScalar, curve::base_to_scalar, poseidon2};

/// Aztec `DomainSeparator.CONTRACT_ADDRESS_V3`.
pub const DOM_SEP_CONTRACT_ADDRESS_V3: u64 = 993_442_748;

/// Aztec `DomainSeparator.APP_SILOED_ECDH_SHARED_SECRET`.
pub const DOM_SEP_APP_SILOED_ECDH_SHARED_SECRET: u64 = 1_707_851_664;

/// `preaddress = Poseidon2([public_keys_hash, partial_address], CONTRACT_ADDRESS_V3)`.
#[must_use]
pub fn preaddress(public_keys_hash: Base, partial_address: Base) -> Base {
    poseidon2::hash_with_separator(
        Base::from(DOM_SEP_CONTRACT_ADDRESS_V3),
        &[public_keys_hash, partial_address],
    )
}

/// Public data that fixes how a viewing share maps to a share of the address secret.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AddressContext {
    /// The account's preaddress, as a scalar.
    pub preaddress: Scalar,
    /// `+1` or `-1`: Aztec's address-secret sign.
    pub sign: Scalar,
    /// `preaddress · G + Ivpk`, normalised to positive `y`; its `x` is the address.
    pub address_point: Point,
}

impl AddressContext {
    /// Derives the context from the preaddress and the group incoming viewing public key.
    ///
    /// # Errors
    /// [`Error::IdentityPoint`] if `preaddress · G + Ivpk` is the identity.
    pub fn new(preaddress: Base, ivpk: &Point) -> Result<Self, Error> {
        let h = base_to_scalar(&preaddress);
        let candidate = Point::try_from(Point::generator().mul_public(&h) + ivpk.as_affine())?;
        let positive = candidate.has_positive_y();
        let sign = if positive {
            Scalar::one()
        } else {
            -Scalar::one()
        };
        let address_point = if positive {
            candidate
        } else {
            Point::from_xy(candidate.x(), -candidate.y())?
        };
        Ok(Self {
            preaddress: h,
            sign,
            address_point,
        })
    }

    /// The Aztec address (`x` of the address point).
    #[must_use]
    pub fn address(&self) -> Base {
        self.address_point.x()
    }

    /// Maps a viewing share `s_i` to its share of the address secret: `sign · (s_i + preaddress)`.
    ///
    /// Lagrange coefficients over any qualified set sum to one, so these shares combine to the address secret.
    #[must_use]
    pub fn address_secret_share(&self, viewing_share: &SecretScalar) -> SecretScalar {
        SecretScalar::new(self.sign * (*viewing_share.expose() + self.preaddress))
    }

    /// Public counterpart of [`AddressContext::address_secret_share`]: `sign · (Y_i + preaddress · G)`.
    ///
    /// # Errors
    /// [`Error::IdentityPoint`] in the negligible case that the result is the identity.
    pub fn address_verification_share(&self, verification_share: &Point) -> Result<Point, Error> {
        let shifted =
            Point::generator().mul_public(&self.preaddress) + verification_share.as_affine();
        Point::try_from(shifted * self.sign)
    }
}

/// Single-key address secret (Aztec `computeAddressSecret`). Used for test vectors and dealt-share tests; production
/// code never holds `ivsk`.
///
/// # Errors
/// [`Error::IdentityPoint`] if `ivsk · G` or the address point is the identity.
pub fn single_key_address_secret(
    preaddress: Base,
    ivsk: &SecretScalar,
) -> Result<SecretScalar, Error> {
    let incoming_viewing_key = Point::generator_mul_secret(ivsk)?;
    let context = AddressContext::new(preaddress, &incoming_viewing_key)?;
    Ok(context.address_secret_share(ivsk))
}

/// App-siloed shared secret `Poseidon2([S.x, S.y, app], APP_SILOED_ECDH_SHARED_SECRET)` (Aztec
/// `appSiloEcdhSharedSecretPoint`).
#[must_use]
pub fn app_siloed_shared_secret(shared_point: &Point, app: Base) -> Base {
    poseidon2::hash_with_separator(
        Base::from(DOM_SEP_APP_SILOED_ECDH_SHARED_SECRET),
        &[shared_point.x(), shared_point.y(), app],
    )
}
