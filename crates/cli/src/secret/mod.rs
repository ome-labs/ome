//! The secret that unseals the signer for one call: none while it is unlocked; otherwise Touch ID (macOS, in a
//! terminal), the passphrase file (`OME_PASSPHRASE_FILE`) or a prompt that never echoes. Never argv, never a flag
//! value. `--yes` skips confirmations, never authentication.

#[cfg(target_os = "macos")]
mod touch_id;

use std::path::PathBuf;

#[cfg(target_os = "macos")]
use ome_api::daemon::{PassphraseParams, SignerSealingKey};
use ome_api::{
    ErrorKind, Hex32,
    daemon::{Secret, SignerStatus},
};
use ome_client::{DaemonClient, Profile};
use zeroize::Zeroizing;

use crate::{
    context::Ctx,
    error::{Error, Result},
    ui::prompt,
};

/// Where the secret comes from.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Source {
    /// The signer is unlocked: send none.
    Unlocked,
    TouchId,
    PassphraseFile(PathBuf),
    Prompt,
}

/// What decides the source.
pub(crate) struct Situation {
    pub(crate) unlocked: bool,
    pub(crate) touch_id: bool,
    pub(crate) interactive: bool,
    pub(crate) passphrase_file: Option<PathBuf>,
}

/// Picks the source, or fails when none can work (no terminal, locked, no passphrase file).
pub(crate) fn source(s: &Situation) -> Result<Source> {
    if s.unlocked {
        return Ok(Source::Unlocked);
    }
    if s.touch_id && s.interactive && cfg!(target_os = "macos") {
        return Ok(Source::TouchId);
    }
    if let Some(file) = &s.passphrase_file {
        return Ok(Source::PassphraseFile(file.clone()));
    }
    if s.interactive {
        return Ok(Source::Prompt);
    }
    let mut error =
        Error::failed("The signer is locked, and there is no way to ask for the passphrase.")
            .hint("Set OME_PASSPHRASE_FILE, or unlock first: `ome unlock --for 15m`.");
    error.kind = Some(ErrorKind::Locked);
    Err(error)
}

