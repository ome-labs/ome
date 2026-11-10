//! `ome vault …`, `ome balance`, `ome activity`, `ome counterparties`. Founding is a lobby inside the daemon; here
//! people only create and join.

use std::{collections::HashSet, time::Duration};

use ome_api::{
    Amount, Empty, Hex32,
    daemon::{
        ActivityEntry, AliasParams, ConfigGet, DaemonEvent, FoundingBegin, FoundingGet,
        FoundingState, FoundingStepId, LobbyCreate, LobbyCreateParams, LobbyGet, LobbyInspect,
        LobbyJoin, LobbyList, LobbyRemove, LobbyStart, LobbyUpdate, LobbyView,
        PaymentRequestParams, RenameParams, StartLobbyParams, TokensList, VaultAlias,
        VaultDetailsGet, VaultList, VaultPeers, VaultReceiving, VaultRename, VaultRequest,
        VaultRetire, VaultUse,
    },
    keeper::{
        InviteParams, LobbyParams, LobbyRemoveParams, LobbyRole, LobbyStatus, LobbyUpdateParams,
        VaultParams,
    },
};
use ome_client::DaemonClient;

use crate::{
    args::{ActivityArgs, BalanceArgs, CreateArgs, LobbyCommand, RequestArgs, VaultCommand},
    context::{Ctx, Mode},
    error::{Error, Result},
    lists, refs,
    ui::{
        describe, format, prompt, qr,
        steps::{Step, Tracked, Tracker},
        table::{self, Tone},
        theme,
    },
};

const POLL: Duration = Duration::from_millis(700);

pub(crate) async fn run(ctx: &Ctx, command: VaultCommand) -> Result<()> {
    match command {
        VaultCommand::Create(args) => create(ctx, args).await,
        VaultCommand::Join { invite } => join(ctx, invite).await,
        VaultCommand::List => list(ctx).await,
        VaultCommand::Show { target } => show(ctx, target).await,
        VaultCommand::Use { target } => use_vault(ctx, target).await,
        VaultCommand::Alias { alias } => set_alias(ctx, alias).await,
        VaultCommand::Members => members(ctx).await,
        VaultCommand::Peers => peers(ctx).await,
        VaultCommand::Health => health(ctx).await,
        VaultCommand::Audit { limit } => audit(ctx, limit).await,
        VaultCommand::Receive => receive(ctx).await,
        VaultCommand::Retire { confirm } => retire(ctx, confirm.as_deref()).await,
        VaultCommand::Balance(args) => balance(ctx, args).await,
        VaultCommand::Lobby(command) => lobby(ctx, command).await,
        VaultCommand::Rename { name } => rename(ctx, &name).await,
    }
}

/// `ome vault rename`: the vault's name for every member.
async fn rename(ctx: &Ctx, name: &str) -> Result<()> {
    let vault = refs::vault(ctx).await?;
    let renamed = ctx
        .client()
        .await?
        .call::<VaultRename>(&RenameParams {
            vault: vault.address,
            name: name.trim().to_owned(),
        })
        .await?;
    if ctx.mode == Mode::Json {
        return ctx.json(&renamed);
    }
    ctx.success(&format!(
        "{} is {} to every member now",
        vault.name, renamed.name
    ));
    Ok(())
}

/// `ome vault lobby`: the lobby this member hosts (or joined), a joiner removed, or the plan changed.
async fn lobby(ctx: &Ctx, command: LobbyCommand) -> Result<()> {
    let client = ctx.client().await?;
    let open: Vec<LobbyView> = client
        .call::<LobbyList>(&Empty {})
        .await?
        .into_iter()
        .filter(|l| l.status == LobbyStatus::Open)
        .collect();
    let hosted = || {
        open.iter()
            .find(|l| l.role == LobbyRole::Host)
            .cloned()
            .ok_or_else(|| {
                Error::usage("You host no open lobby.")
                    .hint(format!("Start one with `{}`.", ctx.ome("vault create")))
            })
    };
    let view = match command {
        LobbyCommand::Show => {
            if ctx.mode == Mode::Json {
                return ctx.json(&open);
            }
            if open.is_empty() {
                ctx.info("No open lobby.");
                return Ok(());
            }
            for lobby in &open {
                show_lobby(ctx, lobby);
            }
            return Ok(());
        }
        LobbyCommand::Remove { member } => {
            let lobby = hosted()?;
            let wanted = member.to_lowercase();
            let found = lobby
                .members
                .iter()
                .filter(|m| !m.you)
                .find(|m| {
                    m.name.to_lowercase() == wanted || m.identity.to_string().starts_with(&wanted)
                })
                .ok_or_else(|| {
                    Error::usage(format!("No joiner {member:?} in {}.", lobby.name)).hint(format!(
                        "See who joined with `{}`.",
                        ctx.ome("vault lobby show")
                    ))
                })?;
            prompt::proceed(ctx, &format!("Remove {} from {}?", found.name, lobby.name))?;
            let view = client
                .call::<LobbyRemove>(&LobbyRemoveParams {
                    lobby: lobby.lobby,
                    identity: found.identity,
                })
                .await?;
            ctx.success(&format!(
                "{} is out; the old invite no longer works",
                found.name
            ));
            view
        }
        LobbyCommand::Update {
            members,
            approvals,
            readers,
        } => {
            let lobby = hosted()?;
            let total = members.unwrap_or(lobby.expected);
            accept_risks(
                ctx,
                u32::from(approvals.unwrap_or(lobby.threshold)),
                u32::from(readers.unwrap_or(lobby.viewing_threshold)),
                u32::from(total),
            )?;
            client
                .call::<LobbyUpdate>(&LobbyUpdateParams {
                    lobby: lobby.lobby,
                    members,
                    threshold: approvals,
                    viewing_threshold: readers,
                })
                .await?
        }
    };
    if ctx.mode == Mode::Json {
        return ctx.json(&view);
    }
    show_lobby(ctx, &view);
    if let Some(invite) = &view.invite {
        show_invite(ctx, invite);
    }
    Ok(())
}

