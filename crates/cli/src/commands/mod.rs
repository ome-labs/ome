//! One module per group of the command tree.

mod apps;
mod config;
mod contract;
mod daemon;
mod device;
mod keeper;
mod member;
mod profile;
pub(crate) mod proposal;
pub(crate) mod propose;
mod setup;
mod signer;
mod token;
mod update;
pub(crate) mod vault;

use crate::{
    args::{Command, ProposalCommand, VaultCommand},
    context::Ctx,
    error::Result,
    watch,
};

pub(crate) async fn run(ctx: &mut Ctx, command: Option<Command>) -> Result<()> {
    let Some(command) = command else {
        return setup::home(ctx).await;
    };
    match command {
        Command::Init(args) => setup::init(ctx, args).await,
        Command::Status => setup::status(ctx).await,
        Command::Fees => setup::fees(ctx).await,
        Command::Doctor(args) => setup::doctor(ctx, args).await,
        Command::Update => update::run(ctx),
        Command::Config(command) => config::run(ctx, command).await,
        Command::Profile(command) => profile::run(ctx, command).await,
        Command::Daemon(command) => daemon::run(ctx, command).await,
        Command::Keeper(command) => keeper::run(ctx, command).await,
        Command::Completions { shell } => {
            setup::completions(shell);
            Ok(())
        }
        Command::Unlock(args) => signer::unlock(ctx, args).await,
        Command::Lock => signer::lock(ctx).await,
        Command::Signer(command) => signer::run(ctx, command).await,
        Command::Vault(VaultCommand::Balance(args)) | Command::Balance(args) => {
            vault::balance(ctx, args).await
        }
        Command::Vault(command) => vault::run(ctx, command).await,
        Command::Activity(args) => vault::activity(ctx, args).await,
        Command::Counterparties => vault::counterparties(ctx).await,
        Command::Request(args) => vault::request(ctx, args).await,
        Command::Token(command) => token::run(ctx, command).await,
        Command::Contract(command) => contract::run(ctx, command).await,
        Command::Propose(command) => propose::run(ctx, command).await,
        Command::Proposals(args) | Command::Proposal(ProposalCommand::List(args)) => {
            proposal::list(ctx, args).await
        }
        Command::Proposal(ProposalCommand::Show(arg)) => proposal::show(ctx, arg).await,
        Command::Proposal(ProposalCommand::Review(arg)) => proposal::review(ctx, arg).await,
        Command::Approve(arg) | Command::Proposal(ProposalCommand::Approve(arg)) => {
            proposal::approve(ctx, arg).await
        }
        Command::Reject(args) | Command::Proposal(ProposalCommand::Reject(args)) => {
            proposal::reject(ctx, args).await
        }
        Command::Execute(args) | Command::Proposal(ProposalCommand::Execute(args)) => {
            proposal::execute(ctx, args).await
        }
        Command::Member(command) => member::run(ctx, command).await,
        Command::Device(command) => device::run(ctx, command).await,
        Command::Apps(command) => apps::run(ctx, command).await,
        Command::Watch => watch::run(ctx).await,
    }
}
