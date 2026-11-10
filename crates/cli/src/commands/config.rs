//! `ome config get|set|unset`: the daemon's settings under the names people type.

use ome_api::{
    Empty, Hex32,
    daemon::{ConfigGet, ConfigSet, RelaysParams, RelaysSet, Settings, SettingsPatch},
};

use crate::{
    args::{ConfigCommand, ConfigKey},
    context::{Ctx, Mode},
    error::{Error, Result},
    ui::{format, table, theme},
};

const KEYS: [ConfigKey; 8] = [
    ConfigKey::NodeUrl,
    ConfigKey::Prover,
    ConfigKey::Relays,
    ConfigKey::FeePayer,
    ConfigKey::UnlockTimeout,
    ConfigKey::AztecWalletSync,
    ConfigKey::AztecWalletDir,
    ConfigKey::Apps,
];

pub(crate) async fn run(ctx: &mut Ctx, command: ConfigCommand) -> Result<()> {
    let client = ctx.client().await?;
    let settings = match command {
        ConfigCommand::Get { key } => {
            let settings = client.call::<ConfigGet>(&Empty {}).await?;
            if let Some(key) = key {
                if ctx.mode == Mode::Json {
                    return ctx.json(&serde_json::json!({ name(key): value(&settings, key) }));
                }
                ctx.out(&value(&settings, key));
                return Ok(());
            }
            settings
        }
        ConfigCommand::Set { key, value } => {
            let before = client.call::<ConfigGet>(&Empty {}).await?.node_url;
            let settings = set(ctx, key, &value).await?;
            if settings.node_url != before {
                // The daemon reads the node URL as it starts.
                ctx.info(&format!(
                    "Restarting the daemon to use {}",
                    settings.node_url
                ));
                ctx.restart_daemon().await?;
            }
            settings
        }
        ConfigCommand::Unset { key } => {
            let default = match key {
                ConfigKey::NodeUrl => {
                    return Err(
                        Error::usage("The node URL has no default to go back to.").hint(format!(
                            "Set it: `{}`.",
                            ctx.ome("config set node-url <url>")
                        )),
                    );
                }
                ConfigKey::Prover => "none",
                ConfigKey::Relays | ConfigKey::AztecWalletDir => "",
                ConfigKey::FeePayer => "0",
                ConfigKey::UnlockTimeout => "1h",
                ConfigKey::AztecWalletSync | ConfigKey::Apps => "off",
            };
            set(ctx, key, default).await?
        }
    };
    if ctx.mode == Mode::Json {
        return ctx.json(&settings);
    }
    let mut rows = table::Table::new(&["setting", "value"]);
    for key in KEYS {
        rows.row(vec![
            table::toned(name(key), table::Tone::Name),
            table::cell(value(&settings, key)),
        ]);
    }
    rows.print(ctx);
    Ok(())
}

async fn set(ctx: &Ctx, key: ConfigKey, value: &str) -> Result<Settings> {
    let client = ctx.client().await?;
    let mut patch = SettingsPatch::default();
    match key {
        ConfigKey::NodeUrl => patch.node_url = Some(value.to_owned()),
        ConfigKey::Prover => {
            patch.prover_enabled = Some(match value {
                "native" | "wasm" => true,
                "none" => false,
                _ => return Err(Error::usage("The prover is native, wasm or none.")),
            });
        }
        ConfigKey::Relays => {
            let relays = value
                .split(',')
                .map(str::trim)
                .filter(|r| !r.is_empty())
                .map(String::from)
                .collect();
            return Ok(client.call::<RelaysSet>(&RelaysParams { relays }).await?);
        }
        ConfigKey::FeePayer => {
            let salt = if value == "0" {
                Hex32([0; 32])
            } else {
                value.parse().map_err(|_| {
                    Error::usage(
                        "The fee payer is a 32-byte salt (0x…), or 0 for the canonical one.",
                    )
                })?
            };
            patch.fee_payer_salt = Some(salt);
        }
        ConfigKey::UnlockTimeout => {
            let secs = format::parse_duration(value)?;
            patch.unlock_timeout_secs =
                Some(u32::try_from(secs).map_err(|_| Error::usage("That is longer than a day."))?);
        }
        ConfigKey::AztecWalletSync => {
            patch.aztec_wallet_sync = Some(match value {
                "on" | "true" | "yes" => true,
                "off" | "false" | "no" => false,
                _ => return Err(Error::usage("aztec-wallet.sync is on or off.")),
            });
        }
        ConfigKey::AztecWalletDir => patch.aztec_wallet_dir = Some(value.to_owned()),
        ConfigKey::Apps => {
            patch.apps_enabled = Some(match value {
                "on" | "true" | "yes" => true,
                "off" | "false" | "no" => false,
                _ => return Err(Error::usage("apps is on or off.")),
            });
        }
    }
    let settings = client.call::<ConfigSet>(&patch).await?;
    if key == ConfigKey::AztecWalletSync && settings.aztec_wallet_sync {
        ctx.info(&format!(
            "Vault aliases now also go into aztec-wallet: `{}` writes {}.",
            ctx.ome("vault alias treasury"),
            theme::name("accounts:treasury")
        ));
    }
    Ok(settings)
}

fn name(key: ConfigKey) -> &'static str {
    match key {
        ConfigKey::NodeUrl => "node-url",
        ConfigKey::Prover => "prover",
        ConfigKey::Relays => "relays",
        ConfigKey::FeePayer => "fee-payer",
        ConfigKey::UnlockTimeout => "unlock-timeout",
        ConfigKey::AztecWalletSync => "aztec-wallet.sync",
        ConfigKey::AztecWalletDir => "aztec-wallet.dir",
        ConfigKey::Apps => "apps",
    }
}

fn value(settings: &Settings, key: ConfigKey) -> String {
    match key {
        ConfigKey::NodeUrl => settings.node_url.clone(),
        ConfigKey::Prover => if settings.prover_enabled {
            "native"
        } else {
            "none"
        }
        .into(),
        ConfigKey::Relays => settings.relays.join(","),
        ConfigKey::FeePayer => {
            if settings.fee_payer_salt.0 == [0; 32] {
                "0 (the canonical SponsoredFPC)".into()
            } else {
                settings.fee_payer_salt.to_string()
            }
        }
        ConfigKey::UnlockTimeout => format::span(u64::from(settings.unlock_timeout_secs)),
        ConfigKey::Apps => if settings.apps_enabled { "on" } else { "off" }.into(),
        ConfigKey::AztecWalletSync => if settings.aztec_wallet_sync {
            "on"
        } else {
            "off"
        }
        .into(),
        ConfigKey::AztecWalletDir => settings
            .aztec_wallet_dir
            .clone()
            .unwrap_or_else(|| "~/.aztec/wallet".into()),
    }
}
