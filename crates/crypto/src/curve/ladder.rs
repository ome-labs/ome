//! Constant-time scalar multiplication for secret scalars.
//!
//! arkworks' `Affine * Scalar` branches on scalar bits, so secret shares, coefficients and nonces go through this
//! ladder instead:
//!
//! - homogeneous projective coordinates with the complete addition formula for `a = 0` curves (Renes, Costello,
//!   Batina 2016, Algorithm 7), which handles doubling and the identity without branches;
//! - a fixed 4-bit window over all 256 scalar bits, selecting table entries with a constant-time linear scan;
//! - conversion to affine by Fermat inversion (fixed public exponent).
//!
//! Field multiplication itself is arkworks' Montgomery arithmetic; its conditional final subtraction is the remaining
//! known timing variation and is documented in `architecture/crypto.md`.

use ark_ff::{BigInt, BigInteger, Field, One, PrimeField, Zero};
use ark_grumpkin::Affine;
use subtle::{Choice, ConditionallySelectable, ConstantTimeEq};

use crate::{Base, Scalar};

const WINDOW_BITS: usize = 4;
const TABLE_SIZE: usize = 1 << WINDOW_BITS;
const SCALAR_BITS: usize = 256;

/// `3 · b` for Grumpkin `y^2 = x^3 - 17`.
fn b3() -> Base {
    -Base::from(51u64)
}

/// Homogeneous projective point `(X : Y : Z)`; the identity is `(0 : 1 : 0)`.
#[derive(Clone, Copy)]
struct HPoint {
    x: Base,
    y: Base,
    z: Base,
}

impl HPoint {
    fn identity() -> Self {
        Self {
            x: Base::zero(),
            y: Base::one(),
            z: Base::zero(),
        }
    }

    fn from_affine(point: &Affine) -> Self {
        // Callers pass validated non-identity points.
        Self {
            x: point.x,
            y: point.y,
            z: Base::one(),
        }
    }

    /// Complete addition, `a = 0` (RCB16 Algorithm 7). Valid for every input pair, including doubling.
    fn add(&self, other: &Self) -> Self {
        let b3 = b3();
        let (x1, y1, z1) = (self.x, self.y, self.z);
        let (x2, y2, z2) = (other.x, other.y, other.z);

        let mut t0 = x1 * x2;
        let mut t1 = y1 * y2;
        let mut t2 = z1 * z2;
        let mut t3 = x1 + y1;
        let mut t4 = x2 + y2;
        t3 *= t4;
        t4 = t0 + t1;
        t3 -= t4;
        t4 = y1 + z1;
        let mut x3 = y2 + z2;
        t4 *= x3;
        x3 = t1 + t2;
        t4 -= x3;
        x3 = x1 + z1;
        let mut y3 = x2 + z2;
        x3 *= y3;
        y3 = t0 + t2;
        y3 = x3 - y3;
        x3 = t0 + t0;
        t0 += x3;
        t2 *= b3;
        let mut z3 = t1 + t2;
        t1 -= t2;
        y3 *= b3;
        x3 = t4 * y3;
        t2 = t3 * t1;
        x3 = t2 - x3;
        y3 *= t0;
        t1 *= z3;
        y3 = t1 + y3;
        t0 *= t3;
        z3 *= t4;
        z3 += t0;

        Self {
            x: x3,
            y: y3,
            z: z3,
        }
    }

    fn conditional_select(a: &Self, b: &Self, choice: Choice) -> Self {
        Self {
            x: select_base(&a.x, &b.x, choice),
            y: select_base(&a.y, &b.y, choice),
            z: select_base(&a.z, &b.z, choice),
        }
    }

    /// Affine conversion with a fixed-exponent inversion. Returns the identity for `Z = 0`.
    fn to_affine(self) -> Affine {
        if bool::from(self.z.is_zero_ct()) {
            return Affine::identity();
        }
        let mut exponent = <Base as PrimeField>::MODULUS;
        exponent.sub_with_borrow(&BigInt::from(2u64));
        let z_inv = self.z.pow(exponent);
        Affine::new_unchecked(self.x * z_inv, self.y * z_inv)
    }
}

trait IsZeroCt {
    fn is_zero_ct(&self) -> Choice;
}

impl IsZeroCt for Base {
    fn is_zero_ct(&self) -> Choice {
        let limbs = self.0.0;
        let mut acc = 0u64;
        for limb in limbs {
            acc |= limb;
        }
        acc.ct_eq(&0)
    }
}

/// Selects `b` when `choice` is set, limb by limb on the Montgomery representation.
fn select_base(a: &Base, b: &Base, choice: Choice) -> Base {
    let (a, b) = (a.0.0, b.0.0);
    let mut out = [0u64; 4];
    for i in 0..4 {
        out[i] = u64::conditional_select(&a[i], &b[i], choice);
    }
    Base::new_unchecked(BigInt::new(out))
}

/// `scalar · point` in constant time with respect to `scalar`.
pub(crate) fn mul(point: &Affine, scalar: &Scalar) -> Affine {
    let base = HPoint::from_affine(point);

    // table[i] = i · point
    let mut table = [HPoint::identity(); TABLE_SIZE];
    for i in 1..TABLE_SIZE {
        table[i] = table[i - 1].add(&base);
    }

    let bits = scalar.into_bigint().to_bits_le();
    let mut acc = HPoint::identity();
    for window in (0..SCALAR_BITS / WINDOW_BITS).rev() {
        for _ in 0..WINDOW_BITS {
            acc = acc.add(&acc);
        }
        let mut digit = 0u8;
        for bit in 0..WINDOW_BITS {
            let index = window * WINDOW_BITS + bit;
            let set = bits.get(index).copied().unwrap_or(false);
            digit |= u8::from(set) << bit;
        }
        let mut selected = HPoint::identity();
        for (i, entry) in (0u8..).zip(table.iter()) {
            let hit = i.ct_eq(&digit);
            selected = HPoint::conditional_select(&selected, entry, hit);
        }
        acc = acc.add(&selected);
    }
    acc.to_affine()
}
