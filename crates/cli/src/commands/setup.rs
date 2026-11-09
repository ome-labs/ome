//! `ome init`, `ome status`, `ome doctor`, `ome completions`, and `ome` alone: the home screen.

use std::process::Command;

use clap::CommandFactory;
use ome_api::{
    Empty,
    daemon::{
        ConfigSet, DaemonInfoGet, NodeStatusGet, ProfileCreate, ProfileCreateParams,
        ProposalStatus, ProposalsList, RelaysParams, RelaysSet, ServiceStatus, SettingsPatch,
        Status, StatusGet,
    },
    keeper::VaultParams,
};

use crate::{
    args::{Cli, InitArgs, NodeArgs, Prover},
    context::{Ctx, Mode},
    error::{Error, Result},
    launch, secret,
    ui::{describe, format, prompt, steps::Step, theme},
};

const LOCAL: &str = "http://localhost:8080";
const TESTNET: &str = "https://aztec-testnet.drpc.org";

/// `ome` with no command: the status and what to do next in a terminal; the help otherwise.
pub(crate) async fn home(ctx: &Ctx) -> Result<()> {
    if !ctx.interactive {
        let _ = Cli::command().print_help();
        return Ok(());
    }
    status(ctx).await?;
    eprintln!(
        "  {} {}",
        theme::dim("more:"),
        theme::dim("ome --help · ome watch · ome proposals · ome vault receive")
    );
    Ok(())
}

