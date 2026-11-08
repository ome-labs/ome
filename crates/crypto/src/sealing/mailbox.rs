//! Pairwise mailboxes for store-and-forward through an untrusted relay.
//!
//! The mailbox where keeper `A` leaves messages for keeper `B` on day `d` has the id
//! `SHA-256("ome.mailbox.v0" ‖ X25519(A, B) ‖ A_pub ‖ B_pub ‖ d)`. Only `A` and `B` can compute it, so the relay
//! cannot link a mailbox to a keeper, a vault, the reverse direction or another day. Payloads are HPKE envelopes
//! (Auth mode) bound to the mailbox id and day, so the relay holds ciphertext only and the recipient learns which keeper
//! sealed each one.

use rand_core::CryptoRng;
use sha2::{Digest, Sha256};
use x25519_dalek::{PublicKey, StaticSecret};
use zeroize::Zeroizing;

use crate::{
    Error,
    sealing::envelope::{self, Context, Envelope, KEY_BYTES, TransportPublic, TransportSecret},
};

/// Domain tag of mailbox ids.
pub const MAILBOX_DOMAIN: &[u8] = b"ome.mailbox.v0";
/// Envelope kind of mailbox payloads.
pub const KIND_MAILBOX: u16 = 3;

/// Id of the mailbox `sender` writes to for `recipient` on `day` (days since the unix epoch). `mine` is either party's
/// secret and `peer` the other party's public key.
///
/// # Errors
/// [`Error::EnvelopeKey`] if the key exchange is degenerate (a low-order peer key).
pub fn mailbox_id(
    mine: &TransportSecret,
    peer: &TransportPublic,
    sender: &TransportPublic,
    recipient: &TransportPublic,
    day: u32,
) -> Result<[u8; 32], Error> {
    let secret = StaticSecret::from(*mine.to_bytes());
    let shared = secret.diffie_hellman(&PublicKey::from(peer.0));
    if !shared.was_contributory() {
        return Err(Error::EnvelopeKey);
    }
    let mut hasher = Sha256::new();
    hasher.update(MAILBOX_DOMAIN);
    hasher.update(shared.as_bytes());
    hasher.update(sender.0);
    hasher.update(recipient.0);
    hasher.update(day.to_be_bytes());
    Ok(hasher.finalize().into())
}

fn context(mailbox: &[u8; 32], day: u32) -> Context {
    Context {
        session: *mailbox,
        epoch: day,
        kind: KIND_MAILBOX,
        sender: 0,
        recipient: 0,
    }
}

/// Seals `plaintext` for `recipient` into mailbox `mailbox` of `day`; returns `encapsulated key ‖ ciphertext`.
///
/// # Errors
/// Envelope errors.
pub fn seal(
    mailbox: &[u8; 32],
    day: u32,
    sender: &TransportSecret,
    recipient: &TransportPublic,
    plaintext: &[u8],
    rng: &mut impl CryptoRng,
) -> Result<Vec<u8>, Error> {
    let sealed = envelope::seal(&context(mailbox, day), sender, recipient, plaintext, rng)?;
    let mut out = Vec::with_capacity(KEY_BYTES + sealed.ciphertext.len());
    out.extend_from_slice(&sealed.encapsulated_key);
    out.extend_from_slice(&sealed.ciphertext);
    Ok(out)
}

/// Opens a payload from mailbox `mailbox` of `day`, sealed by `sender`.
///
/// # Errors
/// [`Error::InvalidLength`] for a truncated payload, [`Error::EnvelopeOpen`] if it was not sealed by `sender` for this
/// mailbox or was modified.
pub fn open(
    mailbox: &[u8; 32],
    day: u32,
    recipient: &TransportSecret,
    sender: &TransportPublic,
    sealed: &[u8],
) -> Result<Zeroizing<Vec<u8>>, Error> {
    if sealed.len() <= KEY_BYTES {
        return Err(Error::InvalidLength {
            expected: KEY_BYTES + 1,
            actual: sealed.len(),
        });
    }
    let (key, ciphertext) = sealed.split_at(KEY_BYTES);
    let mut encapsulated_key = [0u8; KEY_BYTES];
    encapsulated_key.copy_from_slice(key);
    envelope::open(
        &context(mailbox, day),
        recipient,
        sender,
        &Envelope {
            encapsulated_key,
            ciphertext: ciphertext.to_vec(),
        },
    )
}
