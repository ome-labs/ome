//! Byte forms of field elements, scalars, points and signatures, with validated conversions to `ome-crypto` types.

use ome_crypto::{Base, Point, Scalar, curve, schnorr};
use serde::{Deserialize, Serialize};

use crate::Error;

/// Canonical big-endian base field element.
pub type WireField = [u8; 32];

/// Canonical big-endian scalar.
pub type WireScalar = [u8; 32];

/// Affine point as two canonical coordinates; validated on conversion.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WirePoint {
    /// `x`, big-endian.
    pub x: WireField,
    /// `y`, big-endian.
    pub y: WireField,
}

impl From<&Point> for WirePoint {
    fn from(point: &Point) -> Self {
        Self {
            x: field(&point.x()),
            y: field(&point.y()),
        }
    }
}

impl TryFrom<&WirePoint> for Point {
    type Error = Error;

    fn try_from(value: &WirePoint) -> Result<Self, Error> {
        Ok(Point::from_xy(
            parse_field(&value.x)?,
            parse_field(&value.y)?,
        )?)
    }
}

/// A Grumpkin Schnorr signature as the vault contract reads it: `[s.lo, s.hi, e.lo, e.hi]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireSignature(pub [WireField; 4]);

impl From<&schnorr::Signature> for WireSignature {
    fn from(signature: &schnorr::Signature) -> Self {
        Self(signature.to_limbs().map(|limb| field(&limb)))
    }
}

impl TryFrom<&WireSignature> for schnorr::Signature {
    type Error = Error;

    fn try_from(value: &WireSignature) -> Result<Self, Error> {
        let limbs = [
            parse_field(&value.0[0])?,
            parse_field(&value.0[1])?,
            parse_field(&value.0[2])?,
            parse_field(&value.0[3])?,
        ];
        Ok(schnorr::Signature::from_limbs(&limbs)?)
    }
}

/// A 64-byte value (Ed25519 signature, transcript hash) split in two halves, since serde arrays stop at 32.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bytes64(pub [u8; 32], pub [u8; 32]);

impl Bytes64 {
    /// From a 64-byte array.
    #[must_use]
    pub fn from_array(bytes: &[u8; 64]) -> Self {
        let mut first = [0u8; 32];
        let mut second = [0u8; 32];
        first.copy_from_slice(&bytes[..32]);
        second.copy_from_slice(&bytes[32..]);
        Self(first, second)
    }

    /// To a 64-byte array.
    #[must_use]
    pub fn to_array(&self) -> [u8; 64] {
        let mut out = [0u8; 64];
        out[..32].copy_from_slice(&self.0);
        out[32..].copy_from_slice(&self.1);
        out
    }
}

/// Encodes a base field element.
#[must_use]
pub fn field(value: &Base) -> WireField {
    curve::base_to_bytes(value)
}

/// Decodes a canonical base field element.
///
/// # Errors
/// [`Error::Crypto`] for a non-canonical encoding.
pub fn parse_field(value: &WireField) -> Result<Base, Error> {
    Ok(curve::base_from_bytes(value)?)
}

/// Encodes a scalar.
#[must_use]
pub fn scalar(value: &Scalar) -> WireScalar {
    curve::scalar_to_bytes(value)
}

/// Decodes a canonical scalar.
///
/// # Errors
/// [`Error::Crypto`] for a non-canonical encoding.
pub fn parse_scalar(value: &WireScalar) -> Result<Scalar, Error> {
    Ok(curve::scalar_from_bytes(value)?)
}

/// Decodes a list of points.
///
/// # Errors
/// [`Error::Crypto`] for any invalid point.
pub fn parse_points(points: &[WirePoint]) -> Result<Vec<Point>, Error> {
    points.iter().map(Point::try_from).collect()
}
