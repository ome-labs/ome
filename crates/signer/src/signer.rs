use ome_crypto::{Base, Point, schnorr};
use rand_core::CryptoRng;

use crate::Error;

/// A message the signer has agreed to sign: an approval or rejection message produced after `ome-protocol`'s checks.
/// Only this crate can create one, so no caller can have a [`Signer`] sign an arbitrary hash.
pub struct Message(Base);

impl Message {
    pub(crate) fn new(value: Base) -> Self {
        Self(value)
    }

    /// The field element that is signed.
    #[must_use]
    pub fn value(&self) -> Base {
        self.0
    }
}

/// Something that holds a vault signing key. [`crate::SigningKey`] is the software implementation; a hardware-backed
/// signer (Secure Enclave, Android Keystore) can implement this later without changing callers, once the contract
/// accepts its signature scheme.
pub trait Signer {
    /// The key's public half, as it appears in the vault's signer set.
    fn public_key(&self) -> Point;

    /// Signs a checked message.
    ///
    /// # Errors
    /// Implementation-specific signing failures.
    fn sign<R: CryptoRng>(
        &self,
        message: &Message,
        rng: &mut R,
    ) -> Result<schnorr::Signature, Error>;
}
