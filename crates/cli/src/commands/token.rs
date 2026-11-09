//! `ome token add|list|remove|deploy-test`: the tokens whose balances and transfers the vault shows.

use ome_api::daemon::{
    TokenDeployment, TokenParams, TokenRefParams, TokensAdd, TokensDeployTest, TokensRemove,
};
use ome_api::keeper::VaultParams;

use crate::{
    args::{BalanceArgs, TokenAddArgs, TokenCommand},
    commands::vault,
    context::{Ctx, Mode},
    error::{Error, Result},
    refs::{self, Book},
    ui::{prompt, steps::Step, theme},
};

pub(crate) async fn run(ctx: &Ctx, command: TokenCommand) -> Result<()> {
    match command {
        TokenCommand::Add(args) => add(ctx, args).await,
        TokenCommand::List => vault::balance(ctx, BalanceArgs { token: None }).await,
        TokenCommand::Remove { token } => {
            let v = refs::vault(ctx).await?;
            let input = prompt::text(ctx, token, "<token>", "Which token?", None, |_| Ok(()))?;
            let token = refs::token(ctx, v.address, &input).await?;
            let left = ctx
                .client()
                .await?
                .call::<TokensRemove>(&TokenRefParams {
                    vault: v.address,
                    token: token.address,
                })
                .await?;
            if ctx.mode == Mode::Json {
                return ctx.json(&left);
            }
            ctx.success(&format!(
                "{} is no longer tracked; the vault still holds it",
                token.symbol
            ));
            Ok(())
        }
        TokenCommand::DeployTest => {
            let v = refs::vault(ctx).await?;
            let step = Step::start(ctx, "Deploying a test token and minting to the vault");
            let token = ctx
                .client()
                .await?
                .call::<TokensDeployTest>(&VaultParams { vault: v.address })
                .await;
            let token = match token {
                Ok(token) => token,
                Err(error) => {
                    step.fail("Could not deploy the test token");
                    return Err(error.into());
                }
            };
            step.done(&format!("Deployed {} at {}", token.symbol, token.address));
            if ctx.mode == Mode::Json {
                return ctx.json(&token);
            }
            ctx.info(&format!(
                "The other members track it with `ome token add {}`.",
                token.address
            ));
            ctx.next("balance");
            Ok(())
        }
    }
}

async fn add(ctx: &Ctx, args: TokenAddArgs) -> Result<()> {
    let v = refs::vault(ctx).await?;
    let input = prompt::text(
        ctx,
        args.token,
        "<token>",
        "The token contract (0x… or contracts:<name>)",
        None,
        |_| Ok(()),
    )?;
    let address = Book::load(ctx).await?.address(ctx, &input)?;
    let deployment = match args.salt {
        None => None,
        Some(salt) => {
            let hex = |text: Option<String>, flag: &str| -> Result<_> {
                text.unwrap_or_default()
                    .parse()
                    .map_err(|_| Error::usage(format!("{flag} is not a 32-byte hex value.")))
            };
            Some(TokenDeployment {
                salt: hex(Some(salt), "--salt")?,
                deployer: hex(args.deployer, "--deployer")?,
                admin: hex(args.admin, "--admin")?,
                name: args.token_name.unwrap_or_default(),
                symbol: args.symbol.unwrap_or_default(),
                decimals: args.decimals.unwrap_or_default(),
            })
        }
    };
    let step = Step::start(ctx, "Reading the token");
    let token = ctx
        .client()
        .await?
        .call::<TokensAdd>(&TokenParams {
            vault: v.address,
            token: address,
            deployment,
        })
        .await;
    let token = match token {
        Ok(token) => token,
        Err(error) => {
            step.fail("Could not track the token");
            return Err(Error::from(error).hint(
                "A token whose instance was never published needs its deployment: --salt, --deployer, --admin, \
                 --token-name, --symbol and --decimals.",
            ));
        }
    };
    step.done(&format!(
        "Tracking {} ({})",
        theme::name(&token.symbol),
        token.name
    ));
    if ctx.mode == Mode::Json {
        return ctx.json(&token);
    }
    ctx.next("balance");
    Ok(())
}
