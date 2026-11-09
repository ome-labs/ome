//! Running a profile's daemon as a login service, so the member's keeper keeps answering reads while nobody has a
//! terminal open: a launchd agent on macOS, a systemd user unit on Linux. Both restart the daemon after a crash but
//! not after `ome daemon stop`.

use std::{
    fmt::Write as _,
    path::{Path, PathBuf},
    process::Command,
};

use crate::{
    context::Ctx,
    error::{Error, Result},
    launch,
};

/// Where the service definition lives, and its name.
pub(crate) struct Service {
    pub(crate) name: String,
    pub(crate) file: PathBuf,
}

/// The service of the context's profile.
pub(crate) fn of(ctx: &Ctx) -> Result<Service> {
    let user = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| Error::failed("HOME is not set."))?;
    let profile = ctx.profile.name();
    if cfg!(target_os = "macos") {
        let name = format!("network.ome.omed.{profile}");
        let file = user
            .join("Library/LaunchAgents")
            .join(format!("{name}.plist"));
        Ok(Service { name, file })
    } else if cfg!(target_os = "linux") {
        let name = format!("ome-{profile}.service");
        let file = user.join(".config/systemd/user").join(&name);
        Ok(Service { name, file })
    } else {
        Err(Error::usage(
            "Running the daemon as a service needs macOS or Linux.",
        ))
    }
}

/// The program and arguments that run the daemon, with absolute paths (services see no `PATH`).
fn program(ctx: &Ctx) -> Result<Vec<String>> {
    let omed = launch::omed_path(&ctx.home)?;
    let mut argv = Vec::new();
    if omed
        .extension()
        .is_some_and(|ext| ext == "ts" || ext == "js")
    {
        let node = which("node").ok_or_else(|| {
            Error::failed("Node is not on PATH.").hint("Install Node 24: `ome doctor`.")
        })?;
        argv.push(node.display().to_string());
    }
    argv.push(omed.display().to_string());
    argv.extend([
        "--profile".into(),
        ctx.profile.name().into(),
        "--home".into(),
        ctx.home.root().display().to_string(),
    ]);
    Ok(argv)
}

/// `name` on `PATH`, as an absolute path.
fn which(name: &str) -> Option<PathBuf> {
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|dir| dir.join(name))
        .find(|path| path.is_file())
}

/// The environment the daemon needs: its home and the keeper's binary.
fn environment(ctx: &Ctx) -> Vec<(String, String)> {
    let mut env = vec![("OME_HOME".to_owned(), ctx.home.root().display().to_string())];
    if let Some(keeper) = launch::keeper_path(&ctx.home) {
        env.push(("OME_KEEPER_BIN".to_owned(), keeper.display().to_string()));
    }
    env
}

fn xml(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// A launchd agent: started at login, restarted after a crash (`SuccessfulExit` false), not after a clean stop.
pub(crate) fn launchd_plist(
    name: &str,
    argv: &[String],
    env: &[(String, String)],
    log: &Path,
) -> String {
    let mut out = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \
         \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\">\n<dict>\n",
    );
    let _ = writeln!(out, "  <key>Label</key>\n  <string>{}</string>", xml(name));
    out.push_str("  <key>ProgramArguments</key>\n  <array>\n");
    for arg in argv {
        let _ = writeln!(out, "    <string>{}</string>", xml(arg));
    }
    out.push_str("  </array>\n  <key>EnvironmentVariables</key>\n  <dict>\n");
    for (key, value) in env {
        let _ = writeln!(
            out,
            "    <key>{}</key>\n    <string>{}</string>",
            xml(key),
            xml(value)
        );
    }
    out.push_str("  </dict>\n  <key>RunAtLoad</key>\n  <true/>\n");
    out.push_str("  <key>KeepAlive</key>\n  <dict>\n    <key>SuccessfulExit</key>\n    <false/>\n  </dict>\n");
    out.push_str("  <key>ProcessType</key>\n  <string>Background</string>\n");
    let log = xml(&log.display().to_string());
    let _ = writeln!(
        out,
        "  <key>StandardOutPath</key>\n  <string>{log}</string>"
    );
    let _ = writeln!(
        out,
        "  <key>StandardErrorPath</key>\n  <string>{log}</string>"
    );
    out.push_str("</dict>\n</plist>\n");
    out
}

