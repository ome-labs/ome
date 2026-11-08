//! Sealing keys: derived from a passphrase with Argon2id, or held by the platform.

use argon2::{Algorithm, Argon2, Params, Version};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::Error;

/// Argon2id parameters, stored in the sealed key so they can be raised later without breaking old keys.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Argon2Params {
    /// Memory in KiB.
    pub memory_kib: u32,
    /// Passes over memory.
    pub passes: u32,
    /// Parallel lanes.
    pub lanes: u32,
}

impl Argon2Params {
    /// What new keys use: 64 MiB, 3 passes, 1 lane. Also the minimum a sealed key may ask for.
    pub const DEFAULT: Self = Self {
        memory_kib: 64 * 1024,
        passes: 3,
        lanes: 1,
    };
    /// The most a sealed key may ask for, so a crafted file cannot make a device hang or run out of memory.
    pub const MAX: Self = Self {
        memory_kib: 1024 * 1024,
        passes: 16,
        lanes: 8,
    };

    /// Refuses parameters below [`Self::DEFAULT`] or above [`Self::MAX`].
    ///
    /// # Errors
    /// [`Error::KdfOutOfRange`] naming the parameter.
    pub fn check(&self) -> Result<(), Error> {
        let within = |value, min, max, name| {
            if (min..=max).contains(&value) {
                Ok(())
            } else {
                Err(Error::KdfOutOfRange(name))
            }
        };
        within(
            self.memory_kib,
            Self::DEFAULT.memory_kib,
            Self::MAX.memory_kib,
            "memory",
        )?;
        within(
            self.passes,
            Self::DEFAULT.passes,
            Self::MAX.passes,
            "passes",
        )?;
        within(self.lanes, Self::DEFAULT.lanes, Self::MAX.lanes, "lanes")
    }

    /// Derives the 32-byte sealing key for `passphrase` and `salt`.
    pub(crate) fn derive(&self, passphrase: &str, salt: &[u8]) -> Result<SealingKey, Error> {
        self.check()?;
        let params = Params::new(self.memory_kib, self.passes, self.lanes, Some(32))
            .map_err(|_| Error::Kdf)?;
        let mut key = Zeroizing::new([0u8; 32]);
        Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
            .hash_password_into(passphrase.as_bytes(), salt, key.as_mut_slice())
            .map_err(|_| Error::Kdf)?;
        Ok(SealingKey(key))
    }
}

/// The 32-byte key a signing key is sealed under. Zeroized on drop; not `Clone` or `Debug`.
///
/// A platform can keep it behind biometrics (Keychain, Android Keystore) and pass it instead of the passphrase, or seal
/// a key under a platform-generated sealing key from the start.
pub struct SealingKey(pub(crate) Zeroizing<[u8; 32]>);

impl SealingKey {
    /// A sealing key the platform holds.
    #[must_use]
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(Zeroizing::new(bytes))
    }

    /// The raw key, for the platform's secure storage.
    #[must_use]
    pub fn to_bytes(&self) -> Zeroizing<[u8; 32]> {
        Zeroizing::new(*self.0)
    }
}
