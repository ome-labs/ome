use crate::{
    constants::{DOM_SEP__COMMITTEE, MAX_SIGNERS, MIN_VIEWING_THRESHOLD, POINT_LEN},
    point::{assert_ordered_points, Point},
};
use aztec::protocol::hash::poseidon2_hash_with_separator;
use std::embedded_curve_ops::EmbeddedCurvePoint;

/// Viewing committee: the threshold `t_v` and each member's public verification share `Y_i = s_i · G` from the
/// distributed key generation. Member `i` of the committee is signer `i` of the vault.
///
/// Only the commitment is stored on-chain; keepers check every threshold ECDH request and reshare against it.
pub struct Committee {
    pub viewing_threshold: u32,
    pub shares: [Point; MAX_SIGNERS],
    pub member_count: u32,
}

impl Committee {
    /// Builds a committee for `member_count` members and asserts it is well formed.
    pub fn new(viewing_threshold: u32, shares: [Point; MAX_SIGNERS], member_count: u32) -> Self {
        let committee = Self { viewing_threshold, shares, member_count };
        committee.assert_valid();
        committee
    }

    /// Asserts `MIN_VIEWING_THRESHOLD <= t_v <= member_count` and that shares are ordered, distinct, on-curve and
    /// zero-padded.
    pub fn assert_valid(self) {
        assert(
            (self.viewing_threshold >= MIN_VIEWING_THRESHOLD)
                & (self.viewing_threshold <= self.member_count),
            "viewing threshold out of range",
        );
        assert_ordered_points(self.shares, self.member_count, "committee");
    }

    /// Asserts the committee holds `key`: its shares lie on the polynomial whose Feldman commitments are
    /// `polynomial` (`C_k = a_k · G`, zero-padded), of degree exactly `t_v - 1` and with `C_0 = key`. Then any `t_v`
    /// members can read for the vault and fewer learn nothing, and no rotation can install a committee that cannot.
    ///
    /// Share `Y_j` of member `j` (1-based) must equal `Σ_k j^k · C_k`, evaluated by Horner. The member index is a
    /// compile-time constant in the unrolled loop, so each step is a few native Grumpkin doublings and additions,
    /// never a general scalar multiplication.
    pub fn assert_holds_key(self, polynomial: [Point; MAX_SIGNERS], key: Point) {
        assert(polynomial[0] == key, "committee does not hold the vault's key");
        for k in 0..MAX_SIGNERS {
            let coefficient = polynomial[k];
            if k < self.viewing_threshold {
                assert(
                    !coefficient.is_empty() & coefficient.is_on_curve(),
                    "committee polynomial coefficient missing or off curve",
                );
            } else {
                assert(coefficient.is_empty(), "committee polynomial degree above t_v - 1");
            }
        }
        for i in 0..MAX_SIGNERS {
            if i < self.member_count {
                let mut value = EmbeddedCurvePoint::point_at_infinity();
                for k in 0..MAX_SIGNERS {
                    value = times(value, i + 1) + polynomial[MAX_SIGNERS - 1 - k].to_embedded();
                }
                assert(value == self.shares[i].to_embedded(), "share off the committee polynomial");
            }
        }
    }

    /// `Poseidon2("ome.committee.v0", t_v, Y_0.x, Y_0.y, ..., Y_{MAX-1}.y)`.
    pub fn commitment(self) -> Field {
        let mut fields = [0; MAX_SIGNERS * POINT_LEN + 1];
        fields[0] = self.viewing_threshold as Field;
        for i in 0..MAX_SIGNERS {
            fields[1 + i * POINT_LEN] = self.shares[i].x;
            fields[1 + i * POINT_LEN + 1] = self.shares[i].y;
        }
        poseidon2_hash_with_separator(fields, DOM_SEP__COMMITTEE)
    }
}

/// `factor · point` for a small factor known at compile time: double-and-add over its bits.
fn times(point: EmbeddedCurvePoint, factor: u32) -> EmbeddedCurvePoint {
    let mut result = EmbeddedCurvePoint::point_at_infinity();
    let mut base = point;
    for bit in 0..4 {
        if (factor >> bit) & 1 == 1 {
            result = result + base;
        }
        if (factor >> (bit + 1)) != 0 {
            base = base.double();
        }
    }
    result
}
