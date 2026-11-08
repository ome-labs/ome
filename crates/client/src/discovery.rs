//! Where a member's daemon listens. The daemon writes two files into the Ome home when it starts: `daemon.addr`, its
//! socket path (relative paths are relative to the home), and `daemon.token`, the per-launch session token, readable
//! only by the owner.

use std::{
    ffi::OsString,
    fmt,
    path::{Path, PathBuf},
};

use crate::{ClientError, Result};

/// The daemon's socket address file.
pub const ADDR_FILE: &str = "daemon.addr";
/// The daemon's token file.
pub const TOKEN_FILE: &str = "daemon.token";

/// The Ome home: `OME_HOME`, or `~/.ome`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Home {
    root: PathBuf,
}

/// How to reach a daemon.
#[derive(Clone, PartialEq, Eq)]
pub struct Endpoint {
    /// Socket or pipe.
    pub socket: PathBuf,
    /// Session token.
    pub token: String,
}

impl fmt::Debug for Endpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Endpoint")
            .field("socket", &self.socket)
            .field("token", &"<redacted>")
            .finish()
    }
}

impl Home {
    /// The home at `root`.
    #[must_use]
    pub fn at(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// The home this process should use.
    ///
    /// # Errors
    /// Neither `OME_HOME` nor the user's home directory is set.
    pub fn from_env() -> Result<Self> {
        let user = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
        Self::resolve(std::env::var_os("OME_HOME"), std::env::var_os(user))
    }

    /// The home from an `OME_HOME` value, falling back to `.ome` in the user's home directory.
    ///
    /// # Errors
    /// Neither is set.
    pub fn resolve(ome_home: Option<OsString>, user_home: Option<OsString>) -> Result<Self> {
        match (
            ome_home.filter(|v| !v.is_empty()),
            user_home.filter(|v| !v.is_empty()),
        ) {
            (Some(root), _) => Ok(Self::at(root)),
            (None, Some(user)) => Ok(Self::at(PathBuf::from(user).join(".ome"))),
            (None, None) => Err(ClientError::Discovery(
                "set OME_HOME: there is no home directory".into(),
            )),
        }
    }

    /// The directory.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Where the running daemon listens, and its token.
    ///
    /// # Errors
    /// The daemon is not running (no files), or the token file is readable by others.
    pub fn daemon(&self) -> Result<Endpoint> {
        let addr = self.read(ADDR_FILE)?;
        let token_path = self.root.join(TOKEN_FILE);
        check_private(&token_path)?;
        let token = self.read(TOKEN_FILE)?;
        if token.is_empty() {
            return Err(ClientError::Discovery(format!(
                "{} is empty",
                token_path.display()
            )));
        }
        Ok(Endpoint {
            socket: self.root.join(addr),
            token,
        })
    }

    fn read(&self, name: &str) -> Result<String> {
        let path = self.root.join(name);
        std::fs::read_to_string(&path)
            .map(|text| text.trim().to_owned())
            .map_err(|e| match e.kind() {
                std::io::ErrorKind::NotFound => ClientError::Discovery(format!(
                    "no daemon is running for {} (start it with `omed`)",
                    self.root.display()
                )),
                _ => ClientError::Discovery(format!("cannot read {}: {e}", path.display())),
            })
    }
}

/// Refuses a token file that anyone but its owner can read: another user could take over the session.
#[cfg(unix)]
fn check_private(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    match std::fs::metadata(path) {
        Ok(metadata) if metadata.permissions().mode() & 0o077 != 0 => {
            Err(ClientError::Discovery(format!(
                "{} is readable by other users; it must be 0600",
                path.display()
            )))
        }
        _ => Ok(()),
    }
}

#[cfg(not(unix))]
#[allow(clippy::unnecessary_wraps, reason = "the Unix check can fail")]
const fn check_private(_: &Path) -> Result<()> {
    Ok(())
}
