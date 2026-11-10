//! Threshold changes: `set_threshold(k)` keeps every signer and the committee, so keepers only adopt the new `k`.

use ome_wire::{messages::ceremony::Manifest, types::parse_field};

use super::{Vault, rotation::small};
use crate::{Error, payload::Call, vault::Config, vault::constants::SET_THRESHOLD_SELECTOR};

/// The manifest after a threshold change that landed: `calls` are an executed proposal's calls, `config` the vault's
/// config note read after it ran. One call must be `set_threshold` on `manifest`'s vault, and the manifest with that
/// threshold must pass [`Vault::from_manifest`] against `config`, so a keeper adopts a threshold only once the chain
/// holds it.
///
/// # Errors
/// [`Error::NotAThresholdChange`] if no call sets this vault's threshold, or the [`Vault::from_manifest`] error.
pub fn rethresholded_manifest(
    manifest: &Manifest,
    config: &Config,
    calls: &[Call],
) -> Result<Manifest, Error> {
    let vault = parse_field(&manifest.vault)?;
    let threshold = calls
        .iter()
        .filter(|call| {
            call.selector == SET_THRESHOLD_SELECTOR && parse_field(&call.to).ok() == Some(vault)
        })
        .find_map(|call| match call.args.as_slice() {
            [k] => small(parse_field(k).ok()?).ok(),
            _ => None,
        })
        .ok_or(Error::NotAThresholdChange)?;
    let mut next = manifest.clone();
    next.threshold = threshold;
    Vault::from_manifest(&next, config)?;
    Ok(next)
}
