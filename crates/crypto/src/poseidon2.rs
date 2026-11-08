//! Aztec's Poseidon2 hash over BN254: the `t = 4` permutation (`taceo-poseidon2`) inside Barretenberg's sponge.

use ark_ff::{Field, Zero};

use crate::Base;

const RATE: usize = 3;

/// Barretenberg sponge: rate 3, capacity element initialised to `len · 2^64`, absorb in chunks of three, output
/// `state[0]`. Matches `poseidon2Hash` in `@aztec-labs/foundation`.
#[must_use]
pub fn hash(inputs: &[Base]) -> Base {
    let iv = Base::from(inputs.len() as u64) * Base::from(2u64).pow([64]);
    let mut state = [Base::zero(), Base::zero(), Base::zero(), iv];
    let mut chunks = inputs.chunks(RATE).peekable();
    if chunks.peek().is_none() {
        taceo_poseidon2::bn254::t4::permutation_in_place(&mut state);
        return state[0];
    }
    for chunk in chunks {
        for (lane, value) in state.iter_mut().zip(chunk) {
            *lane += value;
        }
        taceo_poseidon2::bn254::t4::permutation_in_place(&mut state);
    }
    state[0]
}

/// `poseidon2HashWithSeparator`: the separator is prepended as a field element.
#[must_use]
pub fn hash_with_separator(separator: Base, inputs: &[Base]) -> Base {
    let mut all = Vec::with_capacity(inputs.len() + 1);
    all.push(separator);
    all.extend_from_slice(inputs);
    hash(&all)
}

/// The raw `t = 4` permutation.
#[must_use]
pub fn permutation(state: [Base; 4]) -> [Base; 4] {
    taceo_poseidon2::bn254::t4::permutation(&state)
}
