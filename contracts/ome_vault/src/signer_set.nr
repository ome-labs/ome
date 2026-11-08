use crate::{
    constants::{DOM_SEP__SIGNER_SET, MAX_SIGNERS, POINT_LEN},
    point::{assert_ordered_points, Point},
};
use aztec::protocol::{hash::poseidon2_hash_with_separator, traits::{Deserialize, Serialize}};

/// Ordered signer keys. Slots `[0, count)` hold distinct on-curve keys; the rest are empty.
///
/// The order is part of the commitment: approvals are matched to signers by slot, and member `i` is both signer `i`
/// and viewing-committee member `i`.
#[derive(Deserialize, Eq, Serialize)]
pub struct SignerSet {
    pub keys: [Point; MAX_SIGNERS],
    pub count: u32,
}

impl SignerSet {
    /// Builds a signer set and asserts it is well formed.
    pub fn new(keys: [Point; MAX_SIGNERS], count: u32) -> Self {
        let set = Self { keys, count };
        set.assert_valid();
        set
    }

    /// Asserts `1 <= count <= MAX_SIGNERS` and that the keys are ordered, distinct, on-curve and zero-padded.
    pub fn assert_valid(self) {
        assert((self.count >= 1) & (self.count <= MAX_SIGNERS), "signer count out of range");
        assert_ordered_points(self.keys, self.count, "signer");
    }

    /// `Poseidon2("ome.signers.v0", x_0, y_0, ..., x_{MAX-1}, y_{MAX-1})`.
    pub fn commitment(self) -> Field {
        let mut fields = [0; MAX_SIGNERS * POINT_LEN];
        for i in 0..MAX_SIGNERS {
            fields[i * POINT_LEN] = self.keys[i].x;
            fields[i * POINT_LEN + 1] = self.keys[i].y;
        }
        poseidon2_hash_with_separator(fields, DOM_SEP__SIGNER_SET)
    }
}
