//! The vault's per-epoch tagging secret for its own unconstrained notes (change, config and signer notes it sends to
//! itself). Tags derived from the vault's address keys never change, so a member removed at a rotation could keep
//! spotting the vault's self-sends. Tagging them with a secret that changes every committee epoch, and that only
//! that epoch's members can derive, stops it.

use ome_crypto::{Base, Point, SecretScalar, curve, poseidon2};

use crate::{
    Error,
    vault::{half, tag},
};

/// Domain tag of the epoch tagging secret.
pub const EPOCH_TAGGING_TAG: &str = "ome.epoch-tagging.v0";

/// The tagging secret point for `vault`'s self-sends in committee epoch `epoch`: `s · G` with
/// `s = Poseidon2("ome.epoch-tagging.v0", [vault, epoch, t_0, t_1, t_2, t_3])`, where `t_i` are the 16-byte big-endian
/// limbs of the epoch's ceremony transcript (the founding DKG for epoch 0, then each reshare). Only the epoch's
/// committee takes part in its ceremony, so a member removed earlier never sees the transcript. Dealers of a reshare
/// must therefore be members who stay.
///
/// # Errors
/// [`Error::Crypto`] in the negligible case that `s` is zero.
pub fn epoch_tagging_secret(
    vault: Base,
    epoch: u32,
    transcript: &[u8; 64],
) -> Result<Point, Error> {
    let s = poseidon2::hash_with_separator(
        tag(EPOCH_TAGGING_TAG),
        &[
            vault,
            Base::from(epoch),
            half(&transcript[..16]),
            half(&transcript[16..32]),
            half(&transcript[32..48]),
            half(&transcript[48..]),
        ],
    );
    Ok(Point::generator_mul_secret(&SecretScalar::new(
        curve::base_to_scalar(&s),
    ))?)
}
