//! A keeper's libp2p identity is its `ome-wire` keeper identity: the same Ed25519 key. The Noise handshake therefore
//! authenticates the keeper, and the frames it signs must carry the same key.

use libp2p::{PeerId, identity};
use ome_wire::{IdentityPublic, KeeperIdentity};

use crate::Error;

/// The libp2p keypair of a keeper identity.
///
/// # Errors
/// [`Error::Invalid`] if the seed is not a valid Ed25519 secret (it always is for a [`KeeperIdentity`]).
pub fn keypair(keeper: &KeeperIdentity) -> Result<identity::Keypair, Error> {
    let mut seed = *keeper.seed();
    identity::Keypair::ed25519_from_bytes(&mut seed).map_err(|e| Error::Invalid(e.to_string()))
}

/// The libp2p peer id of a keeper identity.
///
/// # Errors
/// [`Error::Invalid`] if the identity is not a valid Ed25519 public key.
pub fn peer_id(identity: &IdentityPublic) -> Result<PeerId, Error> {
    let key = identity::ed25519::PublicKey::try_from_bytes(&identity.0)
        .map_err(|e| Error::Invalid(e.to_string()))?;
    Ok(identity::PublicKey::from(key).to_peer_id())
}

/// The keeper identity inside a peer id. Ed25519 peer ids inline the public key, so this needs no lookup; other key
/// types are not keepers.
#[must_use]
pub fn identity_of(peer: &PeerId) -> Option<IdentityPublic> {
    let hash = peer.as_ref();
    if hash.code() != 0 {
        return None;
    }
    let key = identity::PublicKey::try_decode_protobuf(hash.digest())
        .ok()?
        .try_into_ed25519()
        .ok()?;
    Some(IdentityPublic(key.to_bytes()))
}