fn passphrase_file() -> Option<PathBuf> {
    std::env::var_os("OME_PASSPHRASE_FILE")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

/// Reads the passphrase file, dropping one trailing newline.
fn read_file(path: &PathBuf) -> Result<Zeroizing<String>> {
    let mut text = Zeroizing::new(std::fs::read_to_string(path).map_err(|e| {
        Error::failed(format!(
            "Could not read OME_PASSPHRASE_FILE {}.",
            path.display()
        ))
        .cause(e)
    })?);
    if text.ends_with('\n') {
        text.pop();
        if text.ends_with('\r') {
            text.pop();
        }
    }
    if text.is_empty() {
        return Err(Error::failed(format!(
            "OME_PASSPHRASE_FILE {} is empty.",
            path.display()
        )));
    }
    Ok(text)
}

/// The passphrase: from the file, else asked for.
#[cfg(target_os = "macos")]
pub(crate) fn passphrase(ctx: &Ctx) -> Result<Zeroizing<String>> {
    match passphrase_file() {
        Some(file) => read_file(&file),
        None if ctx.interactive => prompt::password("Passphrase"),
        None => Err(Error::usage("The passphrase is needed.")
            .hint("Set OME_PASSPHRASE_FILE to a file that holds it.")),
    }
}

/// A new passphrase: from the file, else typed twice. At least 8 characters.
pub(crate) fn new_passphrase(ctx: &Ctx) -> Result<Zeroizing<String>> {
    let passphrase = match passphrase_file() {
        Some(file) => read_file(&file)?,
        None if ctx.interactive => loop {
            let first = prompt::password("Passphrase (seals your signing key on this device)")?;
            if first.chars().count() < 8 {
                ctx.warn("Use at least 8 characters.");
                continue;
            }
            let second = prompt::password("The same again")?;
            if first == second {
                break first;
            }
            ctx.warn("They did not match; once more.");
        },
        None => {
            return Err(Error::usage("A passphrase is needed.")
                .hint("Set OME_PASSPHRASE_FILE to a file that holds it."));
        }
    };
    if passphrase.chars().count() < 8 {
        return Err(Error::usage("Use a passphrase of at least 8 characters."));
    }
    Ok(passphrase)
}

/// The passphrase that replaces the current one: from `OME_NEW_PASSPHRASE_FILE`, else typed twice. At least 8
/// characters.
pub(crate) fn replacement_passphrase(ctx: &Ctx) -> Result<Zeroizing<String>> {
    let passphrase = match std::env::var_os("OME_NEW_PASSPHRASE_FILE") {
        Some(file) => read_file(&PathBuf::from(file))?,
        None if ctx.interactive => loop {
            let first = prompt::password("New passphrase")?;
            if first.chars().count() < 8 {
                ctx.warn("Use at least 8 characters.");
                continue;
            }
            let second = prompt::password("The same again")?;
            if first == second {
                break first;
            }
            ctx.warn("They did not match; once more.");
        },
        None => {
            return Err(Error::usage("The new passphrase is needed.")
                .hint("Set OME_NEW_PASSPHRASE_FILE to a file that holds it."));
        }
    };
    if passphrase.chars().count() < 8 {
        return Err(Error::usage("Use a passphrase of at least 8 characters."));
    }
    Ok(passphrase)
}

/// The secret for one signature, or `None` while the signer is unlocked.
pub(crate) async fn for_signing(ctx: &Ctx, client: &DaemonClient) -> Result<Option<Secret>> {
    let status = client
        .call::<ome_api::daemon::SignerStatusGet>(&ome_api::Empty {})
        .await?;
    for_status(ctx, status, touch_id_enabled(&ctx.profile))
}

/// The secret for `signer.unlock`, which always needs one.
pub(crate) fn for_unlock(ctx: &Ctx, touch_id: bool) -> Result<Secret> {
    let locked = SignerStatus {
        locked: true,
        unlocked_until: None,
    };
    for_status(ctx, locked, touch_id || touch_id_enabled(&ctx.profile))?
        .ok_or_else(|| Error::failed("No secret to unlock with."))
}

fn for_status(ctx: &Ctx, status: SignerStatus, touch_id: bool) -> Result<Option<Secret>> {
    let situation = Situation {
        unlocked: !status.locked,
        touch_id,
        interactive: ctx.interactive,
        passphrase_file: passphrase_file(),
    };
    let to_secret = |p: Zeroizing<String>| Secret::Passphrase {
        passphrase: p.as_str().to_owned(),
    };
    match source(&situation)? {
        Source::Unlocked => Ok(None),
        Source::PassphraseFile(file) => read_file(&file).map(|p| Some(to_secret(p))),
        Source::Prompt => prompt::password("Passphrase").map(|p| Some(to_secret(p))),
        Source::TouchId => match read_touch_id(&ctx.profile) {
            Ok(key) => Ok(Some(Secret::Key { key })),
            Err(error) => {
                ctx.warn(&format!("{error} Using the passphrase instead."));
                prompt::password("Passphrase").map(|p| Some(to_secret(p)))
            }
        },
    }
}

/// The marker that says this profile unlocks with Touch ID; checking it never prompts.
fn marker(profile: &Profile) -> PathBuf {
    profile.dir().join("touch-id")
}

pub(crate) fn touch_id_enabled(profile: &Profile) -> bool {
    marker(profile).exists()
}

/// The keychain account of a profile: its directory, unique per profile and home.
#[cfg(target_os = "macos")]
fn account(profile: &Profile) -> String {
    profile.dir().display().to_string()
}

#[cfg(target_os = "macos")]
fn read_touch_id(profile: &Profile) -> Result<Hex32> {
    let key = touch_id::read(
        &account(profile),
        &format!("sign as Ome profile {}", profile.name()),
    )?;
    key.parse().map_err(|_| {
        Error::failed("The keychain item is corrupt; run `ome signer touch-id on` again.")
    })
}

#[cfg(not(target_os = "macos"))]
fn read_touch_id(_: &Profile) -> Result<Hex32> {
    Err(Error::failed("Touch ID needs macOS."))
}

/// Touch ID on: one Touch ID check, the passphrase, `signer.sealingKey`, then the key into the keychain.
#[cfg(target_os = "macos")]
pub(crate) async fn enable_touch_id(
    ctx: &Ctx,
    client: &DaemonClient,
    known: Option<&str>,
) -> Result<()> {
    if !ctx.interactive {
        return Err(Error::usage("Touch ID needs a terminal."));
    }
    touch_id::authenticate("turn on Touch ID for Ome")?;
    let passphrase = match known {
        Some(p) => Zeroizing::new(p.to_owned()),
        None => passphrase(ctx)?,
    };
    let sealing = client
        .call::<SignerSealingKey>(&PassphraseParams {
            passphrase: passphrase.as_str().to_owned(),
        })
        .await?;
    let key = Zeroizing::new(sealing.key.to_string());
    touch_id::store(&account(&ctx.profile), &key)?;
    write_private(&marker(&ctx.profile), b"keychain\n")
}

#[cfg(not(target_os = "macos"))]
#[expect(
    clippy::unused_async,
    reason = "the same signature as the macOS version"
)]
pub(crate) async fn enable_touch_id(_: &Ctx, _: &DaemonClient, _: Option<&str>) -> Result<()> {
    Err(Error::failed("Touch ID needs macOS."))
}

