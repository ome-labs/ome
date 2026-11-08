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

/// A non-zero point that is not on Grumpkin (`1 != 1 - 17`).
pub fn off_curve_point() -> Point {
    Point { x: 1, y: 1 }
}
