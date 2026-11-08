//! What the app provides: storage for the core's files and the two sealing keys.

use std::sync::Arc;

/// Which sealing key the core asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum KeyPurpose {
    /// Seals the device's identity and transport keys. It may open without the user, so the app can check its
    /// mailbox in the background.
    Transport,
    /// Seals the signing key. It must require the user: biometrics or the device passcode.
    Signing,
}

/// Why the app's store failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, uniffi::Error)]
pub enum StoreError {
    /// The user cancelled, or the platform refused to release a key.
    #[error("{reason}")]
    Denied {
        /// What the platform said.
        reason: String,
    },
    /// Reading or writing failed.
    #[error("{reason}")]
    Failed {
        /// What failed.
        reason: String,
    },
}

impl From<uniffi::UnexpectedUniFFICallbackError> for StoreError {
    fn from(error: uniffi::UnexpectedUniFFICallbackError) -> Self {
        Self::Failed {
            reason: error.reason,
        }
    }
}

/// Implemented by the app. The core keeps a few small files here (public keys, the sealed keys, the pairing, received
/// reviews and the policy) and asks for the 32-byte sealing keys, which the app holds in the Keychain (iOS) or behind
/// the Android Keystore and creates on first use.
#[uniffi::export(with_foreign)]
pub trait DeviceStore: Send + Sync {
    /// The file `name`, if it exists.
    ///
    /// # Errors
    /// [`StoreError::Failed`].
    fn read(&self, name: String) -> Result<Option<Vec<u8>>, StoreError>;

    /// Replaces the file `name`.
    ///
    /// # Errors
    /// [`StoreError::Failed`].
    fn write(&self, name: String, bytes: Vec<u8>) -> Result<(), StoreError>;

    /// Deletes the file `name`; deleting a missing file is not an error.
    ///
    /// # Errors
    /// [`StoreError::Failed`].
    fn remove(&self, name: String) -> Result<(), StoreError>;

    /// The 32-byte sealing key for `purpose`, created on first use and the same ever after. For
    /// [`KeyPurpose::Signing`] the app must authenticate the user first.
    ///
    /// # Errors
    /// [`StoreError::Denied`] when the user cancels or the platform refuses.
    fn sealing_key(&self, purpose: KeyPurpose) -> Result<Vec<u8>, StoreError>;
}

/// A shared store.
pub(crate) type Store = Arc<dyn DeviceStore>;

/// A 32-byte sealing key from the store.
pub(crate) fn sealing_key(
    store: &Store,
    purpose: KeyPurpose,
) -> crate::error::Result<zeroize::Zeroizing<[u8; 32]>> {
    let bytes = zeroize::Zeroizing::new(store.sealing_key(purpose)?);
    let mut key = zeroize::Zeroizing::new([0u8; 32]);
    if bytes.len() != 32 {
        return Err(crate::FfiError::storage("a sealing key must be 32 bytes"));
    }
    key.copy_from_slice(&bytes);
    Ok(key)
}

/// Reads and parses a JSON file.
pub(crate) fn read_json<T: serde::de::DeserializeOwned>(
    store: &Store,
    name: &str,
) -> crate::error::Result<Option<T>> {
    store
        .read(name.to_owned())?
        .map(|bytes| {
            serde_json::from_slice(&bytes)
                .map_err(|e| crate::FfiError::storage(format!("{name} is damaged: {e}")))
        })
        .transpose()
}

/// Writes a JSON file.
pub(crate) fn write_json<T: serde::Serialize>(
    store: &Store,
    name: &str,
    value: &T,
) -> crate::error::Result<()> {
    let bytes = serde_json::to_vec(value).map_err(|e| crate::FfiError::storage(e.to_string()))?;
    Ok(store.write(name.to_owned(), bytes)?)
}