/// Touch ID off: removes the keychain item and the marker.
pub(crate) fn disable_touch_id(profile: &Profile) -> Result<()> {
    #[cfg(target_os = "macos")]
    touch_id::remove(&account(profile))?;
    match std::fs::remove_file(marker(profile)) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
        _ => Ok(()),
    }
}

/// Writes a file only its owner can read.
#[cfg(target_os = "macos")]
pub(crate) fn write_private(path: &std::path::Path, bytes: &[u8]) -> Result<()> {
    let mut options = std::fs::OpenOptions::new();
    options.create(true).write(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    std::io::Write::write_all(&mut options.open(path)?, bytes)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn situation() -> Situation {
        Situation {
            unlocked: false,
            touch_id: false,
            interactive: true,
            passphrase_file: None,
        }
    }

    #[test]
    fn an_unlocked_signer_needs_no_secret() {
        let s = Situation {
            unlocked: true,
            ..situation()
        };
        assert_eq!(source(&s).unwrap(), Source::Unlocked);
    }

    #[test]
    fn a_locked_signer_always_authenticates() {
        assert_eq!(source(&situation()).unwrap(), Source::Prompt);
        let headless = Situation {
            interactive: false,
            passphrase_file: Some("/p".into()),
            ..situation()
        };
        assert_eq!(
            source(&headless).unwrap(),
            Source::PassphraseFile("/p".into())
        );
    }

    #[test]
    fn without_a_terminal_or_file_it_fails_instead_of_signing() {
        let s = Situation {
            interactive: false,
            touch_id: true,
            ..situation()
        };
        let error = source(&s).err().unwrap();
        assert_eq!(error.exit.code(), 1);
        assert_eq!(error.to_json()["error"]["code"], "locked");
    }

    #[test]
    fn touch_id_only_in_a_terminal_on_macos() {
        let s = Situation {
            touch_id: true,
            ..situation()
        };
        let expected = if cfg!(target_os = "macos") {
            Source::TouchId
        } else {
            Source::Prompt
        };
        assert_eq!(source(&s).unwrap(), expected);
    }

    #[test]
    fn the_passphrase_file_drops_one_newline() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("p");
        std::fs::write(&path, "correct horse\n").unwrap();
        assert_eq!(read_file(&path).unwrap().as_str(), "correct horse");
        std::fs::write(&path, "\n").unwrap();
        assert!(read_file(&path).is_err());
    }
}
