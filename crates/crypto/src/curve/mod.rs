//! Grumpkin points and scalars with Aztec's encodings.
//!
//! Grumpkin's base field is BN254's scalar field ([`Base`], Aztec's `Fr`); its scalar field is BN254's base field
//! ([`Scalar`], Aztec's `Fq`). Points are affine and never the identity once validated.

use ark_ec::{AffineRepr, CurveGroup, PrimeGroup};
use ark_ff::{BigInteger, PrimeField};
use ark_grumpkin::{Affine, Projective};

use crate::{Error, SecretScalar};

mod ladder;

/// Grumpkin scalar field element (Aztec `Fq`, `GrumpkinScalar`).
pub type Scalar = ark_grumpkin::Fr;

/// Grumpkin base field element (Aztec `Fr`); point coordinates and Poseidon2 inputs.
pub type Base = ark_grumpkin::Fq;

/// Encoded length of a field element (big-endian).
pub const FIELD_BYTES: usize = 32;

/// A validated, non-identity Grumpkin point.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Point(Affine);

impl Point {
    /// Aztec's generator `(1, sqrt(-16))`.
    #[must_use]
    pub fn generator() -> Self {
        Self(Projective::generator().into_affine())
    }

    /// Builds a point from affine coordinates, rejecting off-curve points and the identity.
    ///
    /// # Errors
    /// [`Error::PointNotOnCurve`] if `(x, y)` is not on Grumpkin.
    pub fn from_xy(x: Base, y: Base) -> Result<Self, Error> {
        let point = Affine::new_unchecked(x, y);
        if !point.is_on_curve() {
            return Err(Error::PointNotOnCurve);
        }
        // Grumpkin has prime order, so every on-curve point is in the group.
        Ok(Self(point))
    }

    pub(crate) fn from_affine(point: Affine) -> Result<Self, Error> {
        if point.is_zero() {
            return Err(Error::IdentityPoint);
        }
        Ok(Self(point))
    }

    /// Affine `x` coordinate.
    #[must_use]
    pub fn x(&self) -> Base {
        self.0.x
    }

    /// Affine `y` coordinate.
    #[must_use]
    pub fn y(&self) -> Base {
        self.0.y
    }

    /// Whether `y <= (p - 1) / 2`, Aztec's "positive" y (`Point.toXAndSign`).
    #[must_use]
    pub fn has_positive_y(&self) -> bool {
        let half = <Base as PrimeField>::MODULUS_MINUS_ONE_DIV_TWO;
        self.0.y.into_bigint() <= half
    }

    /// Aztec `Point.toBuffer`: `x || y`, 32 bytes each, big-endian.
    #[must_use]
    pub fn to_bytes(&self) -> [u8; 2 * FIELD_BYTES] {
        let mut out = [0u8; 2 * FIELD_BYTES];
        out[..FIELD_BYTES].copy_from_slice(&base_to_bytes(&self.0.x));
        out[FIELD_BYTES..].copy_from_slice(&base_to_bytes(&self.0.y));
        out
    }

    /// Inverse of [`Point::to_bytes`].
    ///
    /// # Errors
    /// [`Error::NonCanonicalEncoding`] for coordinates not below the modulus, [`Error::PointNotOnCurve`] otherwise
    /// invalid.
    pub fn from_bytes(bytes: &[u8; 2 * FIELD_BYTES]) -> Result<Self, Error> {
        let (x, y) = bytes.split_at(FIELD_BYTES);
        Self::from_xy(base_from_bytes(x)?, base_from_bytes(y)?)
    }

    /// Aztec `Point.toCompressedBuffer`: `x` big-endian with the top bit set when `y` is positive.
    #[must_use]
    pub fn to_compressed(&self) -> [u8; FIELD_BYTES] {
        let mut out = base_to_bytes(&self.0.x);
        if self.has_positive_y() {
            out[0] |= 0x80;
        }
        out
    }

    /// Public-scalar multiplication (variable time). Never use with secret scalars.
    pub fn mul_public(&self, scalar: &Scalar) -> Projective {
        self.0 * scalar
    }

    /// Secret-scalar multiplication with a constant-time ladder.
    ///
    /// # Errors
    /// [`Error::IdentityPoint`] if the result is the identity (only possible for a zero scalar).
    pub fn mul_secret(&self, scalar: &SecretScalar) -> Result<Self, Error> {
        Self::from_affine(ladder::mul(&self.0, scalar.expose()))
    }

    /// `scalar · G` for a secret scalar, constant time.
    ///
    /// # Errors
    /// [`Error::IdentityPoint`] if `scalar` is zero.
    pub fn generator_mul_secret(scalar: &SecretScalar) -> Result<Self, Error> {
        Self::generator().mul_secret(scalar)
    }

    /// The underlying arkworks affine point.
    #[must_use]
    pub fn as_affine(&self) -> &Affine {
        &self.0
    }
}

impl TryFrom<Projective> for Point {
    type Error = Error;

    fn try_from(point: Projective) -> Result<Self, Error> {
        Self::from_affine(point.into_affine())
    }
}

/// Big-endian 32-byte encoding of a base field element.
#[must_use]
pub fn base_to_bytes(value: &Base) -> [u8; FIELD_BYTES] {
    let mut out = [0u8; FIELD_BYTES];
    out.copy_from_slice(&value.into_bigint().to_bytes_be());
    out
}

/// Canonical big-endian decoding of a base field element.
///
/// # Errors
/// [`Error::InvalidLength`] for a wrong length, [`Error::NonCanonicalEncoding`] if the value is not below the
/// modulus.
pub fn base_from_bytes(bytes: &[u8]) -> Result<Base, Error> {
    if bytes.len() != FIELD_BYTES {
        return Err(Error::InvalidLength {
            expected: FIELD_BYTES,
            actual: bytes.len(),
        });
    }
    let value = Base::from_be_bytes_mod_order(bytes);
    if base_to_bytes(&value).as_slice() != bytes {
        return Err(Error::NonCanonicalEncoding);
    }
    Ok(value)
}

/// Big-endian 32-byte encoding of a scalar.
#[must_use]
pub fn scalar_to_bytes(value: &Scalar) -> [u8; FIELD_BYTES] {
    let mut out = [0u8; FIELD_BYTES];
    out.copy_from_slice(&value.into_bigint().to_bytes_be());
    out
}

/// Canonical big-endian decoding of a scalar.
///
/// # Errors
/// [`Error::InvalidLength`] for a wrong length, [`Error::NonCanonicalEncoding`] if the value is not below the
/// modulus.
pub fn scalar_from_bytes(bytes: &[u8]) -> Result<Scalar, Error> {
    if bytes.len() != FIELD_BYTES {
        return Err(Error::InvalidLength {
            expected: FIELD_BYTES,
            actual: bytes.len(),
        });
    }
    let value = Scalar::from_be_bytes_mod_order(bytes);
    if scalar_to_bytes(&value).as_slice() != bytes {
        return Err(Error::NonCanonicalEncoding);
    }
    Ok(value)
}

/// Reinterprets a base field element as a scalar (Aztec does this for the preaddress; `Fr < Fq`, so no reduction).
#[must_use]
pub fn base_to_scalar(value: &Base) -> Scalar {
    Scalar::from_be_bytes_mod_order(&base_to_bytes(value))
}