fn show_lobby(ctx: &Ctx, lobby: &LobbyView) {
    let mut lines = vec![format!(
        "{} of {} joined · {} to spend, {} to read",
        lobby.members.len(),
        lobby.expected,
        lobby.threshold,
        lobby.viewing_threshold
    )];
    lines.extend(lobby.members.iter().map(|m| {
        format!(
            "{} {}{}  {}",
            m.index,
            m.name,
            if m.you { " (you)" } else { "" },
            theme::dim(format!(
                "account {} · keeper {}",
                format::short(&m.account),
                format::short(&m.identity)
            ))
        )
    }));
    ctx.section(&format!("Lobby · {}", lobby.name), &lines);
}

// Founding.

#[expect(clippy::too_many_lines, reason = "a guided flow reads top to bottom")]
async fn create(ctx: &Ctx, args: CreateArgs) -> Result<()> {
    let client = ctx.client().await?;
    ctx.intro("Create a vault");
    let open = client
        .call::<LobbyList>(&Empty {})
        .await?
        .into_iter()
        .find(|l| l.role == LobbyRole::Host && l.status == LobbyStatus::Open);
    let resume = match &open {
        Some(lobby) if args.name.is_none() && ctx.interactive => prompt::confirm(
            ctx,
            &format!("You were creating {}. Continue with it?", lobby.name),
            true,
        )?,
        _ => false,
    };
    let (lobby, total) = if let (true, Some(lobby)) = (resume, open) {
        let total = lobby.expected.max(2);
        (lobby, total)
    } else {
        let name = prompt::text(ctx, args.name, "--name", "Name", None, |v| {
            if (1..=48).contains(&v.trim().chars().count()) {
                Ok(())
            } else {
                Err("Use 1 to 48 characters.".into())
            }
        })?;
        let total = prompt::number(
            ctx,
            args.members,
            "--members",
            "How many members found it, you included?",
            3,
            2..=8,
        )?;
        let approvals = prompt::number(
            ctx,
            args.approvals,
            "--approvals",
            "Approvals needed to spend (k)",
            total.min(2),
            1..=total,
        )?;
        let readers = prompt::number(
            ctx,
            args.readers,
            "--readers",
            "Members needed to read (t_v)",
            total.min(2),
            2..=total,
        )?;
        ctx.info(&format!(
            "Any {readers} members together can read everything the vault holds; fewer cannot read anything. \
             A change of members can remove at most {} at a time.",
            readers - 1
        ));
        accept_risks(
            ctx,
            u32::from(approvals),
            u32::from(readers),
            u32::from(total),
        )?;
        let lobby = client
            .call::<LobbyCreate>(&LobbyCreateParams {
                name: name.trim().to_owned(),
                threshold: approvals,
                viewing_threshold: readers,
                members: total,
            })
            .await?;
        (lobby, total)
    };
    let invite = lobby
        .invite
        .clone()
        .ok_or_else(|| Error::failed("The daemon made no invite."))?;
    show_invite(ctx, &invite);

    let mut step = Step::start(ctx, "Waiting for co-founders");
    let mut seen: HashSet<String> = lobby.members.iter().map(|m| m.name.clone()).collect();
    let mut shown = invite;
    let lobby = loop {
        let view = client
            .call::<LobbyGet>(&LobbyParams { lobby: lobby.lobby })
            .await?;
        // A removal rotates the invite: the old one stops working.
        if let Some(fresh) = view.invite.as_ref().filter(|i| **i != shown) {
            step.clear();
            ctx.warn(
                "The invite changed after a removal: send the new one to anyone still to join.",
            );
            show_invite(ctx, fresh);
            shown.clone_from(fresh);
            step = Step::start(ctx, "Waiting for co-founders");
        }
        let total = view.expected.max(total);
        for member in &view.members {
            if seen.insert(member.name.clone()) {
                step.clear();
                ctx.success(&format!("{} joined", member.name));
                step = Step::start(ctx, "Waiting for co-founders");
            }
        }
        let joined = view.members.len();
        step.message(&format!(
            "Waiting for co-founders · {} of {} joined",
            joined.saturating_sub(1),
            usize::from(total) - 1
        ));
        if joined >= usize::from(total) {
            step.done(&format!("Everyone is here: {}", names(&view)));
            break view;
        }
        tokio::time::sleep(POLL).await;
    };
    client
        .call::<LobbyStart>(&StartLobbyParams {
            lobby: lobby.lobby,
            threshold: lobby.threshold,
            viewing_threshold: lobby.viewing_threshold,
        })
        .await?;
    found(ctx, client, &lobby).await
}

/// Shows the thresholds' risks; the ones that can strand the funds for good need a deliberate yes (`--yes` in scripts).
pub(crate) fn accept_risks(
    ctx: &Ctx,
    threshold: u32,
    viewing_threshold: u32,
    members: u32,
) -> Result<()> {
    let risks = describe::risks(threshold, viewing_threshold, members);
    for risk in &risks {
        ctx.warn(&format!(
            "{}{}",
            if risk.severe { "No spare: " } else { "" },
            risk.text
        ));
    }
    if risks.iter().any(|r| r.severe) {
        let accepted = prompt::confirm(ctx, "Accept this risk?", false).map_err(|e| {
            e.hint("Choose more members or lower thresholds, or pass --yes to accept the risk.")
        })?;
        if !accepted {
            return Err(Error::declined().hint(
                "More members, or lower thresholds, leave a spare: see `architecture/recovery.md`.",
            ));
        }
    }
    Ok(())
}

