//! Founding rules: the proof a co-founder presents with an invite, the founding secrets the host seals to each member,
//! and the protocol version the vault is founded on.
//!
//! - `proof = HMAC-SHA256(invite secret, "ome.lobby.join.v0" ‖ lobby ‖ identity ‖ transport)`, checked in constant
//!   time. It binds the join to the joiner's keeper keys, so a seen join cannot be replayed for another keeper.
//! - Secrets are sealed with an HPKE envelope of kind [`KIND_LOBBY_SECRETS`], bound to the plan's session, epoch 0,
//!   the host (index 1) and the recipient's 1-based index.

use hmac::{Hmac, KeyInit, Mac};
use ome_crypto::{
    sealing::envelope::{self, Context, Envelope, TransportPublic, TransportSecret},
    threshold::shamir::MemberIndex,
};
use ome_wire::{
    IdentityPublic,
    messages::lobby::{JoinSecret, KeeperContact, LobbyJoin, LobbyMember},
    version::{self, SUPPORTED},
};
use rand_core::CryptoRng;
use sha2::Sha256;
use subtle::ConstantTimeEq;
use zeroize::Zeroizing;

use crate::Error;

/// Domain tag of join proofs.
pub const JOIN_DOMAIN: &[u8] = b"ome.lobby.join.v0";
/// Envelope kind of founding secrets (after DKG share 1, reshare sub-share 2 and mailbox 3).
pub const KIND_LOBBY_SECRETS: u16 = 4;
/// The host's member index in every lobby plan.
pub const HOST_INDEX: MemberIndex = 1;

/// The proof that the keeper `joiner` holds the invite secret for `lobby`.
#[must_use]
pub fn join_proof(secret: &JoinSecret, lobby: &[u8; 16], joiner: &KeeperContact) -> [u8; 32] {
    // HMAC takes keys of any length, so a 32-byte key cannot fail.
    let mut mac = <Hmac<Sha256> as KeyInit>::new_from_slice(&secret.0)
        .unwrap_or_else(|_| unreachable!("HMAC takes any key length"));
    mac.update(JOIN_DOMAIN);
    mac.update(lobby);
    mac.update(&joiner.identity.0);
    mac.update(&joiner.transport);
    mac.finalize().into_bytes().into()
}

/// Checks a join request the host received in a message signed by `signer`: the joiner is the signer, and the proof
/// matches the invite secret.
///
/// # Errors
/// [`Error::UnexpectedSigner`] or [`Error::InvalidJoinProof`].
pub fn verify_join(
    join: &LobbyJoin,
    secret: &JoinSecret,
    signer: &IdentityPublic,
) -> Result<(), Error> {
    if &join.member.keeper.identity != signer {
        return Err(Error::UnexpectedSigner);
    }
    let expected = join_proof(secret, &join.lobby, &join.member.keeper);
    if bool::from(expected.ct_eq(&join.proof)) {
        Ok(())
    } else {
        Err(Error::InvalidJoinProof)
    }
}

/// The protocol version a vault founded by `members` uses: the newest one this build and every member's keeper speak.
///
/// # Errors
/// [`ome_wire::Error::NoCommonVersion`] naming no version, so the host can tell who must update.
pub fn plan_version(members: &[LobbyMember]) -> Result<u16, Error> {
    Ok(version::negotiate(
        core::iter::once(&SUPPORTED).chain(members.iter().map(|m| &m.keeper.versions)),
    )?)
}

/// Seals the founding `secrets` from the host to member `recipient` of the plan `session`.
///
/// # Errors
/// Envelope errors.
pub fn seal_secrets(
    session: &[u8; 32],
    recipient: MemberIndex,
    host: &TransportSecret,
    recipient_key: &TransportPublic,
    secrets: &[u8],
    rng: &mut impl CryptoRng,
) -> Result<Envelope, Error> {
    Ok(envelope::seal(
        &context(session, recipient),
        host,
        recipient_key,
        secrets,
        rng,
    )?)
}

/// Opens the founding secrets the host sealed for member `recipient` of the plan `session`.
///
/// # Errors
/// [`ome_crypto::Error::EnvelopeOpen`] if the envelope was not sealed by `host` for this session and recipient.
pub fn open_secrets(
    session: &[u8; 32],
    recipient: MemberIndex,
    recipient_secret: &TransportSecret,
    host: &TransportPublic,
    envelope: &Envelope,
) -> Result<Zeroizing<Vec<u8>>, Error> {
    Ok(envelope::open(
        &context(session, recipient),
        recipient_secret,
        host,
        envelope,
    )?)
}

fn context(session: &[u8; 32], recipient: MemberIndex) -> Context {
    Context {
        session: *session,
        epoch: 0,
        kind: KIND_LOBBY_SECRETS,
        sender: HOST_INDEX,
        recipient,
    }
}
