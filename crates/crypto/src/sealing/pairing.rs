//! The pairing mailbox of a device pairing offer. A keeper's offer carries a one-time secret; the device's pairing
//! request is sealed under a key derived from it into a mailbox whose id is derived from it too, so only someone
//! holding the offer can write a request the keeper opens, and the relay sees ciphertext under an id it cannot link to
//! anything.
//!
//! ```text
//! mailbox = SHA-256("ome.pair.mailbox.v0" ‖ secret)
//! key     = HMAC-SHA256(secret, "ome.pair.key.v0")
//! sealed  = nonce (24 B) ‖ XChaCha20-Poly1305(key, nonce, plaintext, aad = mailbox)
//! ```

use chacha20poly1305::{
    XChaCha20Poly1305, XNonce,
    aead::{Aead, Payload},
};
use hmac::{Hmac, KeyInit, Mac};
use rand_core::CryptoRng;
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use crate::Error;

/// Domain tag of pairing mailbox ids.
pub const PAIR_MAILBOX_DOMAIN: &[u8] = b"ome.pair.mailbox.v0";
/// Label of the pairing key.
pub const PAIR_KEY_LABEL: &[u8] = b"ome.pair.key.v0";

const NONCE_BYTES: usize = 24;

/// The pairing mailbox of an offer's secret.
#[must_use]
pub fn pairing_mailbox(secret: &[u8; 32]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(PAIR_MAILBOX_DOMAIN);
    hasher.update(secret);
    hasher.finalize().into()
}

fn cipher(secret: &[u8; 32]) -> Result<XChaCha20Poly1305, Error> {
    let mut mac =
        <Hmac<Sha256> as KeyInit>::new_from_slice(secret).map_err(|_| Error::EnvelopeKey)?;
    mac.update(PAIR_KEY_LABEL);
    let key = Zeroizing::new(<[u8; 32]>::from(mac.finalize().into_bytes()));
    <XChaCha20Poly1305 as chacha20poly1305::KeyInit>::new_from_slice(key.as_slice())
        .map_err(|_| Error::EnvelopeKey)
}

/// Seals a pairing request under the offer's secret.
///
/// # Errors
/// [`Error::EnvelopeSeal`].
pub fn seal(
    secret: &[u8; 32],
    plaintext: &[u8],
    rng: &mut impl CryptoRng,
) -> Result<Vec<u8>, Error> {
    let mut nonce = [0u8; NONCE_BYTES];
    rng.fill_bytes(&mut nonce);
    let ciphertext = cipher(secret)?
        .encrypt(
            &XNonce::from(nonce),
            Payload {
                msg: plaintext,
                aad: &pairing_mailbox(secret),
            },
        )
        .map_err(|_| Error::EnvelopeSeal)?;
    let mut out = Vec::with_capacity(NONCE_BYTES + ciphertext.len());
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

/// Opens a pairing request sealed under the offer's secret.
///
/// # Errors
/// [`Error::InvalidLength`] for a truncated input, [`Error::EnvelopeOpen`] if it was sealed under another secret or
/// modified.
pub fn open(secret: &[u8; 32], sealed: &[u8]) -> Result<Zeroizing<Vec<u8>>, Error> {
    if sealed.len() <= NONCE_BYTES {
        return Err(Error::InvalidLength {
            expected: NONCE_BYTES + 1,
            actual: sealed.len(),
        });
    }
    let (nonce, ciphertext) = sealed.split_at(NONCE_BYTES);
    let mut fixed = [0u8; NONCE_BYTES];
    fixed.copy_from_slice(nonce);
    cipher(secret)?
        .decrypt(
            &XNonce::from(fixed),
            Payload {
                msg: ciphertext,
                aad: &pairing_mailbox(secret),
            },
        )
        .map(Zeroizing::new)
        .map_err(|_| Error::EnvelopeOpen)
}
