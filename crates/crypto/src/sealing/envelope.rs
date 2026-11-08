//! Authenticated envelopes for private protocol messages (DKG and reshare shares).
//!
//! HPKE (RFC 9180) in Auth mode with DHKEM(X25519, HKDF-SHA256), HKDF-SHA256 and ChaCha20-Poly1305. The recipient learns
//! which keeper sealed the envelope (sender authentication); third-party verifiable attribution for complaints comes
//! from the sender's signature at the wire layer. The [`Context`] goes into the HPKE `info`, so an envelope opens only
//! for the ceremony, epoch, message kind, sender and recipient it was sealed for.

use hpke::{
    Deserializable, Kem as _, OpModeR, OpModeS, Serializable, aead::ChaCha20Poly1305,
    kdf::HkdfSha256, kem::X25519HkdfSha256,
};
use rand_core::CryptoRng;
use zeroize::{Zeroize, Zeroizing};

use crate::{
    Error, Scalar, SecretScalar,
    curve::{FIELD_BYTES, scalar_from_bytes, scalar_to_bytes},
    threshold::shamir::MemberIndex,
};

type Kem = X25519HkdfSha256;
type Aead = ChaCha20Poly1305;
type Kdf = HkdfSha256;

/// Domain tag prefixed to every HPKE `info`.
pub const DOMAIN: &[u8] = b"ome.envelope.v0";
/// Length of an encoded transport public key and of an encapsulated key.
pub const KEY_BYTES: usize = 32;

/// What an envelope is bound to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Context {
    /// Ceremony session id (DKG / reshare) or vault identifier.
    pub session: [u8; 32],
    /// Committee epoch.
    pub epoch: u32,
    /// Message kind (e.g. DKG round 2, reshare sub-share); defined by the wire layer.
    pub kind: u16,
    /// Sender member index.
    pub sender: MemberIndex,
    /// Recipient member index.
    pub recipient: MemberIndex,
}

impl Context {
    fn info(&self) -> Vec<u8> {
        let mut info = Vec::with_capacity(DOMAIN.len() + 32 + 4 + 2 + 4 + 4);
        info.extend_from_slice(DOMAIN);
        info.extend_from_slice(&self.session);
        info.extend_from_slice(&self.epoch.to_be_bytes());
        info.extend_from_slice(&self.kind.to_be_bytes());
        info.extend_from_slice(&self.sender.to_be_bytes());
        info.extend_from_slice(&self.recipient.to_be_bytes());
        info
    }
}

/// A keeper's long-lived transport secret key (X25519). Zeroized on drop; never printed.
pub struct TransportSecret(Zeroizing<[u8; KEY_BYTES]>);

/// A keeper's transport public key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TransportPublic(pub [u8; KEY_BYTES]);

impl TransportSecret {
    /// Generates a fresh transport key pair.
    pub fn generate(rng: &mut impl CryptoRng) -> (Self, TransportPublic) {
        let (secret, public) = Kem::gen_keypair_with_rng(rng);
        let mut bytes = [0u8; KEY_BYTES];
        bytes.copy_from_slice(&secret.to_bytes());
        let mut public_bytes = [0u8; KEY_BYTES];
        public_bytes.copy_from_slice(&public.to_bytes());
        let secret = Self(Zeroizing::new(bytes));
        bytes.zeroize();
        (secret, TransportPublic(public_bytes))
    }

    /// Restores a secret key from its 32-byte encoding.
    #[must_use]
    pub fn from_bytes(bytes: [u8; KEY_BYTES]) -> Self {
        Self(Zeroizing::new(bytes))
    }

    /// The 32-byte encoding, for storage in the OS keystore.
    #[must_use]
    pub fn to_bytes(&self) -> Zeroizing<[u8; KEY_BYTES]> {
        self.0.clone()
    }

    /// The matching public key.
    ///
    /// # Errors
    /// [`Error::EnvelopeKey`] for an invalid encoding.
    pub fn public(&self) -> Result<TransportPublic, Error> {
        let secret = private_key(self)?;
        let mut out = [0u8; KEY_BYTES];
        out.copy_from_slice(&Kem::sk_to_pk(&secret).to_bytes());
        Ok(TransportPublic(out))
    }
}

impl core::fmt::Debug for TransportSecret {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("TransportSecret(<redacted>)")
    }
}

