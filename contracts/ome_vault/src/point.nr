use aztec::protocol::traits::{Deserialize, Serialize};
use std::embedded_curve_ops::EmbeddedCurvePoint;

/// Affine Grumpkin point used for signer keys and viewing verification shares. `(0, 0)` marks an empty slot.
#[derive(Deserialize, Eq, Serialize)]
pub struct Point {
    pub x: Field,
    pub y: Field,
}

impl Point {
    pub fn empty() -> Self {
        Self { x: 0, y: 0 }
    }

    pub fn is_empty(self) -> bool {
        (self.x == 0) & (self.y == 0)
    }

    /// Whether the point lies on Grumpkin (`y^2 = x^3 - 17`).
    pub fn is_on_curve(self) -> bool {
        self.y * self.y == self.x * self.x * self.x - 17
    }

    pub fn to_embedded(self) -> EmbeddedCurvePoint {
        EmbeddedCurvePoint { x: self.x, y: self.y }
    }
}

/// Asserts that `points[0..count)` are distinct on-curve points and every later slot is empty.
///
/// `label` prefixes the failure messages (e.g. `"signer"` gives `"duplicate signer"`).
pub fn assert_ordered_points<let N: u32, let L: u32>(
    points: [Point; N],
    count: u32,
    label: str<L>,
) {
    for i in 0..N {
        let point = points[i];
        if i < count {
            assert(!point.is_empty(), f"empty {label} key");
            assert(point.is_on_curve(), f"{label} key not on curve");
            for j in 0..i {
                assert(points[j] != point, f"duplicate {label}");
            }
        } else {
            assert(point.is_empty(), f"non-empty {label} slot past count");
        }
    }
}
