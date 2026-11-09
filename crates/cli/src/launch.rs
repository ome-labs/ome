//! Finding, starting and stopping the profile's daemon (`omed`). It runs detached in its own process group, so a
//! Ctrl-C in this terminal leaves it running, and writes its output to `daemon.log` in the profile.

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

use ome_api::{
    Empty,
    daemon::{DaemonInfo, DaemonInfoGet, DaemonStop},
};
use ome_client::{DaemonClient, Home, Profile};

use crate::{
    context::Ctx,
    error::{Error, Exit, Result},
    ui::steps::Step,
};

/// How long a start waits for the daemon to answer (opening the PXE can take a while).
const START_TIMEOUT: Duration = Duration::from_secs(90);
const STOP_TIMEOUT: Duration = Duration::from_secs(30);

/// The profile's log.
pub(crate) fn log_path(profile: &Profile) -> PathBuf {
    profile.dir().join("daemon.log")
}

/// The local keeper's log.
pub(crate) fn keeper_log_path(profile: &Profile) -> PathBuf {
    profile.dir().join("keeper.log")
}

/// Whether the profile was set up (`ome init` ran).
pub(crate) fn profile_exists(profile: &Profile) -> bool {
    profile.dir().join("profile.json").is_file()
}

/// The repository root, for development builds (`crates/cli` → `../..`).
fn repo_root() -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    root.canonicalize().unwrap_or(root)
}

/// Where `omed` is: `OME_DAEMON`, then `<home>/bin/omed`, then the repository's `omed.ts`.
pub(crate) fn omed_path(home: &Home) -> Result<PathBuf> {
    let configured = std::env::var_os("OME_DAEMON")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from);
    let installed = home.root().join("bin").join("omed");
    let repo = repo_root().join("packages/daemon/src/omed.ts");
    configured
        .or_else(|| installed.is_file().then_some(installed.clone()))
        .or_else(|| repo.is_file().then_some(repo))
        .ok_or_else(|| {
            Error::new(Exit::Daemon, "The daemon (omed) is not installed.").hint(format!(
                "Install it with `pnpm cli:install` (into {}), or set OME_DAEMON.",
                installed.display()
            ))
        })
}

/// Where `ome-keeper` is, as the daemon looks for it: `OME_KEEPER_BIN`, `<home>/bin/ome-keeper`, the repository's
/// build.
pub(crate) fn keeper_path(home: &Home) -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("OME_KEEPER_BIN").filter(|v| !v.is_empty()) {
        return Some(PathBuf::from(path));
    }
    let target = repo_root().join("crates/target");
    [
        home.root().join("bin/ome-keeper"),
        target.join("debug/ome-keeper"),
        target.join("release/ome-keeper"),
    ]
    .into_iter()
    .find(|p| p.is_file())
}

/// The command that runs `omed`; a `.ts` or `.js` path runs with Node.
pub(crate) fn omed(home: &Home) -> Result<Command> {
    let path = omed_path(home)?;
    let script = path
        .extension()
        .is_some_and(|ext| ext == "ts" || ext == "js");
    Ok(if script {
        let mut command = Command::new("node");
        command.arg(path);
        command
    } else {
        Command::new(path)
    })
}

/// `daemon.info` from the profile's running daemon, if one answers.
pub(crate) async fn running(profile: &Profile) -> Option<DaemonInfo> {
    let client = DaemonClient::discover(profile).await.ok()?;
    client.call::<DaemonInfoGet>(&Empty {}).await.ok()
}

/// Starts the daemon detached and waits until it answers.
pub(crate) async fn start(ctx: &Ctx) -> Result<DaemonInfo> {
    let profile = &ctx.profile;
    if let Some(info) = running(profile).await {
        return Ok(info);
    }
    let step = Step::start(ctx, "Starting the daemon");
    fs::create_dir_all(profile.dir())?;
    let log = open_log(&log_path(profile))?;
    let mut command = omed(&ctx.home)?;
    command
        .arg("--profile")
        .arg(profile.name())
        .arg("--home")
        .arg(ctx.home.root())
        .stdin(Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log);
    detach(&mut command);
    let mut child = command.spawn().map_err(|e| {
        Error::new(Exit::Daemon, "Could not run the daemon.")
            .hint("Install Node 24 and check it is on PATH: `ome doctor`.")
            .cause(e)
    })?;
    let deadline = Instant::now() + START_TIMEOUT;
    loop {
        if let Some(info) = running(profile).await {
            step.done(&format!("Daemon started · pid {}", info.pid));
            return Ok(info);
        }
        if let Some(status) = child.try_wait()? {
            step.fail("The daemon exited while starting");
            return Err(Error::new(
                Exit::Daemon,
                format!("The daemon exited while starting ({status})."),
            )
            .cause(tail(&log_path(profile), 8))
            .hint(format!("Read its log: `{}`.", ctx.ome("daemon logs"))));
        }
        if Instant::now() > deadline {
            step.fail("The daemon did not answer");
            return Err(Error::new(
                Exit::Daemon,
                format!(
                    "The daemon did not answer within {}s (pid {}).",
                    START_TIMEOUT.as_secs(),
                    child.id()
                ),
            )
            .hint(format!("Read its log: `{}`.", ctx.ome("daemon logs"))));
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

/// Runs the daemon in this terminal until it exits.
pub(crate) fn foreground(ctx: &Ctx) -> Result<()> {
    let status = omed(&ctx.home)?
        .arg("--profile")
        .arg(ctx.profile.name())
        .arg("--home")
        .arg(ctx.home.root())
        .status()
        .map_err(|e| Error::new(Exit::Daemon, "Could not run the daemon.").cause(e))?;
    if status.success() {
        Ok(())
    } else {
        Err(Error::new(
            Exit::Daemon,
            format!("The daemon exited ({status})."),
        ))
    }
}

/// Stops the daemon and waits until it stops answering; `None` when none was running.
pub(crate) async fn stop(profile: &Profile) -> Result<Option<u32>> {
    let Ok(client) = DaemonClient::discover(profile).await else {
        return Ok(None);
    };
    let pid = client
        .call::<DaemonInfoGet>(&Empty {})
        .await
        .map(|info| info.pid)
        .ok();
    client.call::<DaemonStop>(&Empty {}).await?;
    drop(client);
    let deadline = Instant::now() + STOP_TIMEOUT;
    while Instant::now() < deadline {
        if running(profile).await.is_none() {
            return Ok(pid.or(Some(0)));
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    Err(Error::failed(format!(
        "The daemon did not stop within {}s.",
        STOP_TIMEOUT.as_secs()
    )))
}

fn open_log(path: &Path) -> Result<fs::File> {
    let mut options = fs::OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    Ok(options.open(path)?)
}

#[cfg(unix)]
fn detach(command: &mut Command) {
    use std::os::unix::process::CommandExt;
    command.process_group(0);
}

#[cfg(not(unix))]
fn detach(_: &mut Command) {}

/// The last `lines` lines of a log.
pub(crate) fn tail(path: &Path, lines: usize) -> String {
    let Ok(text) = fs::read_to_string(path) else {
        return String::new();
    };
    let all: Vec<&str> = text.lines().collect();
    all[all.len().saturating_sub(lines)..].join("\n")
}
