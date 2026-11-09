//! `ome profile list|show|use|remove`. Each profile is one member with its own daemon, under `<home>/profiles/`.

use ome_api::{Empty, daemon::StatusGet};
use ome_client::DaemonClient;

use crate::{
    args::ProfileCommand,
    context::{CURRENT_PROFILE, Ctx, Mode},
    error::{Error, Result},
    launch, secret,
    ui::{format, prompt, steps::Step, table, theme},
};

pub(crate) async fn run(ctx: &mut Ctx, command: ProfileCommand) -> Result<()> {
    match command {
        ProfileCommand::List => list(ctx).await,
        ProfileCommand::Show { name } => show(ctx, name).await,
        ProfileCommand::Use { name } => {
            let name = match name {
                Some(name) => name,
                None => prompt::select(
                    ctx,
                    "<name>",
                    "Which profile should commands use?",
                    crate::context::set_up_profiles(&ctx.home)
                        .into_iter()
                        .map(|(name, member)| {
                            (name.clone(), member.unwrap_or_else(|| name.clone()), name)
                        })
                        .collect(),
                )?,
            };
            use_profile(ctx, &name)
        }
        ProfileCommand::Remove { name, confirm } => remove(ctx, &name, confirm.as_deref()).await,
    }
}

async fn list(ctx: &Ctx) -> Result<()> {
    let dir = ctx.home.root().join("profiles");
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .filter(|e| e.path().is_dir())
                .filter_map(|e| e.file_name().into_string().ok())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    let mut rows = Vec::new();
    for name in &names {
        let Ok(profile) = ctx.home.profile(name) else {
            continue;
        };
        let running = launch::running(&profile).await;
        rows.push(serde_json::json!({
            "name": name,
            "current": name == ctx.profile.name(),
            "member": launch::member_name(&profile),
            "setUp": launch::profile_exists(&profile),
            "daemon": running.map(|i| i.pid),
        }));
    }
    if ctx.mode == Mode::Json {
        return ctx.json(&rows);
    }
    if rows.is_empty() {
        ctx.info("No profiles yet.");
        ctx.next("init");
        return Ok(());
    }
    let mut t = table::Table::new(&["", "profile", "member", "daemon"]);
    for row in &rows {
        t.row(vec![
            table::toned(
                if row["current"] == true { "●" } else { "" },
                table::Tone::Ok,
            ),
            table::toned(row["name"].as_str().unwrap_or_default(), table::Tone::Name),
            table::cell(row["member"].as_str().unwrap_or(if row["setUp"] == true {
                "?"
            } else {
                "not set up"
            })),
            match row["daemon"].as_u64() {
                Some(pid) => table::toned(format!("running · pid {pid}"), table::Tone::Ok),
                None => table::toned("stopped", table::Tone::Dim),
            },
        ]);
    }
    t.print(ctx);
    Ok(())
}

async fn show(ctx: &Ctx, name: Option<String>) -> Result<()> {
    let profile = match name {
        Some(name) => ctx
            .home
            .profile(&name)
            .map_err(|e| Error::usage(e.to_string()))?,
        None => ctx.profile.clone(),
    };
    let status = match DaemonClient::discover(&profile).await {
        Ok(client) => client.call::<StatusGet>(&Empty {}).await.ok(),
        Err(_) => None,
    };
    let member = status.as_ref().and_then(|s| s.profile.clone());
    if ctx.mode == Mode::Json {
        return ctx.json(&serde_json::json!({
            "name": profile.name(),
            "dir": profile.dir(),
            "setUp": launch::profile_exists(&profile),
            "member": member,
            "touchId": secret::touch_id_enabled(&profile),
        }));
    }
    let mut lines = vec![format!(
        "directory  {}",
        theme::dim(profile.dir().display())
    )];
    match &member {
        Some(m) => {
            lines.push(format!("member     {}", theme::bold(&m.name)));
            lines.push(format!("account    {}", m.account));
            lines.push(format!("signer key {}", format::point(&m.signer_key)));
            lines.push(format!("created    {}", format::relative(m.created_at)));
        }
        None if launch::profile_exists(&profile) => {
            lines.push(format!(
                "member     {}",
                launch::member_name(&profile).unwrap_or_else(|| "?".into())
            ));
            lines.push(theme::dim("the daemon is stopped: start it for the rest").to_string());
        }
        None => lines.push(theme::warn("not set up").to_string()),
    }
    lines.push(format!(
        "touch id   {}",
        if secret::touch_id_enabled(&profile) {
            "on"
        } else {
            "off"
        }
    ));
    ctx.section(&format!("Profile {}", profile.name()), &lines);
    Ok(())
}

fn use_profile(ctx: &Ctx, name: &str) -> Result<()> {
    let profile = ctx
        .home
        .profile(name)
        .map_err(|e| Error::usage(e.to_string()))?;
    if !profile.dir().is_dir() {
        return Err(Error::usage(format!("There is no profile {name:?}."))
            .hint(format!("Set it up with `ome --profile {name} init`.")));
    }
    std::fs::create_dir_all(ctx.home.root())?;
    std::fs::write(ctx.home.root().join(CURRENT_PROFILE), format!("{name}\n"))?;
    if ctx.mode == Mode::Json {
        return ctx.json(&serde_json::json!({ "current": name }));
    }
    ctx.success(&format!(
        "Commands now act as profile {}",
        theme::name(name)
    ));
    Ok(())
}

async fn remove(ctx: &Ctx, name: &str, confirm: Option<&str>) -> Result<()> {
    let profile = ctx
        .home
        .profile(name)
        .map_err(|e| Error::usage(e.to_string()))?;
    if !profile.dir().is_dir() {
        return Err(Error::usage(format!("There is no profile {name:?}.")));
    }
    ctx.warn(&format!(
        "Removing {name} deletes its signing key and its keeper's viewing shares. In each of its vaults the other \
         members must then remove it with `ome member change`, or swap its key."
    ));
    prompt::typed(ctx, name, confirm, &format!("Removing profile {name}"))?;
    let step = Step::start(ctx, "Stopping its daemon");
    launch::stop(&profile).await?;
    step.done("Daemon stopped");
    secret::disable_touch_id(&profile)?;
    std::fs::remove_dir_all(profile.dir())?;
    let current = ctx.home.root().join(CURRENT_PROFILE);
    if std::fs::read_to_string(&current).is_ok_and(|c| c.trim() == name) {
        std::fs::remove_file(current)?;
    }
    if ctx.mode == Mode::Json {
        return ctx.json(&serde_json::json!({ "removed": name }));
    }
    ctx.success(&format!("Profile {name} removed"));
    Ok(())
}
