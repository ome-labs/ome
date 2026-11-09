//! `ome device pair|list|enroll|forward|revoke`. Pairing connects a phone; enrolling moves this member's signing key
//! in a vault to it, a separate and visible decision (itself a proposal).

use std::time::Duration;

use ome_api::{
    Empty,
    daemon::{
        DevicesEnroll, DevicesForward, DevicesList, DevicesPair, DevicesRevoke, EnrollParams,
        ProposalRef,
    },
    keeper::DeviceParams,
};

use crate::{
    args::DeviceCommand,
    context::{Ctx, Mode},
    error::{Error, Result},
    refs,
    ui::{
        describe, format, prompt, qr,
        steps::Step,
        table::{self, Tone},
        theme,
    },
};

pub(crate) async fn run(ctx: &Ctx, command: DeviceCommand) -> Result<()> {
    let client = ctx.client().await?;
    match command {
        DeviceCommand::Pair => pair(ctx, client).await,
        DeviceCommand::List => {
            let devices = client.call::<DevicesList>(&Empty {}).await?;
            if ctx.mode == Mode::Json {
                return ctx.json(&devices);
            }
            if devices.is_empty() {
                ctx.info("No devices are paired.");
                ctx.next("device pair");
                return Ok(());
            }
            let mut t = table::Table::new(&["device", "identity", "paired", "holds your key in"]);
            for d in &devices {
                t.row(vec![
                    table::toned(&d.name, Tone::Bold),
                    table::toned(format::short(&d.identity), Tone::Dim),
                    table::toned(format::relative(d.paired_at), Tone::Dim),
                    table::cell(format::plural(
                        u64::try_from(d.vaults.len()).unwrap_or(0),
                        "vault",
                        "vaults",
                    )),
                ]);
            }
            t.print(ctx);
            Ok(())
        }
        DeviceCommand::Enroll { device, confirm } => enroll(ctx, client, device, confirm).await,
        DeviceCommand::Forward(arg) => {
            let vault = refs::vault(ctx).await?;
            let p = refs::proposal(
                ctx,
                vault.address,
                arg.proposal.as_deref(),
                describe::can_vote,
            )
            .await?;
            let sent = client
                .call::<DevicesForward>(&ProposalRef {
                    vault: vault.address,
                    id: p.id,
                })
                .await?;
            if ctx.mode == Mode::Json {
                return ctx.json(&sent);
            }
            ctx.success(&format!(
                "Sent {} to {}",
                format::id(&p.id),
                format::plural(sent.devices, "device", "devices")
            ));
            Ok(())
        }
        DeviceCommand::Revoke { device } => {
            let device = refs::device(ctx, device).await?;
            if !device.vaults.is_empty() {
                ctx.warn(&format!(
                    "{} holds your key in {}: swap it back first (`ome member swap-key`), or approvals there stop.",
                    device.name,
                    format::plural(u64::try_from(device.vaults.len()).unwrap_or(0), "vault", "vaults")
                ));
            }
            prompt::proceed(ctx, &format!("Unpair {}?", device.name))?;
            client
                .call::<DevicesRevoke>(&DeviceParams {
                    identity: device.identity,
                })
                .await?;
            if ctx.mode == Mode::Json {
                return ctx.json(&serde_json::json!({ "revoked": device.identity }));
            }
            ctx.success(&format!("{} is unpaired", device.name));
            Ok(())
        }
    }
}

async fn pair(ctx: &Ctx, client: &ome_client::DaemonClient) -> Result<()> {
    let before = client.call::<DevicesList>(&Empty {}).await?;
    let offer = client.call::<DevicesPair>(&Empty {}).await?;
    if ctx.human() {
        let mut lines = Vec::new();
        if let Some(code) = qr::render(&offer.offer) {
            lines.extend(code.lines().map(String::from));
        }
        lines.push(theme::dim(&offer.offer).to_string());
        lines.push(format!(
            "Scan it with Ome on the phone · expires {}",
            format::relative(offer.expires_at)
        ));
        ctx.section("Pair a phone", &lines);
    } else {
        eprintln!("offer: {}", offer.offer);
    }
    let step = Step::start(ctx, "Waiting for the phone");
    let device = loop {
        if format::now() > offer.expires_at {
            step.fail("The offer expired");
            return Err(Error::failed("No phone answered before the offer expired.")
                .hint(format!("Try again: `{}`.", ctx.ome("device pair"))));
        }
        let now = client.call::<DevicesList>(&Empty {}).await?;
        if let Some(new) = now
            .into_iter()
            .find(|d| !before.iter().any(|b| b.identity == d.identity))
        {
            break new;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    };
    step.done(&format!("Paired {}", theme::bold(&device.name)));
    if ctx.mode == Mode::Json {
        return ctx.json(&device);
    }
    ctx.info("Pairing lets the phone see proposals. Your key moves to it only when you enroll it.");
    ctx.next(&format!("device enroll {}", device.name));
    Ok(())
}

async fn enroll(
    ctx: &Ctx,
    client: &ome_client::DaemonClient,
    device: Option<String>,
    confirm: Option<String>,
) -> Result<()> {
    let vault = refs::vault(ctx).await?;
    let device = refs::device(ctx, device).await?;
    ctx.intro(&format!(
        "Move your signing key in {} to {}",
        vault.name, device.name
    ));
    ctx.section(
        "What happens if it executes",
        &[
            format!("Your approvals in {} come only from {}", vault.name, theme::bold(&device.name)),
            format!("new key  {}", theme::dim(format::point(&device.signer_key))),
            theme::dim("The key on this computer stops counting there. Proposals go to the phone to review.").to_string(),
        ],
    );
    prompt::typed(ctx, &vault.name, confirm.as_deref(), "Moving your key")?;
    let view = client
        .call::<DevicesEnroll>(&EnrollParams {
            vault: vault.address,
            identity: device.identity,
        })
        .await?;
    if ctx.mode == Mode::Json {
        return ctx.json(&view);
    }
    ctx.outro(&format!(
        "Proposed {} · {}",
        theme::name(format::id(&view.id)),
        describe::votes(&view)
    ));
    ctx.next(&format!("approve {}", format::id(&view.id)));
    Ok(())
}
