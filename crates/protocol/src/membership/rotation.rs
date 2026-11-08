use ome_crypto::{Error as CryptoError, Point, threshold::shamir::Commitments};

use super::Vault;
use crate::{
    Error,
    vault::{Committee, Config, SignerSet},
};

/// A proposed `rotate_membership`: the new signer set, spending threshold and committee from the reshare.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rotation {
    /// New signer keys in slot order.
    pub signers: SignerSet,
    /// New spending threshold `k`.
    pub threshold: u32,
    /// New viewing committee: `t_v` and the verification shares the reshare produced.
    pub committee: Committee,
}

/// Checks that `committee`'s verification shares lie on one polynomial of degree `t_v - 1` whose value at zero is
/// `group_key`, so the committee can decrypt for the vault and nothing else.
///
/// # Errors
/// [`Error::ShareOffPolynomial`] naming the first share off the polynomial, or [`Error::GroupKeyMismatch`].
pub fn verify_shares(group_key: &Point, committee: &Committee) -> Result<(), Error> {
    let threshold = usize::try_from(committee.viewing_threshold()).unwrap_or(usize::MAX);
    let commitments = Commitments::interpolate(threshold, &committee.evaluations()).map_err(
        |error| match error {
            CryptoError::InvalidShare(index) => Error::ShareOffPolynomial(index),
            other => Error::Crypto(other),
        },
    )?;
    if commitments.constant_term() != group_key {
        return Err(Error::GroupKeyMismatch);
    }
    Ok(())
}

/// Everything an approver checks before signing a rotation, returning the config it produces:
///
/// - the contract's own rules: the current set matches, at most `t_v - 1` members leave, thresholds in range;
/// - the new shares belong to the vault's viewing key (the contract checks only that they are well formed);
/// - the committee was reshared: no new share equals a current one, so removed members' shares stop working.
///
/// A rotation that fails is refused, not warned about: a bad committee can leave the vault unreadable for good.
///
/// # Errors
/// Config transition errors, [`Error::ShareOffPolynomial`], [`Error::GroupKeyMismatch`] or
/// [`Error::SharesNotRefreshed`].
pub fn check_rotation(vault: &Vault, rotation: &Rotation) -> Result<Config, Error> {
    let next = vault.config().rotate(
        vault.signers(),
        &rotation.signers,
        rotation.threshold,
        &rotation.committee,
    )?;
    verify_shares(vault.group_key(), &rotation.committee)?;
    if rotation
        .committee
        .shares()
        .iter()
        .any(|share| vault.committee().shares().contains(share))
    {
        return Err(Error::SharesNotRefreshed);
    }
    Ok(next)
}
