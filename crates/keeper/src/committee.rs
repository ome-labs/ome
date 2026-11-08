//! A vault committee: a checked manifest and the crypto context derived from it.

use ome_crypto::{
    Base, Point,
    aztec::AddressContext,
    threshold::{ecdh::EcdhContext, shamir::MemberIndex},
};
use ome_net::Peer;
use ome_protocol::{membership::verify_shares, vault::Committee as Viewers};
use ome_wire::{
    IdentityPublic,
    messages::ceremony::{Manifest, Member},
    types::parse_field,
};

use crate::Result;

/// A vault committee whose manifest has been checked: its structure, its vault address against `preaddress` and
/// `Ivpk`, and its verification shares lying on one polynomial of degree `t_v - 1` through `Ivpk`
/// (`ome_protocol::membership::verify_shares`).
#[derive(Clone, Debug)]
pub struct Committee {
    /// The manifest.
    pub manifest: Manifest,
    /// Vault address as a field element.
    pub vault: Base,
    /// Address-secret context.
    pub context: AddressContext,
    /// The viewing committee as the contract commits to it.
    pub viewers: Viewers,
    /// Public address-secret shares `A_i`, in member order.
    pub address_shares: Vec<Point>,
}

impl Committee {
    /// Validates `manifest`.
    ///
    /// # Errors
    /// Wire, crypto or protocol errors naming what is inconsistent.
    pub fn new(manifest: Manifest) -> Result<Self> {
        manifest.validate()?;
        let context = manifest.address_context()?;
        let shares = manifest.verification_shares()?;
        let viewers = Viewers::new(manifest.viewing_threshold, shares.clone())?;
        verify_shares(&Point::try_from(&manifest.group_key)?, &viewers)?;
        let address_shares = shares
            .iter()
            .map(|y| context.address_verification_share(y))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            vault: parse_field(&manifest.vault)?,
            manifest,
            context,
            viewers,
            address_shares,
        })
    }

    /// Vault address bytes.
    #[must_use]
    pub fn vault_bytes(&self) -> [u8; 32] {
        self.manifest.vault
    }

    /// Committee epoch.
    #[must_use]
    pub fn epoch(&self) -> u32 {
        self.manifest.epoch
    }

    /// Viewing threshold `t_v`.
    #[must_use]
    pub fn threshold(&self) -> usize {
        usize::try_from(self.manifest.viewing_threshold).unwrap_or(usize::MAX)
    }

    /// The member with `identity`.
    #[must_use]
    pub fn member(&self, identity: &IdentityPublic) -> Option<&Member> {
        self.manifest
            .members
            .iter()
            .find(|m| &m.identity == identity)
    }

    /// `A_i` of member `index`.
    #[must_use]
    pub fn address_share(&self, index: MemberIndex) -> Option<Point> {
        self.address_shares
            .get(usize::try_from(index).ok()?.checked_sub(1)?)
            .copied()
    }

    /// What partials are bound to.
    #[must_use]
    pub fn ecdh_context(&self) -> EcdhContext {
        EcdhContext {
            vault: self.vault,
            epoch: self.manifest.epoch,
        }
    }

    /// The contract's committee commitment for this committee.
    #[must_use]
    pub fn commitment(&self) -> Base {
        self.viewers.commitment()
    }

    /// Every member except `me`, as transport peers.
    #[must_use]
    pub fn peers_except(&self, me: &IdentityPublic) -> Vec<Peer> {
        self.manifest
            .members
            .iter()
            .filter(|m| &m.identity != me)
            .map(Peer::from)
            .collect()
    }
}
