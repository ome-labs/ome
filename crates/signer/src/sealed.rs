//! The at-rest form of a signing key: a small JSON document.
//!
//! ```json
//! { "v": "ome.signer.v0",
//!   "publicKey": { "x": "0x…", "y": "0x…" },
//!   "kdf": { "alg": "argon2id", "salt": "…", "memoryKib": 65536, "passes": 3, "lanes": 1 },
//!   "nonce": "…", "ciphertext": "…" }
//! ```
//!
//! The ciphertext is ChaCha20-Poly1305 of the 32-byte scalar. Its associated data binds the version, the KDF and its
//! parameters, and the public key, so a file cannot be edited to claim cheaper parameters or another member's key.

use chacha20poly1305::{
    ChaCha20Poly1305, Key, KeyInit, Nonce,
    aead::{Aead, Payload},
};
use ome_crypto::{Point, SecretScalar, curve};
use rand_core::CryptoRng;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::{Argon2Params, Error, SealingKey, Signer, SigningKey};

/// Format tag of sealed keys.
pub const SEALED_VERSION: &str = "ome.signer.v0";
/// Largest sealed-key document accepted.
const MAX_JSON_BYTES: usize = 4096;
const SALT_BYTES: usize = 16;
const NONCE_BYTES: usize = 12;
/// The 32-byte scalar plus the 16-byte tag.
const CIPHERTEXT_BYTES: usize = 48;