/// A sealed message: the encapsulated key and the ciphertext (with tag).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Envelope {
    /// HPKE encapsulated key.
    pub encapsulated_key: [u8; KEY_BYTES],
    /// AEAD ciphertext including the tag.
    pub ciphertext: Vec<u8>,
}

/// Seals `plaintext` from `sender` to `recipient` for `context`.
///
/// # Errors
/// [`Error::EnvelopeKey`] for invalid keys, [`Error::EnvelopeSeal`] if sealing fails.
pub fn seal(
    context: &Context,
    sender: &TransportSecret,
    recipient: &TransportPublic,
    plaintext: &[u8],
    rng: &mut impl CryptoRng,
) -> Result<Envelope, Error> {
    let sender_secret = private_key(sender)?;
    let sender_public = Kem::sk_to_pk(&sender_secret);
    let recipient = public_key(recipient)?;
    let mode = OpModeS::Auth((sender_secret, sender_public));
    let (encapsulated, ciphertext) = hpke::single_shot_seal_with_rng::<Aead, Kdf, Kem>(
        &mode,
        &recipient,
        &context.info(),
        plaintext,
        &[],
        rng,
    )
    .map_err(|_| Error::EnvelopeSeal)?;
    let mut encapsulated_key = [0u8; KEY_BYTES];
    encapsulated_key.copy_from_slice(&encapsulated.to_bytes());
    Ok(Envelope {
        encapsulated_key,
        ciphertext,
    })
}

/// Opens an envelope sealed by `sender` for `context`.
///
/// # Errors
/// [`Error::EnvelopeKey`] for invalid keys, [`Error::EnvelopeOpen`] if the envelope was not sealed by `sender` for this
/// recipient and context, or was modified.
pub fn open(
    context: &Context,
    recipient: &TransportSecret,
    sender: &TransportPublic,
    envelope: &Envelope,
) -> Result<Zeroizing<Vec<u8>>, Error> {
    let recipient = private_key(recipient)?;
    let mode = OpModeR::Auth(public_key(sender)?);
    let encapsulated = <Kem as hpke::Kem>::EncappedKey::from_bytes(&envelope.encapsulated_key)
        .map_err(|_| Error::EnvelopeKey)?;
    hpke::single_shot_open::<Aead, Kdf, Kem>(
        &mode,
        &recipient,
        &encapsulated,
        &context.info(),
        &envelope.ciphertext,
        &[],
    )
    .map(Zeroizing::new)
    .map_err(|_| Error::EnvelopeOpen)
}

/// Seals a secret scalar (a DKG share or reshare sub-share).
///
/// # Errors
/// As [`seal`].
pub fn seal_scalar(
    context: &Context,
    sender: &TransportSecret,
    recipient: &TransportPublic,
    value: &SecretScalar,
    rng: &mut impl CryptoRng,
) -> Result<Envelope, Error> {
    let bytes = Zeroizing::new(scalar_to_bytes(value.expose()));
    seal(context, sender, recipient, bytes.as_slice(), rng)
}

/// Opens a secret scalar sealed with [`seal_scalar`].
///
/// # Errors
/// As [`open`], plus decoding errors for a malformed scalar.
pub fn open_scalar(
    context: &Context,
    recipient: &TransportSecret,
    sender: &TransportPublic,
    envelope: &Envelope,
) -> Result<SecretScalar, Error> {
    let bytes = open(context, recipient, sender, envelope)?;
    if bytes.len() != FIELD_BYTES {
        return Err(Error::InvalidLength {
            expected: FIELD_BYTES,
            actual: bytes.len(),
        });
    }
    let value: Scalar = scalar_from_bytes(&bytes)?;
    Ok(SecretScalar::new(value))
}

fn private_key(secret: &TransportSecret) -> Result<<Kem as hpke::Kem>::PrivateKey, Error> {
    <Kem as hpke::Kem>::PrivateKey::from_bytes(secret.0.as_slice()).map_err(|_| Error::EnvelopeKey)
}

fn public_key(public: &TransportPublic) -> Result<<Kem as hpke::Kem>::PublicKey, Error> {
    <Kem as hpke::Kem>::PublicKey::from_bytes(&public.0).map_err(|_| Error::EnvelopeKey)
}
