//! `ome propose transfer|call|cancel|threshold|invalidate`: every proposal is simulated first and shown before it is
//! made; `--dry-run` stops after the simulation.

use ome_api::{
    Amount,
    daemon::ContractFunctionsParams,
    daemon::{
        ContractsFunctions, Intent, IntentCall, ProposalPreview, ProposalsCreate, ProposalsPreview,
        ProposeParams, TokensList, VaultSummary,
    },
    keeper::VaultParams,
};

use crate::{
    args::{ProposeCommand, ProposeOptions},
    context::{Ctx, Mode},
    error::{Error, Result},
    refs::{self, Book},
    ui::{abi, describe, format, prompt, steps::Step, theme},
};

pub(crate) async fn run(ctx: &Ctx, command: ProposeCommand) -> Result<()> {
    let vault = refs::vault(ctx).await?;
    match command {
        ProposeCommand::Transfer {
            amount,
            token,
            to,
            options,
        } => transfer(ctx, &vault, amount, token, to, options).await,
        ProposeCommand::Call {
            function,
            contract,
            args,
            options,
        } => call(ctx, &vault, function, contract, args, options).await,
        ProposeCommand::Threshold { approvals, options } => {
            ctx.intro(&format!("Propose a new threshold · {}", vault.name));
            let intent = Intent::SetThreshold {
                threshold: approvals,
                memo: options.memo.clone(),
            };
            submit(
                ctx,
                &vault,
                intent,
                &options,
                vec![format!(
                    "Spending needs {approvals} approvals instead of {}. Open proposals need their votes again.",
                    vault.threshold
                )],
            )
            .await
        }
        ProposeCommand::Invalidate { options } => {
            ctx.intro(&format!("Propose voiding open approvals · {}", vault.name));
            let intent = Intent::InvalidatePending {
                memo: options.memo.clone(),
            };
            submit(
                ctx,
                &vault,
                intent,
                &options,
                vec![
                    "Every approval collected so far stops counting: open proposals need their votes again."
                        .into(),
                ],
            )
            .await
        }
        ProposeCommand::Cancel { proposal, options } => {
            let target = refs::proposal(ctx, vault.address, proposal.as_deref(), |p| {
                describe::is_open(p.status)
            })
            .await?;
            ctx.intro(&format!(
                "Propose cancelling {} · {}",
                format::id(&target.id),
                vault.name
            ));
            let intent = Intent::Cancel {
                cancels: target.id,
                memo: options.memo.clone(),
            };
            submit(
                ctx,
                &vault,
                intent,
                &options,
                vec![format!(
                    "Cancel {}: {}. Its nonce is spent, so it can never execute.",
                    format::id(&target.id),
                    describe::intent(&target)
                )],
            )
            .await
        }
    }
}

#[expect(clippy::too_many_lines, reason = "a guided flow reads top to bottom")]
async fn transfer(
    ctx: &Ctx,
    vault: &VaultSummary,
    amount: Option<String>,
    token: Option<String>,
    to: Option<String>,
    options: ProposeOptions,
) -> Result<()> {
    ctx.intro(&format!("Propose a payment · {}", vault.name));
    let balances = ctx
        .client()
        .await?
        .call::<TokensList>(&VaultParams {
            vault: vault.address,
        })
        .await?;
    let token = if let Some(input) = token {
        refs::token(ctx, vault.address, &input).await?
    } else {
        if balances.is_empty() {
            return Err(
                Error::usage(format!("{} tracks no tokens.", vault.name)).hint(format!(
                    "Track one with `{}`.",
                    ctx.ome("token add <address>")
                )),
            );
        }
        let address = prompt::select(
            ctx,
            "<token>",
            "Which token?",
            balances
                .iter()
                .map(|b| {
                    let held = b.balance.map_or_else(
                        || "balance unknown".into(),
                        |a| format!("{} held", format::amount(a.0, b.token.decimals)),
                    );
                    (b.token.address, b.token.symbol.clone(), held)
                })
                .collect(),
        )?;
        balances
            .iter()
            .find(|b| b.token.address == address)
            .map(|b| b.token.clone())
            .ok_or_else(|| Error::failed("the token vanished"))?
    };
    let decimals = token.decimals;
    let amount = prompt::text(
        ctx,
        amount,
        "<amount>",
        &format!("How much {}?", token.symbol),
        None,
        move |v| {
            format::parse_amount(v, decimals)
                .map(|_| ())
                .map_err(|e| e.message)
        },
    )?;
    let base = format::parse_amount(&amount, decimals)?;
    if base == 0 {
        return Err(Error::usage("Pay more than nothing."));
    }
    let held = balances
        .iter()
        .find(|b| b.token.address == token.address)
        .and_then(|b| b.balance);
    if let Some(held) = held.filter(|h| h.0 < base) {
        ctx.warn(&format!(
            "The vault holds {} {}: this would fail unless more arrives first.",
            format::amount(held.0, decimals),
            token.symbol
        ));
    }
    let to = prompt::text(
        ctx,
        to,
        "<to>",
        "To whom? (0x…, or accounts:<name> from aztec-wallet)",
        None,
        |_| Ok(()),
    )?;
    let book = Book::load(ctx).await?;
    let recipient = book.address(ctx, &to)?;
    let label = book
        .label(&recipient)
        .map(|l| format!("{l} "))
        .unwrap_or_default();
    let intent = Intent::Transfer {
        token: token.address,
        to: recipient,
        amount: Amount(base),
        memo: options.memo.clone(),
    };
    submit(
        ctx,
        vault,
        intent,
        &options,
        vec![format!(
            "Pay {} {} to {label}{}",
            theme::bold(format::amount(base, decimals)),
            token.symbol,
            theme::dim(format::short(&recipient))
        )],
    )
    .await
}