/// A systemd user unit, restarted on failure only.
pub(crate) fn systemd_unit(
    profile: &str,
    argv: &[String],
    env: &[(String, String)],
    log: &Path,
) -> String {
    let quote = |s: &str| format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""));
    let mut out = format!(
        "[Unit]\nDescription=Ome daemon (profile {profile})\nAfter=network-online.target\n\n[Service]\n"
    );
    let _ = writeln!(
        out,
        "ExecStart={}",
        argv.iter().map(|a| quote(a)).collect::<Vec<_>>().join(" ")
    );
    for (key, value) in env {
        let _ = writeln!(out, "Environment={}", quote(&format!("{key}={value}")));
    }
    let log = log.display();
    let _ = writeln!(
        out,
        "Restart=on-failure\nRestartSec=5\nStandardOutput=append:{log}\nStandardError=append:{log}"
    );
    out.push_str("\n[Install]\nWantedBy=default.target\n");
    out
}

fn run(program: &str, args: &[&str]) -> Result<()> {
    let status = Command::new(program)
        .args(args)
        .status()
        .map_err(|e| Error::failed(format!("Could not run {program}.")).cause(e))?;
    if status.success() {
        Ok(())
    } else {
        Err(Error::failed(format!(
            "`{program} {}` failed ({status}).",
            args.join(" ")
        )))
    }
}

/// The launchd domain of this user.
fn gui_domain() -> Result<String> {
    let out = Command::new("id").arg("-u").output()?;
    Ok(format!(
        "gui/{}",
        String::from_utf8_lossy(&out.stdout).trim()
    ))
}

/// Writes the service definition and starts it. The caller stops a daemon it started itself first: the service's
/// daemon would not get the profile's lock.
pub(crate) fn install(ctx: &Ctx) -> Result<Service> {
    let service = of(ctx)?;
    let argv = program(ctx)?;
    let env = environment(ctx);
    let log = launch::log_path(&ctx.profile);
    if let Some(dir) = service.file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    if cfg!(target_os = "macos") {
        // A previous definition is unloaded first; it may not exist.
        let domain = gui_domain()?;
        let _ = Command::new("launchctl")
            .args(["bootout", &format!("{domain}/{}", service.name)])
            .output();
        std::fs::write(
            &service.file,
            launchd_plist(&service.name, &argv, &env, &log),
        )?;
        run(
            "launchctl",
            &["bootstrap", &domain, &service.file.display().to_string()],
        )?;
    } else {
        std::fs::write(
            &service.file,
            systemd_unit(ctx.profile.name(), &argv, &env, &log),
        )?;
        run("systemctl", &["--user", "daemon-reload"])?;
        run("systemctl", &["--user", "enable", "--now", &service.name])?;
    }
    Ok(service)
}

/// Stops the service and removes its definition; `false` when there was none.
pub(crate) fn uninstall(ctx: &Ctx) -> Result<bool> {
    let service = of(ctx)?;
    if !service.file.exists() {
        return Ok(false);
    }
    if cfg!(target_os = "macos") {
        let domain = gui_domain()?;
        let _ = Command::new("launchctl")
            .args(["bootout", &format!("{domain}/{}", service.name)])
            .output();
        std::fs::remove_file(&service.file)?;
    } else {
        let _ = Command::new("systemctl")
            .args(["--user", "disable", "--now", &service.name])
            .output();
        std::fs::remove_file(&service.file)?;
        run("systemctl", &["--user", "daemon-reload"])?;
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{launchd_plist, systemd_unit};

    fn args() -> (Vec<String>, Vec<(String, String)>) {
        (
            vec![
                "/usr/bin/node".into(),
                "/opt/ome/omed.js".into(),
                "--profile".into(),
                "alice".into(),
            ],
            vec![("OME_HOME".into(), "/Users/a & b/.ome".into())],
        )
    }

    #[test]
    fn the_launchd_agent_restarts_only_after_a_crash() {
        let (argv, env) = args();
        let plist = launchd_plist(
            "network.ome.omed.alice",
            &argv,
            &env,
            Path::new("/tmp/d.log"),
        );
        assert!(plist.contains("<string>network.ome.omed.alice</string>"));
        assert!(plist.contains("<string>/opt/ome/omed.js</string>"));
        assert!(plist.contains("<key>SuccessfulExit</key>\n    <false/>"));
        assert!(plist.contains("/Users/a &amp; b/.ome"));
    }

    #[test]
    fn the_systemd_unit_quotes_its_arguments() {
        let (argv, env) = args();
        let unit = systemd_unit("alice", &argv, &env, Path::new("/tmp/d.log"));
        assert!(
            unit.contains(
                "ExecStart=\"/usr/bin/node\" \"/opt/ome/omed.js\" \"--profile\" \"alice\""
            )
        );
        assert!(unit.contains("Environment=\"OME_HOME=/Users/a & b/.ome\""));
        assert!(unit.contains("Restart=on-failure"));
    }
}
