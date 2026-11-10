//! `ome contract add|list|remove|functions`: the contracts a vault's proposals can call. Adding one checks its artifact
//! is the code of the class at its address; members who don't have it fetch it when a proposal calls it.

use ome_api::{
    Empty, HexBytes,
    daemon::{
        ContractFunctionsParams, ContractInstanceInput, ContractParams, ContractRegisterParams,
        ContractSource, ContractsFunctions, ContractsList, ContractsRegister, ContractsRemove,
    },
};

use crate::{
    args::{ContractAddArgs, ContractCommand},
    context::{Ctx, Mode},
    error::{Error, Result},
    refs::Book,
    ui::{
        abi, format, prompt,
        table::{self, Tone},
    },
};

pub(crate) async fn run(ctx: &Ctx, command: ContractCommand) -> Result<()> {
    match command {
        ContractCommand::Add(args) => add(ctx, args).await,
        ContractCommand::List => list(ctx).await,
        ContractCommand::Remove { contract } => {
            let address = address(ctx, contract).await?;
            ctx.client()
                .await?
                .call::<ContractsRemove>(&ContractParams { contract: address })
                .await?;
            if ctx.mode == Mode::Json {
                return ctx.json(&());
            }
            ctx.success(&format!("{} is forgotten", format::short(&address)));
            Ok(())
        }
        ContractCommand::Functions { contract, utility } => functions(ctx, contract, utility).await,
    }
}

async fn address(ctx: &Ctx, input: Option<String>) -> Result<ome_api::Hex32> {
    let input = prompt::text(
        ctx,
        input,
        "<contract>",
        "Which contract? (0x… or contracts:<name>)",
        None,
        |_| Ok(()),
    )?;
    Book::load(ctx).await?.address(ctx, &input)
}

async fn add(ctx: &Ctx, args: ContractAddArgs) -> Result<()> {
    let path = prompt::text(
        ctx,
        args.artifact,
        "<artifact>",
        "The artifact's JSON file",
        None,
        |_| Ok(()),
    )?;
    let artifact = std::fs::read_to_string(&path)
        .map_err(|e| Error::usage(format!("Can't read {path}.")).cause(e))?;
    let address = address(ctx, args.address).await?;
    let instance = match args.salt {
        None => None,
        Some(salt) => {
            let hex = |text: Option<String>, flag: &str| -> Result<_> {
                text.unwrap_or_default()
                    .parse()
                    .map_err(|_| Error::usage(format!("{flag} is not a 32-byte hex value.")))
            };
            Some(ContractInstanceInput {
                salt: hex(Some(salt), "--salt")?,
                deployer: hex(args.deployer, "--deployer")?,
                initialization_hash: hex(args.initialization_hash, "--initialization-hash")?,
                immutables_hash: hex(args.immutables_hash, "--immutables-hash")?,
                public_keys: args
                    .public_keys
                    .unwrap_or_default()
                    .parse::<HexBytes>()
                    .map_err(|_| Error::usage("--public-keys is not hex."))?,
            })
        }
    };
    let view = ctx
        .client()
        .await?
        .call::<ContractsRegister>(&ContractRegisterParams {
            artifact,
            address,
            instance,
        })
        .await?;
    if ctx.mode == Mode::Json {
        return ctx.json(&view);
    }
    ctx.success(&format!(
        "{} at {}: {} functions a proposal can call",
        view.name,
        format::short(&view.address),
        view.functions
    ));
    ctx.info("Members who don't have it fetch it from your keeper when a proposal calls it.");
    ctx.next(&format!("contract functions {}", view.address));
    Ok(())
}

async fn list(ctx: &Ctx) -> Result<()> {
    let contracts = ctx.client().await?.call::<ContractsList>(&Empty {}).await?;
    if ctx.mode == Mode::Json {
        return ctx.json(&contracts);
    }
    let mut t = table::Table::new(&["contract", "from", "functions", "address"]);
    for c in &contracts {
        let source = match c.source {
            ContractSource::Registered => "added",
            ContractSource::Token => "token",
            ContractSource::Vault => "vault",
        };
        t.row(vec![
            table::toned(&c.name, Tone::Bold),
            table::cell(source),
            table::cell(c.functions.to_string()),
            table::toned(c.address.to_string(), Tone::Dim),
        ]);
    }
    t.print(ctx);
    Ok(())
}

async fn functions(ctx: &Ctx, contract: Option<String>, utility: bool) -> Result<()> {
    let address = address(ctx, contract).await?;
    let functions = ctx
        .client()
        .await?
        .call::<ContractsFunctions>(&ContractFunctionsParams {
            contract: address,
            include_utility: utility.then_some(true),
        })
        .await?;
    if ctx.mode == Mode::Json {
        return ctx.json(&functions);
    }
    let mut t = table::Table::new(&["function", "runs", "parameters"]);
    for f in &functions {
        let params = f
            .params
            .iter()
            .map(|p| format!("{}: {}", p.name, abi::type_name(&p.ty)))
            .collect::<Vec<_>>()
            .join(", ");
        t.row(vec![
            table::toned(&f.name, Tone::Name),
            table::cell(
                format!("{:?}{}", f.kind, if f.is_static { ", static" } else { "" }).to_lowercase(),
            ),
            table::cell(params),
        ]);
    }
    t.print(ctx);
    Ok(())
}