async fn call(
    ctx: &Ctx,
    vault: &VaultSummary,
    function: Option<String>,
    contract: Option<String>,
    args: Vec<String>,
    options: ProposeOptions,
) -> Result<()> {
    ctx.intro(&format!("Propose a call · {}", vault.name));
    let contract = prompt::text(
        ctx,
        contract,
        "-ca <contract>",
        "Which contract? (0x… or contracts:<name>)",
        None,
        |_| Ok(()),
    )?;
    let book = Book::load(ctx).await?;
    let to = book.address(ctx, &contract)?;
    let functions = ctx
        .client()
        .await?
        .call::<ContractsFunctions>(&ContractFunctionsParams {
            contract: to,
            include_utility: None,
        })
        .await
        .unwrap_or_default();
    let function = match function {
        Some(f) => f,
        None if !functions.is_empty() => prompt::select(
            ctx,
            "<function>",
            "Which function?",
            functions
                .iter()
                .filter(|f| !f.is_static)
                .map(|f| {
                    let params = f
                        .params
                        .iter()
                        .map(|p| p.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ");
                    (
                        f.name.clone(),
                        f.name.clone(),
                        format!("{:?}({params})", f.kind).to_lowercase(),
                    )
                })
                .collect(),
        )?,
        None => prompt::text(ctx, None, "<function>", "Which function?", None, |_| Ok(()))?,
    };
    let signature = functions.iter().find(|f| f.name == function);
    if !functions.is_empty() && signature.is_none() {
        return Err(Error::usage(format!(
            "{} has no function {function}.",
            format::short(&to)
        )));
    }
    let args = match signature {
        Some(f) if args.is_empty() && !f.params.is_empty() && ctx.interactive => {
            let mut typed = Vec::new();
            for param in &f.params {
                typed.push(prompt::text(
                    ctx,
                    None,
                    "--args",
                    &format!("{} ({})", param.name, abi::type_name(&param.ty)),
                    None,
                    |_| Ok(()),
                )?);
            }
            typed
        }
        Some(f) if args.len() != f.params.len() => {
            return Err(Error::usage(format!(
                "{function} takes {} arguments ({}); you gave {}.",
                f.params.len(),
                f.params
                    .iter()
                    .map(|p| p.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
                args.len()
            )));
        }
        _ => args,
    };
    let args = book.call_args(ctx, &args)?;
    let label = book.label(&to).unwrap_or_else(|| format::short(&to));
    let summary = format!(
        "Call {} on {label} with {}",
        theme::name(&function),
        args.join(" ")
    );
    let args = args.iter().map(|arg| abi::parse(arg)).collect();
    let intent = Intent::Calls {
        calls: vec![IntentCall { to, function, args }],
        memo: options.memo.clone(),
    };
    submit(ctx, vault, intent, &options, vec![summary]).await
}

/// Validity in whole hours, 1 to 720.
fn expiry_hours(text: &str) -> Result<u32> {
    let secs = format::parse_duration(text)?;
    if !secs.is_multiple_of(3600) || !(3600..=720 * 3600).contains(&secs) {
        return Err(Error::usage(format!(
            "--expires {text}: use whole hours or days, from 1h to 30d."
        )));
    }
    u32::try_from(secs / 3600).map_err(|_| Error::usage("--expires is too long"))
}

fn preview_lines(preview: &ProposalPreview, labels: describe::Labels<'_>) -> Vec<String> {
    let mut lines: Vec<String> = preview
        .calls
        .iter()
        .enumerate()
        .map(|(i, c)| {
            format!(
                "{}  {} · {}  {}",
                i + 1,
                describe::target(c, labels),
                theme::name(&c.function),
                theme::dim(format::limited(abi::decoded(&c.args, false), false, 6).join(" "))
            )
        })
        .collect();
    lines.push(match (&preview.gas, &preview.error) {
        (Some(gas), _) if preview.ok => format!(
            "{} it simulates: {} L2 gas, {} DA gas",
            theme::check(true),
            format::group_thousands(&gas.l2.to_string()),
            format::group_thousands(&gas.da.to_string())
        ),
        _ if preview.ok => format!("{} it simulates", theme::check(true)),
        (_, error) => format!(
            "{} it does not simulate: {}",
            theme::check(false),
            error.as_ref().map_or_else(
                || "no reason given".to_owned(),
                describe::simulation_failure
            )
        ),
    });
    lines.extend(describe::effects(preview.effects.as_ref()));
    lines.extend(preview.fee.as_ref().map(describe::fee_estimate));
    lines
}

async fn submit(
    ctx: &Ctx,
    vault: &VaultSummary,
    intent: Intent,
    options: &ProposeOptions,
    summary: Vec<String>,
) -> Result<()> {
    let params = ProposeParams {
        vault: vault.address,
        intent,
        expires_in_hours: expiry_hours(&options.expires)?,
    };
    let client = ctx.client().await?;
    let book = Book::load(ctx).await?;
    let tokens = client
        .call::<TokensList>(&VaultParams {
            vault: vault.address,
        })
        .await?;
    let labels = |a: &ome_api::Hex32| {
        tokens
            .iter()
            .find(|t| t.token.address == *a)
            .map(|t| t.token.symbol.clone())
            .or_else(|| book.label(a))
    };
    ctx.section("What it will do", &summary);
    let step = Step::start(ctx, "Simulating on this device");
    let preview = client.call::<ProposalsPreview>(&params).await;
    let preview = match preview {
        Ok(preview) => {
            step.clear();
            preview
        }
        Err(error) => {
            step.fail("Could not simulate");
            return Err(error.into());
        }
    };
    if options.dry_run {
        if ctx.mode == Mode::Json {
            return ctx.json(&preview);
        }
        ctx.section(
            "The calls, as the vault would make them",
            &preview_lines(&preview, &labels),
        );
        ctx.outro("Dry run: nothing was proposed");
        return if preview.ok {
            Ok(())
        } else {
            Err(Error::failed("The simulation failed."))
        };
    }
    ctx.section(
        "The calls, as the vault would make them",
        &preview_lines(&preview, &labels),
    );
    let question = if preview.ok {
        format!("Propose it to the {} members?", vault.members)
    } else {
        "It does not simulate now. Propose it anyway?".to_owned()
    };
    if !prompt::confirm(ctx, &question, preview.ok)? {
        return Err(Error::declined());
    }
    let step = Step::start(ctx, "Proposing");
    let view = match client.call::<ProposalsCreate>(&params).await {
        Ok(view) => {
            step.done(&format!("Proposed {}", theme::name(format::id(&view.id))));
            view
        }
        Err(error) => {
            step.fail("Could not propose");
            return Err(error.into());
        }
    };
    if ctx.mode == Mode::Json {
        return ctx.json(&view);
    }
    if ctx.quiet || ctx.mode == Mode::Plain {
        ctx.out(&view.id.to_string());
    }
    ctx.outro(&format!(
        "{} · {} · expires {}",
        describe::intent(&view),
        describe::votes(&view),
        format::relative(view.expires_at)
    ));
    ctx.next(&format!("approve {}", format::id(&view.id)));
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn expiry_is_whole_hours_up_to_thirty_days() {
        assert_eq!(super::expiry_hours("24h").unwrap(), 24);
        assert_eq!(super::expiry_hours("30d").unwrap(), 720);
        assert!(super::expiry_hours("90m").is_err());
        assert!(super::expiry_hours("31d").is_err());
    }
}
