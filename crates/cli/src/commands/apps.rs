//! `ome apps list|allow|revoke`: which apps may use a vault. An allowed app's reads and simulations are answered at
//! once; each transaction it sends becomes a proposal the members review and approve like any other.

use ome_api::{
    Empty,
    daemon::{AppAllowParams, AppParams, AppsAllow, AppsList, AppsRevoke, ConfigGet},
};

use crate::{
    args::AppsCommand,
    context::{Ctx, Mode},
    error::Result,
    refs,
    ui::{
        format, prompt,
        table::{self, Tone},
    },
};

pub(crate) async fn run(ctx: &Ctx, command: AppsCommand) -> Result<()> {
    let client = ctx.client().await?;
    match command {
        AppsCommand::List => {
            let apps = client.call::<AppsList>(&Empty {}).await?;
            if ctx.mode == Mode::Json {
                return ctx.json(&apps);
            }
            if apps.is_empty() {
                ctx.info("No app may use a vault.");
                ctx.next("apps allow https://app.example");
                return Ok(());
            }
            let vaults = refs::vaults(ctx).await?;
            let mut t = table::Table::new(&["app", "name", "vault", "allowed"]);
            for app in &apps {
                let vault = vaults
                    .iter()
                    .find(|v| v.address == app.vault)
                    .map_or_else(|| format::short(&app.vault), |v| v.name.clone());
                t.row(vec![
                    table::toned(&app.origin, Tone::Bold),
                    table::cell(app.name.clone().unwrap_or_default()),
                    table::cell(vault),
                    table::toned(format::relative(app.allowed_at), Tone::Dim),
                ]);
            }
            t.print(ctx);
            Ok(())
        }
        AppsCommand::Allow { origin, name } => {
            let vault = refs::vault(ctx).await?;
            prompt::proceed(
                ctx,
                &format!(
                    "Let {origin} see {} and propose transactions from it?",
                    vault.name
                ),
            )?;
            let app = client
                .call::<AppsAllow>(&AppAllowParams {
                    origin,
                    vault: vault.address,
                    name,
                })
                .await?;
            if ctx.mode == Mode::Json {
                return ctx.json(&app);
            }
            ctx.success(&format!(
                "{} may use {}: its transactions become proposals",
                app.origin, vault.name
            ));
            if !client.call::<ConfigGet>(&Empty {}).await?.apps_enabled {
                ctx.info("App connections are off.");
                ctx.next("config set apps on");
            }
            Ok(())
        }
        AppsCommand::Revoke { origin } => {
            client
                .call::<AppsRevoke>(&AppParams {
                    origin: origin.clone(),
                })
                .await?;
            if ctx.mode == Mode::Json {
                return ctx.json(&serde_json::json!({ "revoked": origin }));
            }
            ctx.success(&format!("{origin} may no longer use a vault"));
            Ok(())
        }
    }
}
