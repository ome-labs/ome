//! Signer swaps: `rotate_membership` with one slot's key replaced and the committee, both thresholds and the member
//! count unchanged. A swap moves a member's signing key to another device. The committee stays, so no reshare runs and
//! every keeper keeps its share; keepers only adopt the new key (`swapped_manifest`).

use ome_crypto::Point;
use ome_wire::{
    messages::ceremony::Manifest,
    types::{WirePoint, parse_field},
};

use super::{Rotation, Vault};
use crate::{
    Error,
    payload::Call,
    vault::{Config, SignerSet, constants::ROTATE_MEMBERSHIP_SELECTOR},
};

impl Rotation {
    /// The rotation that replaces signer slot `slot`'s key with `key`, keeping the committee and both thresholds.
    ///
    /// # Errors
    /// [`Error::UnknownSlot`], [`Error::DuplicateKey`] if `key` is already a signer.
    pub fn signer_swap(vault: &Vault, slot: u32, key: Point) -> Result<Self, Error> {
        let mut keys = vault.signers().keys().to_vec();
        let target = usize::try_from(slot)
            .ok()
            .and_then(|slot| keys.get_mut(slot))
            .ok_or(Error::UnknownSlot(slot))?;
        *target = key;
        Ok(Self {
            signers: SignerSet::new(keys)?,
            threshold: vault.config().threshold,
            committee: vault.committee().clone(),
        })
    }
}

/// The slot a rotation of `vault` swaps, if it is a signer swap: the committee, both thresholds and the member count
/// unchanged, and exactly one key replaced by a key that is not a signer yet.
///
/// # Errors
/// [`Error::NotASignerSwap`] naming what else the rotation changes.
pub fn signer_swap_slot(vault: &Vault, rotation: &Rotation) -> Result<u32, Error> {
    if &rotation.committee != vault.committee() {
        return Err(Error::NotASignerSwap("it changes the viewing committee"));
    }
    if rotation.threshold != vault.config().threshold {
        return Err(Error::NotASignerSwap("it changes the spending threshold"));
    }
    swapped_slot(vault.signers().keys(), rotation.signers.keys())
}

/// Everything an approver checks before signing a signer swap, returning the swapped slot and the config the swap
/// produces: it is a signer swap ([`signer_swap_slot`]) and the contract would accept it.
///
/// # Errors
/// [`Error::NotASignerSwap`] or config transition errors.
pub fn check_signer_swap(vault: &Vault, rotation: &Rotation) -> Result<(u32, Config), Error> {
    let slot = signer_swap_slot(vault, rotation)?;
    let next = vault.config().rotate(
        vault.signers(),
        &rotation.signers,
        rotation.threshold,
        &rotation.committee,
    )?;
    Ok((slot, next))
}

/// The manifest after a signer swap that landed: `calls` are an executed proposal's calls, `config` the vault's config
/// note read after it ran. One call must be a signer swap of `manifest`'s vault (its current keys `manifest`'s, its
/// committee and thresholds unchanged), and the manifest with that key replaced must pass [`Vault::from_manifest`]
/// against `config`, so a keeper adopts a new key only once the chain holds it.
///
/// # Errors
/// [`Error::NotASignerSwap`] if no call is a signer swap of this manifest, or the [`Vault::from_manifest`] error.
pub fn swapped_manifest(
    manifest: &Manifest,
    config: &Config,
    calls: &[Call],
) -> Result<Manifest, Error> {
    let vault = parse_field(&manifest.vault)?;
    let current = manifest.signer_keys()?;
    let shares = manifest.verification_shares()?;
    let swap = calls
        .iter()
        .filter(|call| {
            call.selector == ROTATE_MEMBERSHIP_SELECTOR && parse_field(&call.to).ok() == Some(vault)
        })
        .find_map(|call| {
            let rotation = Rotation::decode(call, &current).ok()?;
            let unchanged = rotation.threshold == manifest.threshold
                && rotation.committee.viewing_threshold() == manifest.viewing_threshold
                && rotation.committee.shares() == shares.as_slice();
            let slot = swapped_slot(&current, rotation.signers.keys()).ok()?;
            unchanged.then_some((slot, rotation))
        })
        .ok_or(Error::NotASignerSwap(
            "no call swaps a signer of this committee",
        ))?;
    let (slot, rotation) = swap;
    let mut next = manifest.clone();
    let position = usize::try_from(slot).map_err(|_| Error::UnknownSlot(slot))?;
    next.members[position].signer_key = WirePoint::from(&rotation.signers.keys()[position]);
    Vault::from_manifest(&next, config)?;
    Ok(next)
}

/// The one slot where `next` differs from `current`, which must have the same size; the new key must not be a current
/// signer.
fn swapped_slot(current: &[Point], next: &[Point]) -> Result<u32, Error> {
    if current.len() != next.len() {
        return Err(Error::NotASignerSwap("it changes the member count"));
    }
    let mut changed = current
        .iter()
        .zip(next)
        .enumerate()
        .filter(|(_, (old, new))| old != new);
    let (Some((slot, (_, key))), None) = (changed.next(), changed.next()) else {
        return Err(Error::NotASignerSwap("it must replace exactly one key"));
    };
    if current.contains(key) {
        return Err(Error::NotASignerSwap("the new key is already a signer"));
    }
    u32::try_from(slot).map_err(|_| Error::UnknownSlot(u32::MAX))
}
