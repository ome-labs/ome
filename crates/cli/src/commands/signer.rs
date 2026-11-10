//! `ome unlock`, `ome lock`, `ome signer status|touch-id`. The signer stays sealed in the daemon; unlocking opens
//! it for a window, so approvals in that window need no passphrase.

use ome_api::{
    Empty,
    daemon::{
        ChangePassphraseParams, ConfigGet, Secret, SignerChangePassphrase, SignerLock,
        SignerStatus, SignerStatusGet, SignerUnlock, UnlockParams,
    },
};

use crate::{
    args::{SignerCommand, Toggle, UnlockArgs},
    context::{Ctx, Mode},
    error::{Error, Result},
    secret,
    ui::{format, steps::Step, theme},
};

pub(crate) async fn unlock(ctx: &Ctx, args: UnlockArgs) -> Result<()> {
    let client = ctx.client().await?;
    let longest = client
        .call::<ConfigGet>(&Empty {})
        .await?
        .unlock_timeout_secs;
    let wanted = match &args.duration {
        Some(text) => format::parse_duration(text)?,
        None => 15 * 60,
    };
    if wanted > u64::from(longest) {
        return Err(Error::usage(format!(
            "The longest unlock is {}.",
            format::span(u64::from(longest))
        ))
        .hint(format!(
            "Raise it with `{}`.",
            ctx.ome("config set unlock-timeout 8h")
        )));
    }
    let secret = secret::for_unlock(ctx, args.touch_id)?;
    let step = Step::start(ctx, "Unlocking the signer");
    let status = client
        .call::<SignerUnlock>(&UnlockParams {
            secret,
            ttl_secs: u32::try_from(wanted).unwrap_or(longest),
        })
        .await;
    match status {
        Ok(status) => {
            step.done(&format!("Unlocked for {}", format::span(wanted)));
            show(ctx, status)
        }
        Err(error) => {
            step.fail("The signer stayed locked");
            Err(error.into())
        }
    }
}

pub(crate) async fn lock(ctx: &Ctx) -> Result<()> {
    let status = ctx.client().await?.call::<SignerLock>(&Empty {}).await?;
    ctx.success("Signer locked");
    show(ctx, status)
}

pub(crate) async fn run(ctx: &Ctx, command: SignerCommand) -> Result<()> {
    match command {
        SignerCommand::Status => {
            let status = ctx
                .client()
                .await?
                .call::<SignerStatusGet>(&Empty {})
                .await?;
            if ctx.mode == Mode::Json {
                return ctx.json(&serde_json::json!({
                    "locked": status.locked,
                    "unlockedUntil": status.unlocked_until,
                    "touchId": secret::touch_id_enabled(&ctx.profile),
                }));
            }
            ctx.section(
                "Signer",
                &[
                    describe(status),
                    format!(
                        "Touch ID {}",
                        if secret::touch_id_enabled(&ctx.profile) {
                            "on"
                        } else {
                            "off"
                        }
                    ),
                ],
            );
            Ok(())
        }
        SignerCommand::TouchId { state: Toggle::On } => {
            let client = ctx.client().await?;
            secret::enable_touch_id(ctx, client, None).await?;
            ctx.success(
                "Touch ID is on: approvals ask for your fingerprint instead of the passphrase",
            );
            Ok(())
        }
        SignerCommand::TouchId { state: Toggle::Off } => {
            secret::disable_touch_id(&ctx.profile)?;
            ctx.success("Touch ID is off; the key is gone from the keychain");
            Ok(())
        }
        SignerCommand::Passphrase => change_passphrase(ctx).await,
    }
}

/// `ome signer passphrase`: the current passphrase, then the new one twice. The signing key stays the same; Touch ID,
/// which keeps the old sealing key, is re-enrolled.
async fn change_passphrase(ctx: &Ctx) -> Result<()> {
    let client = ctx.client().await?;
    let current = secret::passphrase(ctx)?;
    let next = secret::replacement_passphrase(ctx)?;
    let step = Step::start(ctx, "Re-sealing your signing key");
    let status = client
        .call::<SignerChangePassphrase>(&ChangePassphraseParams {
            current: Secret::Passphrase {
                passphrase: current.as_str().to_owned(),
            },
            next: next.as_str().to_owned(),
        })
        .await;
    let status = match status {
        Ok(status) => {
            step.done("Re-sealed under the new passphrase");
            status
        }
        Err(error) => {
            step.fail("The passphrase is unchanged");
            return Err(error.into());
        }
    };
    if secret::touch_id_enabled(&ctx.profile) {
        // The keychain holds the old sealing key, which no longer opens anything.
        secret::enable_touch_id(ctx, client, Some(next.as_str())).await?;
    }
    if ctx.mode == Mode::Json {
        return ctx.json(&status);
    }
    ctx.success("Passphrase changed: the old one no longer opens your signing key");
    Ok(())
}

fn describe(status: SignerStatus) -> String {
    match status.unlocked_until {
        Some(until) if !status.locked => format!(
            "{} unlocked for {}",
            theme::ok("●"),
            format::span(until.saturating_sub(format::now()))
        ),
        _ => format!("{} locked", theme::dim("○")),
    }
}

fn show(ctx: &Ctx, status: SignerStatus) -> Result<()> {
    if ctx.mode == Mode::Json {
        return ctx.json(&status);
    }
    if ctx.mode == Mode::Plain {
        ctx.out(if status.locked { "locked" } else { "unlocked" });
    }
    Ok(())
}