fn names(lobby: &LobbyView) -> String {
    lobby
        .members
        .iter()
        .map(|m| {
            if m.you {
                format!("{} (you)", m.name)
            } else {
                m.name.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn show_invite(ctx: &Ctx, invite: &str) {
    if !ctx.human() {
        eprintln!("invite: {invite}");
        return;
    }
    ctx.section(
        "Invite ready: send this line to each co-founder",
        &[format!(
            "They run {}",
            theme::name("ome vault join <invite>")
        )],
    );
    // On its own line, unindented, so it copies whole however the terminal wraps it.
    eprintln!("{}\n", theme::name(invite));
}

async fn join(ctx: &Ctx, invite: Option<String>) -> Result<()> {
    let client = ctx.client().await?;
    ctx.intro("Join a vault");
    let invite = prompt::text(ctx, invite, "<invite>", "The invite", None, |v| {
        if v.trim().starts_with("ome-invite-v") {
            Ok(())
        } else {
            Err("An invite starts with ome-invite-v0:".into())
        }
    })?;
    let invite = invite.trim().to_owned();
    let preview = client
        .call::<LobbyInspect>(&InviteParams {
            invite: invite.clone(),
        })
        .await?;
    let network = describe::network_id(&preview.network);
    ctx.section(
        &format!(
            "{} invites you to found {}",
            preview.host_name,
            theme::bold(&preview.name)
        ),
        &[
            format!(
                "{} approvals to spend · {} members to read",
                preview.threshold, preview.viewing_threshold
            ),
            format!("on {network}"),
            theme::dim(format!(
                "host keeper {}",
                format::short(&preview.host.identity)
            ))
            .to_string(),
        ],
    );
    if !preview.same_network {
        return Err(Error::usage(format!(
            "This invite is for {network}, and your daemon is on another network."
        ))
        .hint(format!(
            "Point it at the same node: `{}`.",
            ctx.ome("config set node-url <url>")
        )));
    }
    let existing = client
        .call::<LobbyList>(&Empty {})
        .await?
        .into_iter()
        .find(|l| l.lobby == preview.lobby && l.status != LobbyStatus::Closed);
    let lobby = if let Some(lobby) = existing {
        lobby
    } else {
        prompt::proceed(ctx, &format!("Join {}?", preview.name))?;
        client.call::<LobbyJoin>(&InviteParams { invite }).await?
    };
    let step = Step::start(
        ctx,
        &format!("Waiting for {} to start the founding", preview.host_name),
    );
    let lobby = loop {
        let view = client
            .call::<LobbyGet>(&LobbyParams { lobby: lobby.lobby })
            .await?;
        match view.status {
            LobbyStatus::Started => {
                step.done(&format!("Founding with {}", names(&view)));
                break view;
            }
            LobbyStatus::Closed => {
                step.fail("The lobby closed");
                return Err(Error::failed(format!(
                    "{} cancelled the founding.",
                    preview.host_name
                )));
            }
            LobbyStatus::Open => step.message(&format!(
                "Waiting for {} to start · {} here",
                preview.host_name,
                view.members.len()
            )),
        }
        tokio::time::sleep(POLL).await;
    };
    found(ctx, client, &lobby).await
}

fn step_label(id: FoundingStepId) -> (&'static str, &'static str) {
    match id {
        FoundingStepId::Agree => ("agree", "Every member agreed on the plan"),
        FoundingStepId::Dkg => ("dkg", "Generated the viewing key together (DKG)"),
        FoundingStepId::Derive => ("derive", "Derived the vault's keys and address"),
        FoundingStepId::Register => ("register", "Your keeper holds its share"),
        FoundingStepId::Deploy => ("deploy", "Deployed the vault"),
        FoundingStepId::Verify => ("verify", "Checked the deployment against the plan"),
    }
}

fn tracked(state: &FoundingState) -> Vec<Tracked> {
    state
        .steps
        .iter()
        .map(|s| {
            let (key, label) = step_label(s.id);
            Tracked {
                key,
                label: label.to_owned(),
                status: s.status,
                detail: s.detail.clone(),
                took: s
                    .started_at
                    .zip(s.finished_at)
                    .map(|(a, b)| b.saturating_sub(a)),
            }
        })
        .collect()
}

/// Runs the founding of a started lobby and follows its steps.
async fn found(ctx: &Ctx, client: &DaemonClient, lobby: &LobbyView) -> Result<()> {
    client
        .call::<FoundingBegin>(&LobbyParams { lobby: lobby.lobby })
        .await?;
    let mut tracker = Tracker::default();
    let state = loop {
        if let Some(state) = client
            .call::<FoundingGet>(&LobbyParams { lobby: lobby.lobby })
            .await?
        {
            tracker.update(ctx, tracked(&state));
            if state.done {
                break state;
            }
        }
        tokio::time::sleep(POLL).await;
    };
    tracker.finish();
    if let Some(error) = state.error {
        let (_, label) = step_label(error.step);
        let mut failure = Error::failed(format!("The founding failed: {}", error.message))
            .context(format!("At “{label}”"));
        if !error.blamed.is_empty() {
            failure = failure.hint(format!(
                "Keepers held responsible: {}. Start again with `{}`.",
                error.blamed.join(", "),
                ctx.ome("vault create")
            ));
        }
        return Err(failure);
    }
    let vault = state
        .vault
        .ok_or_else(|| Error::failed("The founding finished without a vault."))?;
    let summary = client
        .call::<VaultList>(&Empty {})
        .await?
        .into_iter()
        .find(|v| v.address == vault);
    if ctx.mode == Mode::Json {
        return ctx.json(&summary);
    }
    if ctx.quiet {
        ctx.out(&vault.to_string());
    }
    ctx.outro(&format!(
        "{} is founded  {}",
        theme::bold(&lobby.name),
        theme::dim(vault.to_string())
    ));
    ctx.next("vault receive");
    Ok(())
}

// Inspecting.

async fn list(ctx: &Ctx) -> Result<()> {
    let vaults = refs::vaults(ctx).await?;
    if ctx.mode == Mode::Json {
        return ctx.json(&vaults);
    }
    if vaults.is_empty() {
        ctx.info("No vaults yet.");
        ctx.next("vault create");
        return Ok(());
    }
    let mut t = table::Table::new(&["", "vault", "alias", "spend", "read", "you", "address"]);
    for v in &vaults {
        t.row(vec![
            table::toned(if v.is_default { "●" } else { "" }, Tone::Ok),
            table::toned(&v.name, Tone::Bold),
            table::toned(v.alias.as_deref().unwrap_or(""), Tone::Name),
            table::cell(format!("{} of {}", v.threshold, v.members)),
            table::cell(format!("{} of {}", v.viewing_threshold, v.members)),
            table::cell(format!("#{}", v.my_index)),
            table::toned(address(ctx, &v.address), Tone::Dim),
        ]);
    }
    t.print(ctx);
    Ok(())
}

fn address(ctx: &Ctx, address: &Hex32) -> String {
    if ctx.verbose || ctx.mode == Mode::Plain {
        address.to_string()
    } else {
        format::short(address)
    }
}

async fn show(ctx: &Ctx, target: Option<String>) -> Result<()> {
    let client = ctx.client().await?;
    let vaults = refs::vaults(ctx).await?;
    let vault = refs::vault_in(ctx, &vaults, target.as_deref().or(ctx.vault.as_deref()))?;
    let details = client
        .call::<VaultDetailsGet>(&VaultParams {
            vault: vault.address,
        })
        .await?;
    if ctx.mode == Mode::Json {
        return ctx.json(&details);
    }
    let s = &details.summary;
    ctx.intro(&format!(
        "{} · {}",
        s.name,
        describe::shape(s.threshold, s.viewing_threshold, s.members)
    ));
    let mut about = vec![
        format!("address   {}", s.address),
        format!("founded   {}", format::relative(s.founded_at)),
        format!("you are   member #{}", s.my_index),
    ];
    if let Some(alias) = &s.alias {
        about.push(format!("alias     {}", theme::name(alias)));
    }
    ctx.section("Vault", &about);
    match &details.config {
        Some(c) => ctx.section(
            "On-chain config",
            &[
                format!(
                    "version {} · {} of {} to spend · {} to read",
                    c.version, c.threshold, c.signer_count, c.viewing_threshold
                ),
                format!(
                    "{} it matches the committee your keeper serves",
                    theme::check(c.matches_committee)
                ),
                theme::dim(format!(
                    "signers {} · committee {}",
                    format::short(&c.signer_set_commitment),
                    format::short(&c.committee_commitment)
                ))
                .to_string(),
            ],
        ),
        None => ctx.warn("The on-chain config has not been read yet."),
    }
    let c = &details.committee;
    let mut committee = vec![format!(
        "epoch {} · {} · your keeper is {}",
        c.epoch,
        if c.serving {
            theme::ok("serving its share").to_string()
        } else {
            theme::warn("not serving").to_string()
        },
        c.keeper_status
    )];
    committee.extend(c.members.iter().map(|m| {
        format!(
            "#{} {}{}  {}",
            m.index,
            m.name,
            if m.you {
                theme::dim(" (you)").to_string()
            } else {
                String::new()
            },
            theme::dim(format!(
                "key {} · keeper {}",
                format::short_point(&m.signer_key),
                format::short(&m.identity)
            ))
        )
    }));
    ctx.section("Members and viewing committee", &committee);
    let k = &details.contract;
    let mut contract = vec![
        format!("class     {}", address(ctx, &k.class_id)),
        format!("salt      {}", address(ctx, &k.salt)),
        format!("deployer  {}", address(ctx, &k.deployer)),
    ];
    if let Some(tx) = details.founding.deploy_tx {
        contract.push(format!("deployed  {}", address(ctx, &tx)));
    }
    ctx.section("Contract", &contract);
    Ok(())
}

async fn use_vault(ctx: &Ctx, target: Option<String>) -> Result<()> {
    let client = ctx.client().await?;
    let vaults = refs::vaults(ctx).await?;
    let input = target.or_else(|| ctx.vault.clone());
    let vault = if let Some(input) = input.as_deref() {
        refs::vault_in(ctx, &vaults, Some(input))?
    } else {
        let address = prompt::select(
            ctx,
            "<vault>",
            "Which vault should commands use?",
            vaults
                .iter()
                .map(|v| (v.address, v.name.clone(), format::short(&v.address)))
                .collect(),
        )?;
        refs::vault_in(ctx, &vaults, Some(&address.to_string()))?
    };
    let vaults = client
        .call::<VaultUse>(&VaultParams {
            vault: vault.address,
        })
        .await?;
    if ctx.mode == Mode::Json {
        return ctx.json(&vaults);
    }
    ctx.success(&format!("Commands now act on {}", theme::bold(&vault.name)));
    Ok(())
}

async fn set_alias(ctx: &Ctx, alias: Option<String>) -> Result<()> {
    let vault = refs::vault(ctx).await?;
    let alias = prompt::text(
        ctx,
        alias,
        "<alias>",
        &format!("A short name for {}", vault.name),
        None,
        |v| {
            let ok = !v.is_empty()
                && v.len() <= 32
                && v.bytes().all(|b| {
                    b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_'
                });
            if ok {
                Ok(())
            } else {
                Err("Lowercase letters, digits, - and _, at most 32.".into())
            }
        },
    )?;
    let client = ctx.client().await?;
    let vaults = client
        .call::<VaultAlias>(&AliasParams {
            vault: vault.address,
            alias: alias.clone(),
        })
        .await?;
    if ctx.mode == Mode::Json {
        return ctx.json(&vaults);
    }
    let sync = client.call::<ConfigGet>(&Empty {}).await?.aztec_wallet_sync;
    ctx.success(&format!(
        "{} is now {}",
        vault.name,
        theme::name(format!("vault:{alias}"))
    ));
    if sync {
        ctx.info(&format!(
            "aztec-wallet can pay it as {}",
            theme::name(format!("accounts:{alias}"))
        ));
    }
    Ok(())
}

async fn members(ctx: &Ctx) -> Result<()> {
    let vault = refs::vault(ctx).await?;
    let details = ctx
        .client()
        .await?
        .call::<VaultDetailsGet>(&VaultParams {
            vault: vault.address,
        })
        .await?;
    if ctx.mode == Mode::Json {
        return ctx.json(&details.committee.members);
    }
    let mut t = table::Table::new(&["#", "member", "account", "signing key", "keeper", "release"]);
    for m in &details.committee.members {
        t.row(vec![
            table::cell(m.index.to_string()),
            table::toned(
                if m.you {
                    format!("{} (you)", m.name)
                } else {
                    m.name.clone()
                },
                Tone::Bold,
            ),
            table::toned(
                m.account
                    .as_ref()
                    .map_or_else(|| "unknown".to_owned(), |a| address(ctx, a)),
                Tone::Dim,
            ),
            table::toned(
                if ctx.verbose || ctx.mode == Mode::Plain {
                    format::point(&m.signer_key)
                } else {
                    format::short_point(&m.signer_key)
                },
                Tone::Dim,
            ),
            table::toned(address(ctx, &m.identity), Tone::Dim),
            table::toned(m.release.as_deref().unwrap_or("not yet known"), Tone::Dim),
        ]);
    }
    t.print(ctx);
    let mut releases: Vec<&str> = details
        .committee
        .members
        .iter()
        .filter_map(|m| m.release.as_deref())
        .collect();
    releases.sort_unstable();
    releases.dedup();
    if releases.len() > 1 {
        ctx.info(&format!(
            "Members run different releases; the vault speaks protocol v{}, which all of them read.",
            details.committee.protocol_version
        ));
    }
    Ok(())
}

async fn peers(ctx: &Ctx) -> Result<()> {
    let vault = refs::vault(ctx).await?;
    let step = Step::start(ctx, "Reaching the members' keepers");
    let peers = ctx
        .client()
        .await?
        .call::<VaultPeers>(&VaultParams {
            vault: vault.address,
        })
        .await?;
    step.clear();
    if ctx.mode == Mode::Json {
        return ctx.json(&peers);
    }
    let mut t = table::Table::new(&["#", "member", "keeper", "round trip", "last seen"]);
    for p in &peers {
        let (state, tone) = match (p.you, p.connected, p.relayed) {
            (true, ..) => ("this device", Tone::Dim),
            (_, true, false) => ("connected", Tone::Ok),
            (_, true, true) => ("through a relay", Tone::Ok),
            _ => ("unreachable", Tone::Bad),
        };
        t.row(vec![
            table::cell(p.index.to_string()),
            table::toned(&p.name, Tone::Bold),
            table::toned(state, tone),
            table::cell(p.rtt_ms.map(|ms| format!("{ms} ms")).unwrap_or_default()),
            table::toned(
                p.last_seen.map(format::relative).unwrap_or_default(),
                Tone::Dim,
            ),
        ]);
    }
    t.print(ctx);
    let reachable = peers.iter().filter(|p| p.you || p.connected).count();
    if reachable < usize::try_from(vault.viewing_threshold).unwrap_or(usize::MAX) {
        ctx.warn(&format!(
            "Only {reachable} keepers are reachable; reading needs {}.",
            vault.viewing_threshold
        ));
    }
    Ok(())
}

/// How far the vault is from unreadable: reading needs `t_v` keepers, and a vault that cannot read cannot find its
/// notes to spend them either. Exits 4 when it cannot read now.
async fn health(ctx: &Ctx) -> Result<()> {
    let vault = refs::vault(ctx).await?;
    let client = ctx.client().await?;
    let step = Step::start(ctx, "Reaching the members' keepers");
    let params = VaultParams {
        vault: vault.address,
    };
    let (details, peers) = tokio::join!(
        client.call::<VaultDetailsGet>(&params),
        client.call::<VaultPeers>(&params)
    );
    let (details, peers) = (details?, peers?);
    step.clear();
    let needed = details.committee.viewing_threshold;
    let answering = u32::try_from(
        peers
            .iter()
            .filter(|p| p.connected || (p.you && details.committee.serving))
            .count(),
    )
    .unwrap_or(0);
    let spare = answering.saturating_sub(needed);
    let config_matches = details.config.as_ref().is_some_and(|c| c.matches_committee);
    let risks = describe::risks(vault.threshold, needed, vault.members);
    let can_read = answering >= needed;
    if ctx.mode == Mode::Json {
        ctx.json(&serde_json::json!({
            "vault": vault.address,
            "viewingThreshold": needed,
            "members": vault.members,
            "answering": answering,
            "spare": spare,
            "canRead": can_read,
            "serving": details.committee.serving,
            "keeperStatus": details.committee.keeper_status,
            "configMatches": config_matches,
            "risks": risks.iter().map(|r| serde_json::json!({ "severe": r.severe, "text": r.text })).collect::<Vec<_>>(),
        }))?;
    } else {
        show_health(ctx, &vault, &details, (answering, needed, spare), &risks);
    }
    if can_read {
        Ok(())
    } else {
        Err(Error::new(
            crate::error::Exit::NotReady,
            format!(
                "{} cannot read now: {answering} of {needed} keepers answer.",
                vault.name
            ),
        ))
    }
}

/// The health report on the rail.
fn show_health(
    ctx: &Ctx,
    vault: &ome_api::daemon::VaultSummary,
    details: &ome_api::daemon::VaultDetails,
    (answering, needed, spare): (u32, u32, u32),
    risks: &[describe::Risk],
) {
    let can_read = answering >= needed;
    ctx.intro(&format!("Health of {}", vault.name));
    let reading = if can_read {
        format!(
            "{} {answering} of {} keepers answer; reading needs {needed}, so {} more can be lost",
            theme::check(spare > 0),
            vault.members,
            format::plural(spare, "keeper", "keepers")
        )
    } else {
        format!(
            "{} only {answering} of {} keepers answer; reading needs {needed}: nobody can read or spend until \
             another is back",
            theme::check(false),
            vault.members
        )
    };
    ctx.section("Reading", &[reading]);
    let keeper = if details.committee.serving {
        format!(
            "{} holds its share (epoch {})",
            theme::check(true),
            details.committee.epoch
        )
    } else {
        format!(
            "{} does not hold a share ({}): ask a member to re-add you (below)",
            theme::check(false),
            details.committee.keeper_status
        )
    };
    let config = match &details.config {
        Some(c) if c.matches_committee => format!(
            "{} the on-chain config (version {}) names this committee",
            theme::check(true),
            c.version
        ),
        Some(_) => format!(
            "{} the on-chain config names another committee: see `{}`",
            theme::check(false),
            ctx.ome("member status")
        ),
        None => format!(
            "{} the on-chain config has not been read yet",
            theme::caution()
        ),
    };
    ctx.section("Your keeper", &[keeper, config]);
    if !risks.is_empty() {
        ctx.section(
            "Thresholds",
            &risks
                .iter()
                .map(|r| {
                    format!(
                        "{} {}",
                        if r.severe {
                            theme::bad("✗")
                        } else {
                            theme::caution()
                        },
                        r.text
                    )
                })
                .collect::<Vec<_>>(),
        );
    }
    ctx.section(
        "If a keeper is lost for good",
        &[
            format!(
                "While {needed} keepers still answer, the others reshare to a replacement: the member runs {} on \
                 the new device,",
                theme::name("ome member request")
            ),
            format!(
                "and anyone runs {}.",
                theme::name("ome member change --remove <member> --add <request>")
            ),
            theme::dim("Viewing shares are never backed up: a reshare is the only way back. See architecture/recovery.md.")
                .to_string(),
        ],
    );
}

async fn audit(ctx: &Ctx, limit: u32) -> Result<()> {
    let vault = refs::vault(ctx).await?;
    let entries = lists::audit(
        ctx.client().await?,
        vault.address,
        usize::try_from(limit.clamp(1, 10_000)).unwrap_or(500),
    )
    .await?;
    if ctx.mode == Mode::Json {
        return ctx.json(&entries);
    }
    if entries.is_empty() {
        ctx.info("Your keeper has answered no threshold reads yet.");
        return Ok(());
    }
    let mut t = table::Table::new(&["when", "asked by", "for", "points", "answer"]);
    for e in &entries {
        let who = e.requester_name.clone().unwrap_or_else(|| {
            if e.requester == "local" {
                "you".into()
            } else {
                format::short_str(&e.requester)
            }
        });
        t.row(vec![
            table::toned(format::relative(e.at), Tone::Dim),
            table::cell(who),
            table::cell(&e.purpose),
            table::cell(e.points.to_string()),
            table::toned(
                &e.outcome,
                if e.outcome == "served" {
                    Tone::Ok
                } else {
                    Tone::Warn
                },
            ),
        ]);
    }
    t.print(ctx);
    Ok(())
}

async fn receive(ctx: &Ctx) -> Result<()> {
    let vault = refs::vault(ctx).await?;
    if ctx.mode == Mode::Json {
        return ctx.json(&serde_json::json!({ "vault": vault.address, "name": vault.name }));
    }
    if !ctx.human() {
        ctx.out(&vault.address.to_string());
        return Ok(());
    }
    let client = ctx.client().await?;
    let tokens = client
        .call::<TokensList>(&VaultParams {
            vault: vault.address,
        })
        .await?;
    let token = tokens.first().map_or_else(
        || "contracts:<token>".to_owned(),
        |t| t.token.address.to_string(),
    );
    // aztec-wallet knows the vault by its alias only when aliases are shared with it.
    let sync = client.call::<ConfigGet>(&Empty {}).await?.aztec_wallet_sync;
    let to = match &vault.alias {
        Some(alias) if sync => format!("accounts:{alias}"),
        _ => vault.address.to_string(),
    };
    let mut lines = vec![theme::bold(vault.address.to_string()).to_string()];
    if let Some(code) = qr::render(&vault.address.to_string()) {
        lines.extend(code.lines().map(String::from));
    }
    ctx.section(&format!("Pay {} privately", vault.name), &lines);
    ctx.section(
        "From aztec-wallet",
        &[
            theme::name(format!(
                "aztec-wallet send transfer_in_private -ca {token} --args accounts:<you> {to} <amount> 0 -f <you>"
            ))
            .to_string(),
            theme::dim("Only private transfers keep the vault's balance private.").to_string(),
        ],
    );
    let receiving = client
        .call::<VaultReceiving>(&VaultParams {
            vault: vault.address,
        })
        .await?;
    let mut finds = vec![format!(
        "{} payments from any Aztec wallet that handshakes with the vault (its default)",
        theme::check(receiving.handshakes)
    )];
    finds.push(format!(
        "{} payments from the {} registered from aztec-wallet",
        theme::check(true),
        format::plural(receiving.senders, "account", "accounts")
    ));
    finds.push(match receiving.last_received_at {
        Some(at) => format!("last payment {}", format::relative(at)),
        None => theme::dim("no payment received yet").to_string(),
    });
    ctx.section("What the vault finds", &finds);
    ctx.out(&vault.address.to_string());
    ctx.next("request <amount> <token>");
    Ok(())
}

/// `ome request`: an `ome-pay:` link and QR code naming the vault and what is asked for.
pub(crate) async fn request(ctx: &Ctx, args: RequestArgs) -> Result<()> {
    let vault = refs::vault(ctx).await?;
    let client = ctx.client().await?;
    let token = match &args.token {
        Some(input) => Some(refs::token(ctx, vault.address, input).await?),
        None if args.amount.is_some() => {
            let tracked = client
                .call::<TokensList>(&VaultParams {
                    vault: vault.address,
                })
                .await?;
            match tracked.as_slice() {
                [only] => Some(only.token.clone()),
                _ => {
                    return Err(Error::usage("Name the token the amount is in.")
                        .hint(format!("For example `{}`.", ctx.ome("request 25 USDC"))));
                }
            }
        }
        None => None,
    };
    let amount = match (&args.amount, &token) {
        (Some(amount), Some(token)) => Some(Amount(format::parse_amount(amount, token.decimals)?)),
        _ => None,
    };
    let request = client
        .call::<VaultRequest>(&PaymentRequestParams {
            vault: vault.address,
            token: token.as_ref().map(|t| t.address),
            amount,
            memo: args.memo.clone(),
        })
        .await?;
    if ctx.mode == Mode::Json {
        return ctx.json(&request);
    }
    if !ctx.human() {
        ctx.out(&request.uri);
        return Ok(());
    }
    let asked = match (&request.amount, &request.token) {
        (Some(a), Some(t)) => format!("{} {}", format::amount(a.0, t.decimals), t.symbol),
        (None, Some(t)) => format!("any amount of {}", t.symbol),
        _ => "any amount".to_owned(),
    };
    let mut lines = vec![
        format!("{asked} to {}", vault.name),
        theme::name(&request.uri).to_string(),
    ];
    if let Some(code) = qr::render(&request.uri) {
        lines.extend(code.lines().map(String::from));
    }
    ctx.section("Payment request", &lines);
    ctx.out(&request.uri);
    Ok(())
}

async fn retire(ctx: &Ctx, confirm: Option<&str>) -> Result<()> {
    let vault = refs::vault(ctx).await?;
    ctx.warn(&format!(
        "Retiring destroys the viewing share your keeper kept for {}. Do it only after a change of members removed \
         you and your keeper could not verify the new committee.",
        vault.name
    ));
    prompt::typed(
        ctx,
        &vault.name,
        confirm,
        &format!("Retiring {}", vault.name),
    )?;
    let vaults = ctx
        .client()
        .await?
        .call::<VaultRetire>(&VaultParams {
            vault: vault.address,
        })
        .await?;
    if ctx.mode == Mode::Json {
        return ctx.json(&vaults);
    }
    ctx.success(&format!("{} retired: its share is gone", vault.name));
    Ok(())
}

// Holdings and history.

pub(crate) async fn balance(ctx: &Ctx, args: BalanceArgs) -> Result<()> {
    let vault = refs::vault(ctx).await?;
    let step = Step::start(ctx, "Reading the vault's notes");
    let mut balances = ctx
        .client()
        .await?
        .call::<TokensList>(&VaultParams {
            vault: vault.address,
        })
        .await?;
    step.clear();
    if let Some(input) = &args.token {
        let token = refs::token(ctx, vault.address, input).await?;
        balances.retain(|b| b.token.address == token.address);
    }
    if ctx.mode == Mode::Json {
        return ctx.json(&balances);
    }
    if balances.is_empty() {
        ctx.info(&format!("{} tracks no tokens yet.", vault.name));
        ctx.next("token add <address>");
        return Ok(());
    }
    let mut t = table::Table::new(&["token", "private", "public", "address"]);
    for b in &balances {
        let (private, private_tone) = describe::balance_state(&b.private, b.token.decimals);
        let (public, public_tone) = describe::balance_state(&b.public, b.token.decimals);
        let symbol = if b.token.standard {
            b.token.symbol.clone()
        } else {
            format!("{} (custom)", b.token.symbol)
        };
        t.row(vec![
            table::toned(symbol, Tone::Name),
            table::toned(private, private_tone),
            table::toned(public, public_tone),
            table::toned(address(ctx, &b.token.address), Tone::Dim),
        ]);
    }
    t.print(ctx);
    if balances
        .iter()
        .any(|b| matches!(b.private, ome_api::daemon::BalanceState::Unreadable { .. }))
    {
        ctx.warn("Too few keepers answered to read the private balance: members' keepers may be offline.");
    }
    Ok(())
}

fn entry_key(entry: &ActivityEntry) -> String {
    match entry {
        ActivityEntry::Proposal { proposal, .. } => format!(
            "{}:{:?}:{}:{}",
            proposal.id,
            proposal.status,
            proposal.approvals().count(),
            proposal.rejections().count()
        ),
        ActivityEntry::Received { tx_hash, token, .. } => format!("{tx_hash}:{}", token.address),
    }
}

fn entry_row(ctx: &Ctx, entry: &ActivityEntry, labels: &refs::Book) -> Vec<(String, Tone)> {
    match entry {
        ActivityEntry::Proposal { at, proposal } => {
            let (status, tone) = describe::status(proposal);
            vec![
                table::toned(format::relative(*at), Tone::Dim),
                table::toned(format::id(&proposal.id), Tone::Name),
                table::cell(describe::intent_with(proposal, &|a| labels.label(a))),
                table::toned(status, tone),
            ]
        }
        ActivityEntry::Received {
            at,
            token,
            from,
            amount,
            block,
            ..
        } => vec![
            table::toned(format::relative(*at), Tone::Dim),
            table::toned(
                format!("block {}", format::group_thousands(&block.to_string())),
                Tone::Dim,
            ),
            table::cell(format!(
                "received {} {} from {}",
                format::amount(amount.0, token.decimals),
                token.symbol,
                labels.label(from).unwrap_or_else(|| address(ctx, from))
            )),
            table::toned("received", Tone::Ok),
        ],
    }
}

/// Entries `activity --follow` rereads on each event.
const FOLLOW_PAGE: usize = 50;

pub(crate) async fn activity(ctx: &Ctx, args: ActivityArgs) -> Result<()> {
    let vault = refs::vault(ctx).await?;
    let client = ctx.client().await?;
    let book = refs::Book::load(ctx).await?;
    let entries = lists::activity(client, vault.address, Some(args.limit)).await?;
    if ctx.mode == Mode::Json && !args.follow {
        return ctx.json(&entries);
    }
    let mut t = table::Table::new(&["when", "id", "what", "status"]);
    for e in entries.iter().rev() {
        t.row(entry_row(ctx, e, &book));
    }
    if t.is_empty() && !args.follow {
        ctx.info("Nothing has happened yet.");
        return Ok(());
    }
    t.print(ctx);
    if !args.follow {
        return Ok(());
    }
    let mut seen: HashSet<String> = entries.iter().map(entry_key).collect();
    let mut events = client.events().await?;
    if ctx.human() {
        eprintln!("{}", theme::dim("following; Ctrl-C to stop"));
    }
    loop {
        let refresh = tokio::select! {
            event = events.next() => match event.map(|e| e.map(|e| e.event)) {
                Some(Ok(DaemonEvent::HandshakeAdvised { vault: v, counterparties })) if v == vault.address => {
                    ctx.warn(&format!("{counterparties} counterparties should handshake again: `{}`", ctx.ome("counterparties")));
                    false
                }
                Some(Ok(ref event)) if event.vault() == Some(&vault.address) && describe::lifecycle_notice(event).is_some() => {
                    if ctx.human() && let Some(notice) = describe::lifecycle_notice(event) {
                        ctx.info(&notice);
                    }
                    true
                }
                Some(Ok(event)) if event.vault() == Some(&vault.address) => true,
                Some(_) => false,
                None => return Err(Error::new(crate::error::Exit::Daemon, "The daemon closed the connection.")),
            },
            () = tokio::time::sleep(Duration::from_secs(15)) => true,
        };
        if !refresh {
            continue;
        }
        // New entries are the newest: one page holds them.
        let latest = lists::activity(client, vault.address, Some(FOLLOW_PAGE)).await?;
        for e in latest.iter().rev() {
            if seen.insert(entry_key(e)) {
                if ctx.mode == Mode::Json {
                    let line =
                        serde_json::to_string(e).map_err(|e| Error::failed(e.to_string()))?;
                    ctx.out(&line);
                } else {
                    let mut row = table::Table::new(&["when", "id", "what", "status"]);
                    row.row(entry_row(ctx, e, &book));
                    ctx.out(&row.render(Mode::Plain, false));
                }
            }
        }
    }
}

pub(crate) async fn counterparties(ctx: &Ctx) -> Result<()> {
    let vault = refs::vault(ctx).await?;
    let list = lists::counterparties(ctx.client().await?, vault.address).await?;
    if ctx.mode == Mode::Json {
        return ctx.json(&list);
    }
    if list.is_empty() {
        ctx.info("Nobody has paid the vault yet.");
        return Ok(());
    }
    let mut t = table::Table::new(&["sender", "transfers", "last", "after a removal"]);
    for c in &list {
        t.row(vec![
            table::cell(c.label.clone().unwrap_or_else(|| address(ctx, &c.address))),
            table::cell(c.received.to_string()),
            table::toned(format::relative(c.last_at), Tone::Dim),
            if c.exposed {
                table::toned("should handshake again", Tone::Warn)
            } else {
                table::toned("private", Tone::Ok)
            },
        ]);
    }
    t.print(ctx);
    if list.iter().any(|c| c.exposed) {
        ctx.warn(
            "A member removed since can still tell when these senders pay again, until they handshake with the \
             vault anew.",
        );
    }
    Ok(())
}
