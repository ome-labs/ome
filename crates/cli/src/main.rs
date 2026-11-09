//! `ome`: an Ome member's command line, a client of their daemon (`omed`). Vault work only: ordinary accounts,
//! deployments and Fee Juice stay with `aztec-wallet`. See `architecture/cli.md`.

mod args;
mod commands;
mod context;
mod error;
#[cfg(test)]
mod fixtures;
mod launch;
mod refs;
mod secret;
mod ui;
mod watch;

use std::process::ExitCode;

use clap::Parser;

use crate::{
    args::Cli,
    context::Ctx,
    error::{Error, Exit},
    ui::theme,
};

#[tokio::main]
async fn main() -> ExitCode {
    let argv = args::normalize(std::env::args_os().map(|a| a.to_string_lossy().into_owned()));
    let cli = match Cli::try_parse_from(argv) {
        Ok(cli) => cli,
        Err(error) => {
            let _ = error.print();
            return ExitCode::from(u8::try_from(error.exit_code()).unwrap_or(2));
        }
    };
    theme::configure(cli.global.no_color);
    let (json, verbose) = (cli.global.json, cli.global.verbose);
    let result = match Ctx::new(&cli.global) {
        Ok(mut ctx) => commands::run(&mut ctx, cli.command).await,
        Err(error) => Err(error),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            report(&error, json, verbose);
            ExitCode::from(error.exit.code())
        }
    }
}

/// The error on stderr: a sentence, the fix, and with `--verbose` (or when it explains a crash) the cause.
fn report(error: &Error, json: bool, verbose: bool) {
    if json {
        eprintln!("{}", error.to_json());
        return;
    }
    eprintln!("{} {}", theme::bad("✗"), error.message);
    if let Some(hint) = &error.hint {
        eprintln!("  {hint}");
    }
    if let Some(cause) = &error.cause
        && (verbose || error.exit == Exit::Daemon)
    {
        for line in cause.lines() {
            eprintln!("  {}", theme::dim(line));
        }
    }
    if error.exit == Exit::Failed && error.kind.is_none() && error.hint.is_none() {
        eprintln!(
            "  {}",
            theme::dim(
                "If this looks wrong, see `ome daemon logs` and report it with what you ran."
            )
        );
    }
}
