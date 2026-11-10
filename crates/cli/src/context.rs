//! What every command gets: the profile, the output mode, whether it may prompt, and the daemon session.

use std::{
    cell::Cell,
    io::{IsTerminal, Write},
};

use ome_client::{DEFAULT_PROFILE, DaemonClient, Home, Profile};
use serde::Serialize;
use tokio::sync::OnceCell;

use crate::{
    args::Global,
    error::{Error, Result},
    launch,
    ui::theme,
};

/// The file in the Ome home that names the profile `ome profile use` chose.
pub(crate) const CURRENT_PROFILE: &str = "current-profile";

/// How results are printed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Mode {
    /// Prompts, steps and colour on stderr; results on stdout.
    Human,
    /// One JSON value on stdout.
    Json,
    /// One record per line, tab-separated.
    Plain,
}

#[expect(
    clippy::struct_excessive_bools,
    reason = "independent command-line switches"
)]
pub(crate) struct Ctx {
    pub(crate) home: Home,
    pub(crate) profile: Profile,
    /// Whether the profile was named with `--profile`, so hints repeat it.
    pub(crate) named_profile: bool,
    /// Whether `--profile` or `OME_PROFILE` named it.
    pub(crate) profile_given: bool,
    pub(crate) mode: Mode,
    /// Prompts allowed: a terminal on stdin and stderr, human mode, no `--no-input`.
    pub(crate) interactive: bool,
    pub(crate) yes: bool,
    pub(crate) quiet: bool,
    pub(crate) verbose: bool,
    /// `--vault`.
    pub(crate) vault: Option<String>,
    client: OnceCell<DaemonClient>,
    /// Set while a flow runs other commands inside it (a membership change approving its proposal), so only the
    /// flow's own result is printed as JSON.
    nested: Cell<bool>,
}

/// Restores the context when a nested command ends.
pub(crate) struct Nested<'a>(&'a Cell<bool>, bool);

impl Drop for Nested<'_> {
    fn drop(&mut self) {
        self.0.set(self.1);
    }
}

impl Ctx {
    /// `choose`: the command acts as a member, so with several profiles and none chosen it may ask which.
    pub(crate) fn new(global: &Global, choose: bool) -> Result<Self> {
        let home = Home::from_env().map_err(Error::from)?;
        let mode = if global.json {
            Mode::Json
        } else if global.plain {
            Mode::Plain
        } else {
            Mode::Human
        };
        let terminal = std::io::stdin().is_terminal() && std::io::stderr().is_terminal();
        let interactive = terminal && mode == Mode::Human && !global.no_input;
        let from_env = std::env::var("OME_PROFILE").ok().filter(|v| !v.is_empty());
        let (name, named_profile) = match &global.profile {
            // A profile from OME_PROFILE holds for the whole terminal: hints need not repeat it.
            Some(name) => (name.clone(), from_env.as_deref() != Some(name.as_str())),
            None => (pick_profile(&home, choose, interactive)?, false),
        };
        let profile = home
            .profile(&name)
            .map_err(|e| Error::usage(e.to_string()))?;
        Ok(Self {
            home,
            profile,
            named_profile,
            profile_given: global.profile.is_some(),
            mode,
            interactive,
            yes: global.yes,
            quiet: global.quiet,
            verbose: global.verbose,
            vault: global.vault.clone(),
            client: OnceCell::new(),
            nested: Cell::new(false),
        })
    }

    /// The same context acting on profile `name`, with its own daemon session.
    pub(crate) fn with_profile(&self, name: &str) -> Result<Self> {
        let profile = self
            .home
            .profile(name)
            .map_err(|e| Error::usage(e.to_string()))?;
        Ok(Self {
            home: self.home.clone(),
            profile,
            named_profile: true,
            profile_given: true,
            mode: self.mode,
            interactive: self.interactive,
            yes: self.yes,
            quiet: self.quiet,
            verbose: self.verbose,
            vault: None,
            client: OnceCell::new(),
            nested: Cell::new(false),
        })
    }

    /// Steps, notes and prompts are shown.
    pub(crate) fn human(&self) -> bool {
        self.mode == Mode::Human && !self.quiet
    }

