use argon2::{Algorithm, Argon2, Params, Version};
use zeroize::Zeroizing;

use crate::{Error, Result};

/// Argon2id cost for passphrase-derived master keys (64 MiB, 3 passes).
const ARGON2_MEMORY_KIB: u32 = 64 * 1024;
const ARGON2_PASSES: u32 = 3;

/// The 32-byte key that seals every secret at rest. Zeroized on drop, never printed.
pub struct MasterKey(pub(crate) Zeroizing<[u8; 32]>);

impl MasterKey {
    /// A key supplied by the supervising app, which keeps it in the OS keychain.
    #[must_use]
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(Zeroizing::new(bytes))
    }

    /// Parses a hex-encoded key.
    ///
    /// # Errors
    /// [`Error::Invalid`] unless `hex` is 32 bytes of hex.
    pub fn from_hex(hex: &str) -> Result<Self> {
        let mut bytes = Zeroizing::new([0u8; 32]);
        hex::decode_to_slice(hex.trim(), bytes.as_mut_slice())
            .map_err(|_| Error::Invalid("master key must be 32 bytes of hex".into()))?;
        Ok(Self(bytes))
    }

    /// Derives the key from a passphrase (for keepers on a member's own server) with Argon2id over a per-keeper salt.
    ///
    /// # Errors
    /// [`Error::Invalid`] for an empty passphrase.
    pub fn from_passphrase(passphrase: &[u8], salt: &[u8; 16]) -> Result<Self> {
        if passphrase.is_empty() {
            return Err(Error::Invalid("empty passphrase".into()));
        }
        let params = Params::new(ARGON2_MEMORY_KIB, ARGON2_PASSES, 1, Some(32))
            .map_err(|e| Error::Invalid(e.to_string()))?;
        let mut key = Zeroizing::new([0u8; 32]);
        Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
            .hash_password_into(passphrase, salt, key.as_mut_slice())
            .map_err(|e| Error::Invalid(e.to_string()))?;
        Ok(Self(key))
    }
}

impl core::fmt::Debug for MasterKey {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("MasterKey(<redacted>)")
    }
}
