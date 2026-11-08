use ome_crypto::sealing::envelope::{TransportPublic, TransportSecret};
use ome_wire::KeeperIdentity;

use super::SecretStore;
use crate::{Error, Result, random::os_rng};

const IDENTITY: &str = "identity";
const TRANSPORT: &str = "transport";

/// The keeper's long-lived keys: Ed25519 identity (signs every message; the future peer id) and X25519 transport key
/// (envelopes and the encrypted peer channel). Created on first run, sealed at rest.
pub struct KeeperKeys {
    /// Signing identity.
    pub identity: KeeperIdentity,
    /// Transport secret.
    pub transport: TransportSecret,
    /// Transport public key.
    pub transport_public: TransportPublic,
}

impl KeeperKeys {
    /// Loads the keys, generating them on first use.
    ///
    /// # Errors
    /// Storage or unseal errors.
    pub fn load_or_create(secrets: &SecretStore) -> Result<Self> {
        if !secrets.exists(IDENTITY) {
            let identity = KeeperIdentity::generate(&mut os_rng());
            secrets.put(IDENTITY, identity.seed().as_slice())?;
        }
        if !secrets.exists(TRANSPORT) {
            let (transport, _) = TransportSecret::generate(&mut os_rng());
            secrets.put(TRANSPORT, transport.to_bytes().as_slice())?;
        }
        let identity = KeeperIdentity::from_seed(&fixed(&secrets.get(IDENTITY)?, IDENTITY)?);
        let transport = TransportSecret::from_bytes(fixed(&secrets.get(TRANSPORT)?, TRANSPORT)?);
        let transport_public = transport.public()?;
        Ok(Self {
            identity,
            transport,
            transport_public,
        })
    }
}

fn fixed(bytes: &[u8], name: &str) -> Result<[u8; 32]> {
    bytes.try_into().map_err(|_| Error::Unseal(name.into()))
}