    #[expect(
        clippy::unused_self,
        reason = "output decisions go through the context"
    )]
    pub(crate) fn color(&self) -> bool {
        console::colors_enabled()
    }

    /// `ome <args>`, with `--profile` when the profile was named.
    pub(crate) fn ome(&self, args: &str) -> String {
        if self.named_profile {
            format!("ome --profile {} {args}", self.profile.name())
        } else {
            format!("ome {args}")
        }
    }

    /// A result line on stdout.
    #[expect(
        clippy::unused_self,
        reason = "output decisions go through the context"
    )]
    pub(crate) fn out(&self, text: &str) {
        let mut stdout = std::io::stdout().lock();
        let _ = writeln!(stdout, "{text}");
    }

    /// Runs the commands a flow calls into until the guard drops: their JSON results are not printed.
    pub(crate) fn nest(&self) -> Nested<'_> {
        Nested(&self.nested, self.nested.replace(true))
    }

    /// The result as one JSON value on stdout.
    pub(crate) fn json<T: Serialize>(&self, value: &T) -> Result<()> {
        if self.nested.get() {
            return Ok(());
        }
        let text = serde_json::to_string_pretty(value)
            .map_err(|e| Error::failed(format!("could not encode the result: {e}")))?;
        self.out(&text);
        Ok(())
    }

    /// What to run next, after a state change.
    pub(crate) fn next(&self, args: &str) {
        if self.human() {
            eprintln!("  {} {}", theme::dim("next:"), theme::name(self.ome(args)));
        }
    }

    pub(crate) fn intro(&self, title: &str) {
        if self.human() {
            let _ = cliclack::intro(theme::bold(title));
        }
    }

    pub(crate) fn outro(&self, text: &str) {
        if self.human() {
            let _ = cliclack::outro(text);
        }
    }

    /// A section on the rail: a title and its lines.
    pub(crate) fn section(&self, title: &str, lines: &[String]) {
        if self.human() {
            let mut text = title.to_owned();
            for line in lines {
                text.push('\n');
                text.push_str(line);
            }
            let _ = cliclack::log::step(text);
        }
    }

    pub(crate) fn info(&self, text: &str) {
        if self.human() {
            let _ = cliclack::log::info(text);
        }
    }

    pub(crate) fn warn(&self, text: &str) {
        if self.human() {
            let _ = cliclack::log::warning(text);
        }
    }

    pub(crate) fn success(&self, text: &str) {
        if self.human() {
            let _ = cliclack::log::success(text);
        }
    }

    /// The daemon session, starting the daemon first when the profile exists but its daemon is not running.
    pub(crate) async fn client(&self) -> Result<&DaemonClient> {
        self.connect(false).await
    }

    /// The daemon session, starting the daemon even for a profile not set up yet (`ome init`).
    pub(crate) async fn client_for_setup(&self) -> Result<&DaemonClient> {
        self.connect(true).await
    }

    async fn connect(&self, setup: bool) -> Result<&DaemonClient> {
        self.client
            .get_or_try_init(|| async {
                match DaemonClient::discover(&self.profile).await {
                    Ok(client) => Ok(client),
                    Err(_) if setup || launch::profile_exists(&self.profile) => {
                        launch::start(self).await?;
                        Ok(DaemonClient::discover(&self.profile).await?)
                    }
                    Err(_) => Err(Error::new(
                        crate::error::Exit::Daemon,
                        format!("There is no profile {:?} yet.", self.profile.name()),
                    )
                    .hint(format!("Set one up with `{}`.", self.ome("init")))),
                }
            })
            .await
    }

    /// A session with an already-running daemon, never starting one.
    pub(crate) async fn running_client(&self) -> Option<&DaemonClient> {
        if let Some(client) = self.client.get() {
            return Some(client);
        }
        let client = DaemonClient::discover(&self.profile).await.ok()?;
        let _ = self.client.set(client);
        self.client.get()
    }

    /// Restarts the daemon, which reads some settings (the node URL) only as it starts.
    pub(crate) async fn restart_daemon(&mut self) -> Result<()> {
        self.drop_client();
        launch::stop(&self.profile).await?;
        launch::start(self).await?;
        Ok(())
    }

    /// Forgets the session, after the daemon stopped.
    pub(crate) fn drop_client(&mut self) {
        self.client = OnceCell::new();
    }
}

/// The profile to act as when none is named: the one `ome profile use` chose, else the only one set up, else (in a
/// terminal, for a command that acts as a member) the one the person picks, else `default`.
fn pick_profile(home: &Home, choose: bool, interactive: bool) -> Result<String> {
    let chosen = std::fs::read_to_string(home.root().join(CURRENT_PROFILE))
        .map(|name| name.trim().to_owned())
        .ok()
        .filter(|name| !name.is_empty());
    if let Some(chosen) = chosen {
        return Ok(chosen);
    }
    let set_up = set_up_profiles(home);
    match set_up.as_slice() {
        [] => Ok(DEFAULT_PROFILE.to_owned()),
        [(only, _)] => Ok(only.clone()),
        _ if choose && interactive => {
            let mut select = cliclack::select("Which profile?");
            for (name, member) in &set_up {
                select = select.item(
                    name.clone(),
                    member.clone().unwrap_or_else(|| name.clone()),
                    name,
                );
            }
            let name: String = select.interact()?;
            let _ = cliclack::log::remark(format!(
                "To stop being asked: `ome profile use {name}`, or `export OME_PROFILE={name}` in this terminal."
            ));
            Ok(name)
        }
        _ if choose => Err(Error::usage("There are several profiles; which one?").hint(
            "Pass --profile <name>, set OME_PROFILE, or choose one with `ome profile use <name>`.",
        )),
        _ => Ok(DEFAULT_PROFILE.to_owned()),
    }
}

/// The profiles that ran `ome init`, by name, with their member's name.
pub(crate) fn set_up_profiles(home: &Home) -> Vec<(String, Option<String>)> {
    let mut out: Vec<(String, Option<String>)> = std::fs::read_dir(home.root().join("profiles"))
        .map(|entries| {
            entries
                .filter_map(std::result::Result::ok)
                .filter_map(|e| e.file_name().into_string().ok())
                .filter_map(|name| home.profile(&name).ok())
                .filter(launch::profile_exists)
                .map(|p| (p.name().to_owned(), launch::member_name(&p)))
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}