/// What unlocks a sealed key.
pub enum Secret<'a> {
    /// The member's passphrase (Argon2id).
    Passphrase(&'a str),
    /// A 32-byte key: one the platform holds, or the key derived from the passphrase and kept behind biometrics.
    Key(&'a SealingKey),
}

/// How the sealing key is obtained.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "alg",
    rename_all = "lowercase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
enum Kdf {
    /// Argon2id over the passphrase.
    Argon2id {
        salt: String,
        memory_kib: u32,
        passes: u32,
        lanes: u32,
    },
    /// A key the platform holds.
    External,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PublicKey {
    x: String,
    y: String,
}

/// A sealed signing key, ready to store.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SealedKey {
    v: String,
    public_key: PublicKey,
    kdf: Kdf,
    nonce: String,
    ciphertext: String,
}

impl SealedKey {
    /// Seals `key` under a passphrase (Argon2id, [`Argon2Params::DEFAULT`]) or a platform key.
    ///
    /// # Errors
    /// [`Error::Kdf`] if key derivation fails.
    pub fn seal(
        key: &SigningKey,
        secret: &Secret<'_>,
        rng: &mut impl CryptoRng,
    ) -> Result<Self, Error> {
        let derived;
        let (kdf, sealing) = match secret {
            Secret::Passphrase(passphrase) => {
                let mut salt = [0u8; SALT_BYTES];
                rng.fill_bytes(&mut salt);
                let params = Argon2Params::DEFAULT;
                derived = params.derive(passphrase, &salt)?;
                (argon2id(&salt, params), &derived)
            }
            Secret::Key(key) => (Kdf::External, *key),
        };
        let mut nonce = [0u8; NONCE_BYTES];
        rng.fill_bytes(&mut nonce);
        let public = key.public_key();
        let mut sealed = Self {
            v: SEALED_VERSION.to_owned(),
            public_key: PublicKey {
                x: hex32(&curve::base_to_bytes(&public.x())),
                y: hex32(&curve::base_to_bytes(&public.y())),
            },
            kdf,
            nonce: hex::encode(nonce),
            ciphertext: String::new(),
        };
        let plaintext = Zeroizing::new(curve::scalar_to_bytes(key.secret().expose()));
        let ciphertext = cipher(sealing)
            .encrypt(
                &Nonce::from(nonce),
                Payload {
                    msg: plaintext.as_slice(),
                    aad: &sealed.associated_data()?,
                },
            )
            .map_err(|_| Error::Kdf)?;
        sealed.ciphertext = hex::encode(ciphertext);
        Ok(sealed)
    }

    /// Opens the key.
    ///
    /// # Errors
    /// [`Error::WrongSecret`] for a wrong secret or a modified file, [`Error::WrongSecretKind`],
    /// [`Error::UnsupportedVersion`], [`Error::KdfOutOfRange`] or [`Error::Malformed`].
    pub fn open(&self, secret: &Secret<'_>) -> Result<SigningKey, Error> {
        let derived;
        let sealing = match (&self.kdf, secret) {
            (
                Kdf::Argon2id {
                    salt,
                    memory_kib,
                    passes,
                    lanes,
                },
                Secret::Passphrase(passphrase),
            ) => {
                let params = Argon2Params {
                    memory_kib: *memory_kib,
                    passes: *passes,
                    lanes: *lanes,
                };
                derived = params.derive(passphrase, &bytes::<SALT_BYTES>(salt, "salt")?)?;
                &derived
            }
            (Kdf::Argon2id { .. } | Kdf::External, Secret::Key(key)) => *key,
            (Kdf::External, Secret::Passphrase(_)) => {
                return Err(Error::WrongSecretKind {
                    expected: "platform key",
                });
            }
        };
        let nonce = bytes::<NONCE_BYTES>(&self.nonce, "nonce")?;
        let ciphertext = bytes::<CIPHERTEXT_BYTES>(&self.ciphertext, "ciphertext")?;
        let plaintext = Zeroizing::new(
            cipher(sealing)
                .decrypt(
                    &Nonce::from(nonce),
                    Payload {
                        msg: &ciphertext,
                        aad: &self.associated_data()?,
                    },
                )
                .map_err(|_| Error::WrongSecret)?,
        );
        let key =
            SigningKey::from_secret(SecretScalar::new(curve::scalar_from_bytes(&plaintext)?))?;
        if key.public_key() != self.public_key()? {
            return Err(Error::WrongSecret);
        }
        Ok(key)
    }

    /// The sealing key a passphrase derives for this file, checked by opening it. A platform keeps it behind
    /// biometrics and later opens with [`Secret::Key`] without running Argon2id.
    ///
    /// # Errors
    /// [`Error::WrongSecretKind`] for a platform-sealed key, or as [`Self::open`].
    pub fn sealing_key(&self, passphrase: &str) -> Result<SealingKey, Error> {
        let Kdf::Argon2id {
            salt,
            memory_kib,
            passes,
            lanes,
        } = &self.kdf
        else {
            return Err(Error::WrongSecretKind {
                expected: "platform key",
            });
        };
        let params = Argon2Params {
            memory_kib: *memory_kib,
            passes: *passes,
            lanes: *lanes,
        };
        let key = params.derive(passphrase, &bytes::<SALT_BYTES>(salt, "salt")?)?;
        self.open(&Secret::Key(&key))?;
        Ok(key)
    }

    /// The public key, readable without the secret.
    ///
    /// # Errors
    /// [`Error::Malformed`] or [`Error::Crypto`] for an invalid point.
    pub fn public_key(&self) -> Result<Point, Error> {
        let x = curve::base_from_bytes(&bytes::<32>(&self.public_key.x, "publicKey")?)?;
        let y = curve::base_from_bytes(&bytes::<32>(&self.public_key.y, "publicKey")?)?;
        Ok(Point::from_xy(x, y)?)
    }

    /// The JSON document to store.
    ///
    /// # Errors
    /// [`Error::Malformed`] if serialization fails (not expected).
    pub fn to_json(&self) -> Result<String, Error> {
        serde_json::to_string_pretty(self).map_err(|_| Error::Malformed("json"))
    }

    /// Parses a stored document and checks its version and parameters (the secret is checked on open).
    ///
    /// # Errors
    /// [`Error::Malformed`], [`Error::UnsupportedVersion`] or [`Error::KdfOutOfRange`].
    pub fn from_json(text: &str) -> Result<Self, Error> {
        if text.len() > MAX_JSON_BYTES {
            return Err(Error::Malformed("too large"));
        }
        let sealed: Self = serde_json::from_str(text).map_err(|_| Error::Malformed("json"))?;
        sealed.check()?;
        Ok(sealed)
    }

    fn check(&self) -> Result<(), Error> {
        if self.v != SEALED_VERSION {
            return Err(Error::UnsupportedVersion(self.v.clone()));
        }
        if let Kdf::Argon2id {
            memory_kib,
            passes,
            lanes,
            ..
        } = self.kdf
        {
            Argon2Params {
                memory_kib,
                passes,
                lanes,
            }
            .check()?;
        }
        self.public_key()?;
        Ok(())
    }

    /// `version ‖ kdf ‖ public key`.
    fn associated_data(&self) -> Result<Vec<u8>, Error> {
        self.check()?;
        let mut aad = Vec::with_capacity(128);
        aad.extend_from_slice(SEALED_VERSION.as_bytes());
        match &self.kdf {
            Kdf::Argon2id {
                salt,
                memory_kib,
                passes,
                lanes,
            } => {
                aad.extend_from_slice(b"argon2id");
                aad.extend_from_slice(&bytes::<SALT_BYTES>(salt, "salt")?);
                for value in [memory_kib, passes, lanes] {
                    aad.extend_from_slice(&value.to_be_bytes());
                }
            }
            Kdf::External => aad.extend_from_slice(b"external"),
        }
        let public = self.public_key()?;
        aad.extend_from_slice(&public.to_bytes());
        Ok(aad)
    }
}

fn argon2id(salt: &[u8], params: Argon2Params) -> Kdf {
    Kdf::Argon2id {
        salt: hex::encode(salt),
        memory_kib: params.memory_kib,
        passes: params.passes,
        lanes: params.lanes,
    }
}

fn cipher(key: &SealingKey) -> ChaCha20Poly1305 {
    ChaCha20Poly1305::new(&Key::from(*key.0))
}

fn hex32(bytes: &[u8; 32]) -> String {
    format!("0x{}", hex::encode(bytes))
}

/// Exactly `N` bytes of lowercase hex, with an optional `0x`.
fn bytes<const N: usize>(text: &str, field: &'static str) -> Result<[u8; N], Error> {
    let digits = text.strip_prefix("0x").unwrap_or(text);
    if digits.len() != 2 * N || digits.bytes().any(|b| b.is_ascii_uppercase()) {
        return Err(Error::Malformed(field));
    }
    let mut out = [0u8; N];
    hex::decode_to_slice(digits, &mut out).map_err(|_| Error::Malformed(field))?;
    Ok(out)
}
