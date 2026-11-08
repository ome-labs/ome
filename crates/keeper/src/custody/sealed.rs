//! Secrets at rest: one file per secret under `<data>/secrets/`, sealed with XChaCha20-Poly1305 under the master key.
//! The secret's name is authenticated as associated data, so files cannot be swapped. Writes are atomic (temp file,
//! fsync, rename, fsync of the directory); destruction overwrites the file before unlinking it.

use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

use chacha20poly1305::{
    KeyInit, XChaCha20Poly1305, XNonce,
    aead::{Aead, Payload},
};
use zeroize::Zeroizing;

use super::MasterKey;
use crate::{Error, Result, random::random_bytes};

const VERSION: u8 = 0;
const AAD_DOMAIN: &[u8] = b"ome.keeper.sealed.v0:";
const NONCE_LEN: usize = 24;
/// Sealed under every master key so a wrong key is detected before any real secret is touched.
const CHECK: &str = "check";
const CHECK_PLAINTEXT: &[u8] = b"ome keeper master key check";

/// Sealed secret files of one keeper.
pub struct SecretStore {
    dir: PathBuf,
    cipher: XChaCha20Poly1305,
}

impl SecretStore {
    /// Opens (creating if needed) the store and checks the master key against it.
    ///
    /// # Errors
    /// [`Error::Unseal`] for a wrong master key, I/O errors otherwise.
    pub fn open(dir: impl Into<PathBuf>, key: &MasterKey) -> Result<Self> {
        let dir = dir.into();
        create_private_dir(&dir)?;
        let store = Self {
            dir,
            cipher: XChaCha20Poly1305::new(&(*key.0).into()),
        };
        if store.exists(CHECK) {
            if store.get(CHECK)?.as_slice() != CHECK_PLAINTEXT {
                return Err(Error::Unseal(CHECK.into()));
            }
        } else {
            store.put(CHECK, CHECK_PLAINTEXT)?;
        }
        Ok(store)
    }

    /// Whether a secret exists.
    #[must_use]
    pub fn exists(&self, name: &str) -> bool {
        self.path(name).is_file()
    }

    /// Seals and atomically writes `plaintext` under `name`.
    ///
    /// # Errors
    /// [`Error::Invalid`] for a bad name, I/O errors.
    pub fn put(&self, name: &str, plaintext: &[u8]) -> Result<()> {
        let nonce: [u8; NONCE_LEN] = random_bytes();
        let ciphertext = self
            .cipher
            .encrypt(
                &XNonce::from(nonce),
                Payload {
                    msg: plaintext,
                    aad: &aad(name),
                },
            )
            .map_err(|_| Error::Storage("seal failed".into()))?;
        let mut file = Vec::with_capacity(1 + NONCE_LEN + ciphertext.len());
        file.push(VERSION);
        file.extend_from_slice(&nonce);
        file.extend_from_slice(&ciphertext);
        write_atomic(&self.path(name), &file)
    }

    /// Reads and unseals `name`.
    ///
    /// # Errors
    /// [`Error::MissingSecret`], [`Error::Unseal`] for tampering or a wrong key.
    pub fn get(&self, name: &str) -> Result<Zeroizing<Vec<u8>>> {
        let path = self.path(name);
        if !path.is_file() {
            return Err(Error::MissingSecret(name.into()));
        }
        let file = fs::read(path)?;
        let unseal = || Error::Unseal(name.into());
        let (version, rest) = file.split_first().ok_or_else(unseal)?;
        if *version != VERSION || rest.len() < NONCE_LEN {
            return Err(unseal());
        }
        let (nonce, ciphertext) = rest.split_at(NONCE_LEN);
        let nonce: [u8; NONCE_LEN] = nonce.try_into().map_err(|_| unseal())?;
        let plaintext = self
            .cipher
            .decrypt(
                &XNonce::from(nonce),
                Payload {
                    msg: ciphertext,
                    aad: &aad(name),
                },
            )
            .map_err(|_| unseal())?;
        Ok(Zeroizing::new(plaintext))
    }

    /// Atomically renames a secret (e.g. a pending share becoming active).
    ///
    /// # Errors
    /// [`Error::MissingSecret`], I/O errors.
    pub fn rename(&self, from: &str, to: &str) -> Result<()> {
        // Re-seal under the new name: the name is bound into the ciphertext.
        let plaintext = self.get(from)?;
        self.put(to, &plaintext)?;
        self.destroy(from)
    }

    /// Overwrites and deletes a secret. Missing secrets are ignored.
    ///
    /// # Errors
    /// I/O errors.
    pub fn destroy(&self, name: &str) -> Result<()> {
        let path = self.path(name);
        if !path.is_file() {
            return Ok(());
        }
        let len = usize::try_from(fs::metadata(&path)?.len()).unwrap_or(0);
        let mut file = OpenOptions::new().write(true).open(&path)?;
        file.write_all(&vec![0u8; len])?;
        file.sync_all()?;
        drop(file);
        fs::remove_file(&path)?;
        sync_dir(&self.dir)
    }

    /// Names of stored secrets starting with `prefix`.
    ///
    /// # Errors
    /// I/O errors.
    pub fn list(&self, prefix: &str) -> Result<Vec<String>> {
        let mut names = Vec::new();
        for entry in fs::read_dir(&self.dir)? {
            let name = entry?.file_name().to_string_lossy().into_owned();
            if let Some(stem) = name.strip_suffix(".sealed")
                && stem.starts_with(prefix)
            {
                names.push(stem.to_owned());
            }
        }
        names.sort();
        Ok(names)
    }

    fn path(&self, name: &str) -> PathBuf {
        debug_assert!(valid_name(name), "invalid secret name {name}");
        self.dir.join(format!("{name}.sealed"))
    }
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

fn aad(name: &str) -> Vec<u8> {
    [AAD_DOMAIN, name.as_bytes()].concat()
}

/// Writes a non-secret file (salt, RPC token, public info) atomically with owner-only permissions.
///
/// # Errors
/// I/O errors.
pub fn write_private(path: &Path, contents: &[u8]) -> Result<()> {
    write_atomic(path, contents)
}

/// Creates `dir` readable only by the owner.
///
/// # Errors
/// I/O errors.
pub fn create_private_dir(dir: &Path) -> Result<()> {
    fs::create_dir_all(dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

/// Writes `contents` to `path` atomically with owner-only permissions.
pub(crate) fn write_atomic(path: &Path, contents: &[u8]) -> Result<()> {
    let tmp = path.with_extension("tmp");
    {
        let mut options = OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&tmp)?;
        file.write_all(contents)?;
        file.sync_all()?;
    }
    fs::rename(&tmp, path)?;
    if let Some(parent) = path.parent() {
        sync_dir(parent)?;
    }
    Ok(())
}

fn sync_dir(dir: &Path) -> Result<()> {
    #[cfg(unix)]
    File::open(dir)?.sync_all()?;
    #[cfg(not(unix))]
    let _ = dir;
    Ok(())
}
