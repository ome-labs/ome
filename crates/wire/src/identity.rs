use ed25519_dalek::{Signer, SigningKey, VerifyingKey};
use rand_core::CryptoRng;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::{Error, types::Bytes64};

/// Domain tag prefixed to every signed payload.
pub(crate) const SIGNING_DOMAIN: &[u8] = b"ome.wire.v0";

/// A keeper's Ed25519 identity. It signs every message the keeper sends and is separate from the member's Aztec
/// signing key. Zeroized on drop; `Debug` shows only the public key.
pub struct KeeperIdentity(SigningKey);

/// A keeper's public identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct IdentityPublic(pub [u8; 32]);

impl KeeperIdentity {
    /// Generates a new identity.
    pub fn generate(rng: &mut impl CryptoRng) -> Self {
        let mut seed = Zeroizing::new([0u8; 32]);
        rng.fill_bytes(seed.as_mut_slice());
        Self(SigningKey::from_bytes(&seed))
    }

    /// Restores an identity from its 32-byte seed.
    #[must_use]
    pub fn from_seed(seed: &[u8; 32]) -> Self {
        Self(SigningKey::from_bytes(seed))
    }

    /// The 32-byte seed, for the keeper's sealed storage.
    #[must_use]
    pub fn seed(&self) -> Zeroizing<[u8; 32]> {
        Zeroizing::new(self.0.to_bytes())
    }

    /// The public identity.
    #[must_use]
    pub fn public(&self) -> IdentityPublic {
        IdentityPublic(self.0.verifying_key().to_bytes())
    }

    pub(crate) fn sign(&self, payload: &[u8]) -> Bytes64 {
        Bytes64::from_array(&self.0.sign(&domain_separated(payload)).to_bytes())
    }
}

impl core::fmt::Debug for KeeperIdentity {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_tuple("KeeperIdentity")
            .field(&self.public())
            .finish()
    }
}

impl IdentityPublic {
    pub(crate) fn verify(&self, payload: &[u8], signature: &Bytes64) -> Result<(), Error> {
        let key = VerifyingKey::from_bytes(&self.0).map_err(|_| Error::InvalidSignature)?;
        let signature = ed25519_dalek::Signature::from_bytes(&signature.to_array());
        // `verify_strict` also rejects small-order keys and non-canonical signatures.
        key.verify_strict(&domain_separated(payload), &signature)
            .map_err(|_| Error::InvalidSignature)
    }
}

fn domain_separated(payload: &[u8]) -> Vec<u8> {
    let mut message = Vec::with_capacity(SIGNING_DOMAIN.len() + payload.len());
    message.extend_from_slice(SIGNING_DOMAIN);
    message.extend_from_slice(payload);
    message
}
