use ome_crypto::{Base, Point};
use ome_wire::messages::ceremony::Manifest;

use super::verify_shares;
use crate::{
    Error,
    vault::{Committee, Config, SignerSet},
};

/// A vault as a member's software may act on it: a manifest that matches the vault's on-chain config.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Vault {
    address: Base,
    epoch: u32,
    config: Config,
    signers: SignerSet,
    committee: Committee,
    group_key: Point,
}

impl Vault {
    /// Checks `manifest` against `config`, the vault's current config note:
    ///
    /// - the manifest is well formed and its vault address follows from its preaddress and group key;
    /// - its signer keys and thresholds match the stored signer-set commitment and config;
    /// - its verification shares and viewing threshold match the stored committee commitment;
    /// - those shares lie on one polynomial of degree `t_v - 1` whose constant term is the vault's viewing key;
    /// - its epoch is not newer than the config.
    ///
    /// A keeper or client refuses to act on a manifest that fails any check.
    ///
    /// # Errors
    /// [`Error::SignerSetMismatch`], [`Error::CommitteeMismatch`], [`Error::ManifestMismatch`],
    /// [`Error::GroupKeyMismatch`], [`Error::ShareOffPolynomial`], or wire and crypto errors.
    pub fn from_manifest(manifest: &Manifest, config: &Config) -> Result<Self, Error> {
        manifest.validate()?;
        let address = manifest.address_context()?.address();
        let signers = SignerSet::new(manifest.signer_keys()?)?;
        if signers.commitment() != config.signer_set_commitment
            || signers.count() != config.signer_count
        {
            return Err(Error::SignerSetMismatch);
        }
        if manifest.threshold != config.threshold {
            return Err(Error::ManifestMismatch("threshold"));
        }
        let committee =
            Committee::new(manifest.viewing_threshold, manifest.verification_shares()?)?;
        if committee.commitment() != config.committee_commitment {
            return Err(Error::CommitteeMismatch);
        }
        if manifest.epoch > config.version {
            return Err(Error::ManifestMismatch("epoch"));
        }
        let group_key = Point::try_from(&manifest.group_key).map_err(Error::Wire)?;
        verify_shares(&group_key, &committee)?;
        Ok(Self {
            address,
            epoch: manifest.epoch,
            config: *config,
            signers,
            committee,
            group_key,
        })
    }

    /// Vault address.
    #[must_use]
    pub fn address(&self) -> Base {
        self.address
    }

    /// Committee epoch.
    #[must_use]
    pub fn epoch(&self) -> u32 {
        self.epoch
    }

    /// The config the manifest was checked against.
    #[must_use]
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Signer keys in slot order.
    #[must_use]
    pub fn signers(&self) -> &SignerSet {
        &self.signers
    }

    /// The viewing committee.
    #[must_use]
    pub fn committee(&self) -> &Committee {
        &self.committee
    }

    /// The vault's incoming viewing key `Ivpk`.
    #[must_use]
    pub fn group_key(&self) -> &Point {
        &self.group_key
    }
}