/// A profile name from a member's name: `Alice Smith` → `alice-smith`.
fn profile_name(member: &str) -> String {
    let mut out = String::new();
    for c in member.trim().to_lowercase().chars() {
        if c.is_ascii_alphanumeric() || c == '_' {
            out.push(c);
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    let out: String = out.trim_end_matches('-').chars().take(32).collect();
    if out.is_empty() { "member".into() } else { out }
}

#[expect(clippy::too_many_lines, reason = "a guided flow reads top to bottom")]
pub(crate) async fn init(ctx: &mut Ctx, args: InitArgs) -> Result<()> {
    ctx.intro("Set up Ome");
    let name = prompt::text(
        ctx,
        args.name,
        "--name",
        "Your name, as the other members will see it",
        None,
        |v| {
            let n = v.trim().chars().count();
            if (1..=48).contains(&n) {
                Ok(())
            } else {
                Err("Use 1 to 48 characters.".into())
            }
        },
    )?;
    // Without --profile, each member gets a profile named after them: `ome init` twice on one machine sets up two
    // members, and nobody has to type --profile afterwards.
    if !ctx.profile_given {
        ctx.profile = ctx
            .home
            .profile(&profile_name(&name))
            .map_err(|e| Error::usage(e.to_string()))?;
    }
    if launch::profile_exists(&ctx.profile) {
        let set_up = ctx
            .client()
            .await?
            .call::<StatusGet>(&Empty {})
            .await?
            .profile
            .is_some();
        if set_up {
            return Err(Error::usage(format!(
                "Profile {:?} is already set up.",
                ctx.profile.name()
            ))
            .hint("Set up another member under another name: `ome init --profile <name>`."));
        }
    }
    let node_url = match args.node.node_url {
        Some(url) => url,
        None if ctx.interactive => {
            let choice = prompt::select(
                ctx,
                "--node-url",
                "Which network?",
                vec![
                    (LOCAL.to_owned(), "Local network".into(), LOCAL.into()),
                    (TESTNET.to_owned(), "Testnet".into(), TESTNET.into()),
                    (
                        String::new(),
                        "Another node…".into(),
                        "its JSON-RPC URL".into(),
                    ),
                ],
            )?;
            if choice.is_empty() {
                prompt::text(ctx, None, "--node-url", "Node URL", None, |v| {
                    if v.starts_with("http://") || v.starts_with("https://") {
                        Ok(())
                    } else {
                        Err("Use an http:// or https:// URL.".into())
                    }
                })?
            } else {
                choice
            }
        }
        None => return Err(Error::missing("the node", "--node-url (or AZTEC_NODE_URL)")),
    };
    let local = node_url.contains("localhost") || node_url.contains("127.0.0.1");
    let prover = args
        .prover
        .unwrap_or(if local { Prover::None } else { Prover::Native });
    // Testnet profiles get Ome Labs' relay from the daemon; `--relay` replaces it.
    let relays = args.relays;
    let current = ctx
        .client_for_setup()
        .await?
        .call::<DaemonInfoGet>(&Empty {})
        .await?
        .node;
    if current != node_url {
        // The daemon reads the node URL as it starts.
        ctx.client_for_setup()
            .await?
            .call::<ConfigSet>(&SettingsPatch {
                node_url: Some(node_url.clone()),
                ..SettingsPatch::default()
            })
            .await?;
        ctx.restart_daemon().await?;
    }
    let client = ctx.client_for_setup().await?;
    let step = Step::start(ctx, &format!("Checking the node at {node_url}"));
    let node = client.call::<NodeStatusGet>(&Empty {}).await?;
    if node.reachable {
        step.done(&format!(
            "The node answers: {}",
            describe::network(node.l1_chain_id, node.rollup_version)
        ));
    } else {
        step.fail("The node does not answer");
        return Err(Error::new(
            crate::error::Exit::NotReady,
            format!("No Aztec node answers at {node_url}."),
        )
        .hint(if local {
            "Start one with `aztec start --local-network`, then run this again.".to_owned()
        } else {
            "Check the URL, or try again in a moment.".to_owned()
        })
        .cause(node.error.unwrap_or_default()));
    }
    let passphrase = secret::new_passphrase(ctx)?;

    let step = Step::start(ctx, "Sealing your signing key and creating your account");
    let created = client
        .call::<ProfileCreate>(&ProfileCreateParams {
            name: name.trim().to_owned(),
            node_url: node_url.clone(),
            passphrase: passphrase.as_str().to_owned(),
        })
        .await;
    let mut status = match created {
        Ok(status) => {
            step.done("Signing key sealed, account created");
            status
        }
        Err(error) => {
            step.fail("Could not create the profile");
            return Err(error.into());
        }
    };
    status.settings = client
        .call::<ConfigSet>(&SettingsPatch {
            prover_enabled: Some(prover != Prover::None),
            ..SettingsPatch::default()
        })
        .await?;
    if !relays.is_empty() {
        status.settings = client.call::<RelaysSet>(&RelaysParams { relays }).await?;
    }
    if !status.settings.relays.is_empty() {
        ctx.info(&format!(
            "Relays: {}",
            status
                .settings
                .relays
                .iter()
                // `/dns4/relay.ome.network/udp/4001/…` → `relay.ome.network`, once per host.
                .filter_map(|r| r.split('/').nth(2))
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    let touch_id = args.touch_id
        || (cfg!(target_os = "macos")
            && ctx.interactive
            && prompt::confirm(ctx, "Unlock with Touch ID from now on?", false)?);
    if touch_id {
        secret::enable_touch_id(ctx, client, Some(passphrase.as_str())).await?;
        ctx.success("Touch ID is on");
    }
    if ctx.mode == Mode::Json {
        return ctx.json(&status);
    }
    ctx.outro(&format!(
        "{} is ready on {} · profile {}",
        name.trim(),
        theme::name(&node_url),
        theme::name(ctx.profile.name())
    ));
    if crate::context::set_up_profiles(&ctx.home).len() > 1 {
        eprintln!(
            "  {}",
            theme::dim(format!(
                "Several members on this machine: in {}'s terminal, `export OME_PROFILE={}`.",
                name.trim(),
                ctx.profile.name()
            ))
        );
    }
    ctx.next("vault create");
    ctx.next("vault join <invite>");
    Ok(())
}

pub(crate) async fn status(ctx: &Ctx) -> Result<()> {
    let Some(client) = ctx.running_client().await else {
        if ctx.mode == Mode::Json {
            return ctx.json(&serde_json::json!({
                "profile": ctx.profile.name(),
                "daemon": "stopped",
                "setUp": launch::profile_exists(&ctx.profile),
            }));
        }
        if ctx.mode == Mode::Plain {
            ctx.out("daemon\tstopped");
            return Ok(());
        }
        ctx.intro(&format!("Ome · profile {}", ctx.profile.name()));
        if launch::profile_exists(&ctx.profile) {
            ctx.warn("The daemon is not running.");
            ctx.outro("Start it, or run any command and it starts by itself");
            ctx.next("daemon start");
        } else {
            ctx.warn("This profile is not set up yet.");
            ctx.outro("Set it up in a minute");
            ctx.next("init");
        }
        return Ok(());
    };
    let status = client.call::<StatusGet>(&Empty {}).await?;
    if ctx.mode == Mode::Json {
        return ctx.json(&status);
    }
    if ctx.mode == Mode::Plain {
        ctx.out(&plain_status(&status));
        return Ok(());
    }
    ctx.intro(&format!("Ome · profile {}", ctx.profile.name()));
    if let Some(profile) = &status.profile {
        ctx.section(
            "Member",
            &[format!(
                "{} · account {} · signing key {}",
                theme::bold(&profile.name),
                theme::dim(format::short(&profile.account)),
                theme::dim(format::short_point(&profile.signer_key))
            )],
        );
    }
    ctx.section("Services", &services(&status));
    let vaults: Vec<String> = status
        .vaults
        .iter()
        .map(|v| {
            format!(
                "{} {}  {} · you are #{}  {}",
                if v.is_default {
                    theme::ok("●")
                } else {
                    theme::dim("○")
                },
                theme::bold(&v.name),
                describe::shape(v.threshold, v.viewing_threshold, v.members),
                v.my_index,
                theme::dim(format::short(&v.address))
            )
        })
        .collect();
    if !vaults.is_empty() {
        ctx.section("Vaults", &vaults);
    }
    match next_step(ctx, &status).await {
        Some((text, command)) => {
            ctx.outro(&text);
            ctx.next(&command);
        }
        None => ctx.outro("All quiet"),
    }
    Ok(())
}

fn services(status: &Status) -> Vec<String> {
    let k = &status.keeper;
    let keeper = match k.status {
        ServiceStatus::Running => format!(
            "{} keeper   running{}{}",
            theme::check(true),
            k.remote
                .as_ref()
                .map(|r| format!(" on {r}"))
                .unwrap_or_default(),
            k.version
                .as_ref()
                .map(|v| theme::dim(format!(" · v{v}")).to_string())
                .unwrap_or_default()
        ),
        ServiceStatus::Starting | ServiceStatus::Restarting => {
            format!("{} keeper   starting", theme::caution())
        }
        ServiceStatus::Stopped => format!(
            "{} keeper   stopped{}",
            theme::caution(),
            theme::dim(" (it starts when you unlock: ome unlock)")
        ),
        ServiceStatus::Failed => format!(
            "{} keeper   failed: {}",
            theme::check(false),
            k.error.as_deref().unwrap_or("no reason given")
        ),
    };
    let n = &status.node;
    let node = if n.reachable {
        format!(
            "{} node     {} · block {}{}",
            theme::check(true),
            describe::network(n.l1_chain_id, n.rollup_version),
            n.block_number
                .map_or("?".into(), |b| format::group_thousands(&b.to_string())),
            n.synced_block
                .filter(|s| Some(*s) != n.block_number)
                .map(|s| theme::dim(format!(
                    " (synced {})",
                    format::group_thousands(&s.to_string())
                ))
                .to_string())
                .unwrap_or_default()
        )
    } else {
        format!(
            "{} node     unreachable at {}: {}",
            theme::check(false),
            n.url,
            n.error.as_deref().unwrap_or("no answer")
        )
    };
    let s = &status.signer;
    let signer = match s.unlocked_until {
        Some(until) if !s.locked => format!(
            "{} signer   unlocked for {}",
            theme::check(true),
            format::span(until.saturating_sub(format::now()))
        ),
        _ => format!(
            "{} signer   locked {}",
            theme::dim("○"),
            theme::dim("(asks for your passphrase when you sign)")
        ),
    };
    let relays = if status.settings.relays.is_empty() {
        format!(
            "{} relays   none {}",
            theme::dim("○"),
            theme::dim("(direct connections only)")
        )
    } else {
        format!(
            "{} relays   {}",
            theme::check(true),
            format::plural(
                u64::try_from(status.settings.relays.len()).unwrap_or(0),
                "relay",
                "relays"
            )
        )
    };
    vec![keeper, node, signer, relays]
}

/// What the member should do next, as a sentence and a command.
async fn next_step(ctx: &Ctx, status: &Status) -> Option<(String, String)> {
    if status.profile.is_none() {
        return Some(("Set up this profile".into(), "init".into()));
    }
    if status.vaults.is_empty() {
        return Some((
            "No vault yet: create one, or join a co-founder's".into(),
            "vault create".into(),
        ));
    }
    let vault = status
        .default_vault
        .or_else(|| (status.vaults.len() == 1).then(|| status.vaults[0].address))?;
    let proposals = ctx
        .running_client()
        .await?
        .call::<ProposalsList>(&VaultParams { vault })
        .await
        .ok()?;
    if let Some(p) = proposals.iter().find(|p| describe::can_vote(p)) {
        return Some((
            format!(
                "{} waits for your vote: {}",
                format::id(&p.id),
                describe::intent(p)
            ),
            format!("approve {}", format::id(&p.id)),
        ));
    }
    proposals
        .iter()
        .find(|p| p.status == ProposalStatus::Ready)
        .map(|p| {
            (
                format!(
                    "{} is ready to execute: {}",
                    format::id(&p.id),
                    describe::intent(p)
                ),
                format!("execute {}", format::id(&p.id)),
            )
        })
}

fn plain_status(status: &Status) -> String {
    let mut lines = vec![
        format!(
            "profile\t{}",
            status.profile.as_ref().map_or("-", |p| p.name.as_str())
        ),
        format!("keeper\t{:?}", status.keeper.status).to_lowercase(),
        format!(
            "node\t{}",
            if status.node.reachable {
                "reachable"
            } else {
                "unreachable"
            }
        ),
        format!(
            "signer\t{}",
            if status.signer.locked {
                "locked"
            } else {
                "unlocked"
            }
        ),
    ];
    for v in &status.vaults {
        lines.push(format!(
            "vault\t{}\t{}\t{}",
            v.address, v.name, v.is_default
        ));
    }
    lines.join("\n")
}

#[expect(clippy::too_many_lines, reason = "a guided flow reads top to bottom")]
pub(crate) async fn doctor(ctx: &Ctx, args: NodeArgs) -> Result<()> {
    ctx.intro("Ome doctor");
    let mut checks: Vec<(bool, String, Option<String>)> = Vec::new();
    let (method, update) = launch::install_method(&ctx.home);
    checks.push((
        true,
        format!("ome {} installed by {method}", ome_api::RELEASE_VERSION),
        Some(format!("Update with `{update}`.")),
    ));
    let node = Command::new("node").arg("--version").output();
    let version = node
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned());
    let major = version.as_deref().and_then(|v| {
        v.trim_start_matches('v')
            .split('.')
            .next()?
            .parse::<u32>()
            .ok()
    });
    checks.push(match (&version, major) {
        (Some(v), Some(m)) if m >= 24 => (true, format!("Node {v}"), None),
        (Some(v), _) => (
            false,
            format!("Node {v} is too old"),
            Some("Install Node 24 or later.".into()),
        ),
        (None, _) => (
            false,
            "Node is not on PATH".into(),
            Some("Install Node 24 or later.".into()),
        ),
    });
    checks.push(match launch::omed_path(&ctx.home) {
        Ok(path) => (true, format!("omed at {}", path.display()), None),
        Err(e) => (false, e.message, e.hint),
    });
    checks.push(match launch::keeper_path(&ctx.home) {
        Some(path) => (true, format!("ome-keeper at {}", path.display()), None),
        None => (
            false,
            "ome-keeper is not installed".into(),
            Some("Install it with `pnpm cli:install`, or set OME_KEEPER_BIN.".into()),
        ),
    });
    let set_up = launch::profile_exists(&ctx.profile);
    checks.push(if set_up {
        (
            true,
            format!("profile {} is set up", ctx.profile.name()),
            None,
        )
    } else {
        (
            false,
            format!("profile {} is not set up", ctx.profile.name()),
            Some(format!("Run `{}`.", ctx.ome("init"))),
        )
    });
    if let Some(client) = ctx.running_client().await {
        let status = client.call::<StatusGet>(&Empty {}).await?;
        let daemon = client.call::<DaemonInfoGet>(&Empty {}).await?;
        checks.push(if daemon.version == ome_api::RELEASE_VERSION {
            (
                true,
                format!("the daemon is running, version {}", daemon.version),
                None,
            )
        } else {
            (
                false,
                format!(
                    "the daemon runs version {}, this ome {}",
                    daemon.version,
                    ome_api::RELEASE_VERSION
                ),
                Some(format!(
                    "Restart it on this version: `{}`.",
                    ctx.ome("daemon restart")
                )),
            )
        });
        if let Some(keeper) = status
            .keeper
            .version
            .as_ref()
            .filter(|v| *v != ome_api::RELEASE_VERSION)
        {
            checks.push((
                false,
                format!(
                    "the keeper is version {keeper}, this ome {}",
                    ome_api::RELEASE_VERSION
                ),
                Some(format!(
                    "Restart the daemon so it starts this release's keeper: `{}`.",
                    ctx.ome("daemon restart")
                )),
            ));
        }
        if let Ok(service) = crate::service::of(ctx) {
            checks.push((
                true,
                if service.file.exists() {
                    format!("the daemon runs as a login service ({})", service.name)
                } else {
                    "the daemon runs only while a command starts it".into()
                },
                (!service.file.exists()).then(|| {
                    format!(
                        "To keep your keeper answering, run `{}`.",
                        ctx.ome("daemon install-service")
                    )
                }),
            ));
        }
        let node = client.call::<NodeStatusGet>(&Empty {}).await?;
        if let Some(url) = args.node_url.filter(|u| *u != node.url) {
            checks.push((
                false,
                format!("the daemon uses {}, not {url}", node.url),
                Some(format!(
                    "Switch with `{}`.",
                    ctx.ome(&format!("config set node-url {url}"))
                )),
            ));
        }
        checks.push(if node.reachable {
            (
                true,
                format!(
                    "the node at {} answers: {}, v{}",
                    node.url,
                    describe::network(node.l1_chain_id, node.rollup_version),
                    node.node_version.as_deref().unwrap_or("?")
                ),
                None,
            )
        } else {
            (
                false,
                format!("the node at {} does not answer", node.url),
                Some(format!(
                    "Start it (`aztec start --local-network`) or point at another: `{}`.",
                    ctx.ome("config set node-url <url>")
                )),
            )
        });
        checks.push(match status.keeper.status {
            ServiceStatus::Running => (true, "the keeper is running".into(), None),
            _ if !set_up => (
                true,
                "the keeper starts once the profile is set up".into(),
                None,
            ),
            other => (
                false,
                format!("the keeper is {other:?}").to_lowercase(),
                Some(format!("Read its log: `{}`.", ctx.ome("keeper logs"))),
            ),
        });
        if status.settings.relays.is_empty() {
            checks.push((
                true,
                "no relays: members must reach your keeper directly".into(),
                None,
            ));
        }
    } else if set_up {
        checks.push((
            false,
            "the daemon is not running".into(),
            Some(format!("Start it with `{}`.", ctx.ome("daemon start"))),
        ));
    }
    if cfg!(target_os = "macos") {
        checks.push((
            true,
            if secret::touch_id_enabled(&ctx.profile) {
                "Touch ID unlocks the signer".into()
            } else {
                format!("Touch ID is off ({})", ctx.ome("signer touch-id on"))
            },
            None,
        ));
    }
    for tool in ["aztec", "aztec-wallet"] {
        let found = Command::new(tool).arg("--version").output().is_ok();
        checks.push((
            true,
            if found {
                format!("{tool} is on PATH")
            } else {
                format!("{tool} is not on PATH (optional: `aztec-up install 6.0.0`)")
            },
            None,
        ));
    }
    let failed = checks.iter().filter(|(ok, ..)| !ok).count();
    if ctx.mode == Mode::Json {
        ctx.json(
            &checks
                .iter()
                .map(|(ok, text, fix)| serde_json::json!({ "ok": ok, "check": text, "fix": fix }))
                .collect::<Vec<_>>(),
        )?;
    } else if ctx.mode == Mode::Plain {
        for (ok, text, _) in &checks {
            ctx.out(&format!("{}\t{text}", if *ok { "ok" } else { "fail" }));
        }
    } else {
        let lines: Vec<String> = checks
            .iter()
            .flat_map(|(ok, text, fix)| {
                let mut lines = vec![format!("{} {text}", theme::check(*ok))];
                if let Some(fix) = fix {
                    lines.push(format!("  {}", theme::dim(fix)));
                }
                lines
            })
            .collect();
        ctx.section("Checks", &lines);
    }
    if failed == 0 {
        ctx.outro("Everything checks out");
        Ok(())
    } else {
        Err(Error::failed(format!(
            "{} failed.",
            format::plural(u64::try_from(failed).unwrap_or(0), "check", "checks")
        )))
    }
}

pub(crate) fn completions(shell: clap_complete::Shell) {
    clap_complete::generate(shell, &mut Cli::command(), "ome", &mut std::io::stdout());
}

#[cfg(test)]
mod tests {
    #[test]
    fn profiles_are_named_after_members() {
        assert_eq!(super::profile_name("Alice"), "alice");
        assert_eq!(super::profile_name("  Alice Smith "), "alice-smith");
        assert_eq!(super::profile_name("Zoë!"), "zo");
        assert_eq!(super::profile_name("…"), "member");
    }
}
