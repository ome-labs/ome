//! Deterministic Grumpkin points for tests. Signer keys from the TXE come from `members`; these stand in for committee
//! shares and for keys that never sign.

use crate::{constants::MAX_SIGNERS, point::Point};
use std::embedded_curve_ops::{EmbeddedCurveScalar, fixed_base_scalar_mul};

/// Seed of the committee shares a fixture vault is founded with.
pub global FOUNDING_SHARES: Field = 1000;

/// Seed of the committee shares a fixture rotation installs.
pub global ROTATED_SHARES: Field = 2000;

/// `scalar · G`: on-curve and distinct for distinct non-zero scalars.
pub fn point_from_scalar(scalar: Field) -> Point {
    let point = fixed_base_scalar_mul(EmbeddedCurveScalar::from_field(scalar));
    Point { x: point.x, y: point.y }
}

/// `count` distinct on-curve points `(seed + i) · G`, zero-padded to `MAX_SIGNERS`.
pub fn distinct_points(count: u32, seed: Field) -> [Point; MAX_SIGNERS] {
    let mut points = [Point::empty(); MAX_SIGNERS];
    for i in 0..MAX_SIGNERS {
        if i < count {
            points[i] = point_from_scalar(seed + i as Field);
        }
    }
    points
}

/// A committee for tests: its members' verification shares and the Feldman commitments of their polynomial.
pub struct TestCommittee {
    pub shares: [Point; MAX_SIGNERS],
    pub polynomial: [Point; MAX_SIGNERS],
}

/// The committee of `count` members whose polynomial is `key + Σ_{k=1}^{degree} (seed + k)·G · X^k`: it holds `key`
/// when `degree = t_v - 1`. Coefficients are known scalars, so share `j` is `key + (Σ_k (seed + k) j^k) · G`.
pub fn committee_of_degree(key: Point, count: u32, degree: u32, seed: Field) -> TestCommittee {
    let mut polynomial = [Point::empty(); MAX_SIGNERS];
    polynomial[0] = key;
    for k in 1..MAX_SIGNERS {
        if k <= degree {
            polynomial[k] = point_from_scalar(seed + k as Field);
        }
    }
    let mut shares = [Point::empty(); MAX_SIGNERS];
    for i in 0..MAX_SIGNERS {
        if i < count {
            let j = (i + 1) as Field;
            let mut scalar: Field = 0;
            let mut power: Field = 1;
            for k in 1..MAX_SIGNERS {
                power *= j;
                if k <= degree {
                    scalar += (seed + k as Field) * power;
                }
            }
            let share = key.to_embedded() + point_from_scalar(scalar).to_embedded();
            shares[i] = Point { x: share.x, y: share.y };
        }
    }
    TestCommittee { shares, polynomial }
}

/// The committee of `count` members that holds `key` with viewing threshold `viewing_threshold`.
pub fn committee_holding(
    key: Point,
    count: u32,
    viewing_threshold: u32,
    seed: Field,
) -> TestCommittee {
    committee_of_degree(key, count, viewing_threshold - 1, seed)
}

/// A non-zero point that is not on Grumpkin (`1 != 1 - 17`).
pub fn off_curve_point() -> Point {
    Point { x: 1, y: 1 }
}
