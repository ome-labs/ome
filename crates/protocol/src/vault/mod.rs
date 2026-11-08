//! A mirror of the `OmeVault` contract. Every value here must equal what the contract computes; the Noir tests and
//! `tests/contract_vectors.rs` pin the same vectors.

mod approval;
mod committee;
mod config;
pub mod constants;
mod signer_set;
mod witness;

pub use approval::{approval_message, rejection_message};
pub use committee::Committee;
pub use config::Config;
pub use signer_set::SignerSet;
pub use witness::Witness;

use ome_crypto::{Base, Point, curve};

use crate::Error;
use constants::{MAX_SIGNERS, POINT_LEN};

/// A domain tag as the contract writes it: the field whose big-endian bytes spell the ASCII name.
#[must_use]
pub fn tag(name: &str) -> Base {
    let mut bytes = [0u8; 32];
    bytes[32 - name.len()..].copy_from_slice(name.as_bytes());
    // Tags are short ASCII names, so the value is far below the field modulus.
    curve::base_from_bytes(&bytes).unwrap_or_else(|_| unreachable!("tags are at most 31 bytes"))
}

/// A 16-byte big-endian value as a field (128 bits always fit).
pub(crate) fn half(bytes: &[u8]) -> Base {
    let mut padded = [0u8; 32];
    padded[32 - bytes.len()..].copy_from_slice(bytes);
    curve::base_from_bytes(&padded).unwrap_or_else(|_| unreachable!("a 128-bit value is canonical"))
}

/// `[x_0, y_0, ..., x_7, y_7]` with empty slots zero, as the contract lays out ordered points.
pub(crate) fn padded(points: &[Point]) -> [Base; MAX_SIGNERS * POINT_LEN] {
    let mut fields = [Base::from(0u64); MAX_SIGNERS * POINT_LEN];
    for (i, point) in points.iter().enumerate() {
        fields[i * POINT_LEN] = point.x();
        fields[i * POINT_LEN + 1] = point.y();
    }
    fields
}

/// `1..=MAX_SIGNERS` distinct points, as `assert_ordered_points` requires.
fn check_ordered(points: &[Point]) -> Result<(), Error> {
    if points.is_empty() || points.len() > MAX_SIGNERS {
        return Err(Error::MemberCount(points.len()));
    }
    for (i, point) in points.iter().enumerate() {
        if points[..i].contains(point) {
            return Err(Error::DuplicateKey(i));
        }
    }
    Ok(())
}

/// `n` as the contract's `u32`; checked against `MAX_SIGNERS` first, so it always fits.
fn count(points: &[Point]) -> u32 {
    u32::try_from(points.len()).unwrap_or(u32::MAX)
}
