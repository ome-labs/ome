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

/// A step that takes one phone file from a schema to the next.
type Migration = fn(serde_json::Value) -> serde_json::Value;

/// Each file's migrations, oldest first; its current schema is their count. A release that changes a file's shape
/// gives that file its own list here (matching on `name`), and phones upgrade the file the next time they read it.
fn migrations(_name: &str) -> &'static [Migration] {
    // 1: files gain their schema envelope.
    &[std::convert::identity]
}

/// How a file is kept: its schema beside the value. A file without it was written before schemas, which is schema 0.
#[derive(serde::Serialize, serde::Deserialize)]
struct Versioned {
    schema: usize,
    value: serde_json::Value,
}

/// Reads and parses a JSON file at its current schema: an older file is migrated and written back, and one a newer
/// release wrote is refused.
pub(crate) fn read_json<T: serde::de::DeserializeOwned>(
    store: &Store,
    name: &str,
) -> crate::error::Result<Option<T>> {
    let Some(bytes) = store.read(name.to_owned())? else {
        return Ok(None);
    };
    let damaged =
        |e: serde_json::Error| crate::FfiError::storage(format!("{name} is damaged: {e}"));
    let raw: serde_json::Value = serde_json::from_slice(&bytes).map_err(damaged)?;
    let (schema, mut value) = match serde_json::from_value::<Versioned>(raw.clone()) {
        Ok(versioned) => (versioned.schema, versioned.value),
        Err(_) => (0, raw),
    };
    let steps = migrations(name);
    if schema > steps.len() {
        return Err(crate::FfiError::storage(format!(
            "{name} was written by a newer Ome (schema {schema}; this release knows {}): update the app",
            steps.len()
        )));
    }
    for step in &steps[schema..] {
        value = step(value);
    }
    let parsed: T = serde_json::from_value(value.clone()).map_err(damaged)?;
    if schema < steps.len() {
        write_value(store, name, value)?;
    }
    Ok(Some(parsed))
}

/// Writes a JSON file at its current schema.
pub(crate) fn write_json<T: serde::Serialize>(
    store: &Store,
    name: &str,
    value: &T,
) -> crate::error::Result<()> {
    let value = serde_json::to_value(value).map_err(|e| crate::FfiError::storage(e.to_string()))?;
    write_value(store, name, value)
}

fn write_value(store: &Store, name: &str, value: serde_json::Value) -> crate::error::Result<()> {
    let file = Versioned {
        schema: migrations(name).len(),
        value,
    };
    let bytes = serde_json::to_vec(&file).map_err(|e| crate::FfiError::storage(e.to_string()))?;
    Ok(store.write(name.to_owned(), bytes)?)
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, sync::Mutex};

    use super::{DeviceStore, KeyPurpose, Store, StoreError, read_json, write_json};

    #[derive(Default)]
    struct Memory(Mutex<HashMap<String, Vec<u8>>>);

    impl DeviceStore for Memory {
        fn read(&self, name: String) -> Result<Option<Vec<u8>>, StoreError> {
            Ok(self.0.lock().unwrap().get(&name).cloned())
        }
        fn write(&self, name: String, bytes: Vec<u8>) -> Result<(), StoreError> {
            self.0.lock().unwrap().insert(name, bytes);
            Ok(())
        }
        fn remove(&self, name: String) -> Result<(), StoreError> {
            self.0.lock().unwrap().remove(&name);
            Ok(())
        }
        fn sealing_key(&self, _: KeyPurpose) -> Result<Vec<u8>, StoreError> {
            Ok(vec![0; 32])
        }
    }

    fn raw(store: &Store, name: &str) -> serde_json::Value {
        serde_json::from_slice(&store.read(name.into()).unwrap().unwrap()).unwrap()
    }

    #[test]
    fn a_file_from_before_schemas_is_read_and_upgraded() {
        let store: Store = std::sync::Arc::new(Memory::default());
        store
            .write("policy.json".into(), br#"{"limit":5}"#.to_vec())
            .unwrap();
        let value: serde_json::Value = read_json(&store, "policy.json").unwrap().unwrap();
        assert_eq!(value, serde_json::json!({ "limit": 5 }));
        assert_eq!(
            raw(&store, "policy.json"),
            serde_json::json!({ "schema": 1, "value": { "limit": 5 } })
        );

        write_json(&store, "policy.json", &serde_json::json!({ "limit": 6 })).unwrap();
        let value: serde_json::Value = read_json(&store, "policy.json").unwrap().unwrap();
        assert_eq!(value, serde_json::json!({ "limit": 6 }));
    }

    #[test]
    fn a_file_from_a_newer_release_is_refused() {
        let store: Store = std::sync::Arc::new(Memory::default());
        store
            .write(
                "pairing.json".into(),
                br#"{"schema":9,"value":{}}"#.to_vec(),
            )
            .unwrap();
        let error = read_json::<serde_json::Value>(&store, "pairing.json").unwrap_err();
        assert!(
            error.to_string().contains("written by a newer Ome"),
            "{error}"
        );
    }
}
