//! `ome keeper status|connect|use-local|logs`: the member's keeper, which holds their viewing shares. It runs under
//! the daemon, or on the member's own server (`ome-keeper run --rpc-listen`), reached over TLS with a pinned
//! certificate.

use ome_api::{
    Empty,
    daemon::{
        KeeperConnect, KeeperConnectParams, KeeperState, RemoteKeeperParams, ServiceStatus,
        StatusGet,
    },
};
use zeroize::Zeroizing;

use crate::{
    args::KeeperCommand,
    commands::daemon,
    context::{Ctx, Mode},
    error::{Error, Result},
    launch,
    ui::{prompt, steps::Step, theme},
};

pub(crate) async fn run(ctx: &Ctx, command: KeeperCommand) -> Result<()> {
    match command {
        KeeperCommand::Status => {
            let keeper = ctx
                .client()
                .await?
                .call::<StatusGet>(&Empty {})
                .await?
                .keeper;
            show(ctx, &keeper)
        }
        KeeperCommand::Connect {
            address,
            fingerprint,
            token_file,
        } => {
            let address = prompt::text(
                ctx,
                address,
                "<address>",
                "The keeper's host:port",
                None,
                |v| {
                    if v.rsplit_once(':')
                        .is_some_and(|(_, port)| port.parse::<u16>().is_ok())
                    {
                        Ok(())
                    } else {
                        Err("Use host:port.".into())
                    }
                },
            )?;
            let fingerprint = prompt::text(
                ctx,
                fingerprint,
                "--fingerprint",
                "Its certificate fingerprint (ome-keeper prints it at start)",
                None,
                |v| {
                    let hex: String = v.chars().filter(|c| *c != ':').collect();
                    if hex.len() == 64 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
                        Ok(())
                    } else {
                        Err("A SHA-256 fingerprint is 64 hex digits.".into())
                    }
                },
            )?;
            let token = match token_file {
                Some(path) => Zeroizing::new(
                    std::fs::read_to_string(&path)
                        .map_err(|e| {
                            Error::usage(format!("Could not read {}.", path.display())).cause(e)
                        })?
                        .trim()
                        .to_owned(),
                ),
                None if ctx.interactive => {
                    prompt::password("Its RPC token (<data>/rpc.token on the server)")?
                }
                None => return Err(Error::missing("the keeper's RPC token", "--token-file")),
            };
            let step = Step::start(ctx, &format!("Connecting to the keeper at {address}"));
            let keeper = ctx
                .client()
                .await?
                .call::<KeeperConnect>(&KeeperConnectParams {
                    remote: Some(RemoteKeeperParams {
                        address: address.clone(),
                        fingerprint,
                        token: token.as_str().to_owned(),
                    }),
                })
                .await;
            match keeper {
                Ok(keeper) => {
                    step.done(&format!("Using the keeper at {address}"));
                    show(ctx, &keeper)
                }
                Err(error) => {
                    step.fail("Could not connect");
                    Err(error.into())
                }
            }
        }
        KeeperCommand::UseLocal => {
            let step = Step::start(ctx, "Starting the daemon's own keeper");
            let keeper = ctx
                .client()
                .await?
                .call::<KeeperConnect>(&KeeperConnectParams { remote: None })
                .await?;
            step.done("Using the daemon's own keeper");
            show(ctx, &keeper)
        }
        KeeperCommand::Logs(args) => {
            daemon::logs(ctx, &launch::keeper_log_path(&ctx.profile), &args).await
        }
    }
}

fn show(ctx: &Ctx, keeper: &KeeperState) -> Result<()> {
    if ctx.mode == Mode::Json {
        return ctx.json(keeper);
    }
    let state = match keeper.status {
        ServiceStatus::Running => theme::ok("running").to_string(),
        ServiceStatus::Failed => theme::bad("failed").to_string(),
        other => theme::warn(format!("{other:?}").to_lowercase()).to_string(),
    };
    let mut lines = vec![format!(
        "status     {state}{}",
        keeper.remote.as_ref().map_or_else(
            || " · the daemon's own".to_owned(),
            |r| format!(" · on {r}")
        )
    )];
    if let Some(identity) = &keeper.identity {
        lines.push(format!("identity   {identity}"));
    }
    if let Some(peer) = &keeper.peer_id {
        lines.push(format!("peer id    {peer}"));
    }
    for address in &keeper.addresses {
        lines.push(format!("address    {}", theme::dim(address)));
    }
    if let Some(error) = &keeper.error {
        lines.push(format!("{} {error}", theme::bad("✗")));
    }
    ctx.section("Keeper", &lines);
    Ok(())
}
