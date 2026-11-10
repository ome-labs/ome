//! `ome update`: updates Ome the way it was installed (npm, Homebrew or the install script), then restarts this
//! profile's daemon from the new release. Local files upgrade themselves when the new daemon and keeper start, and
//! each vault moves to a newer protocol by itself once every member runs a release that speaks it.

use std::process::Command;

use crate::{
    context::{Ctx, Mode},
    error::{Error, Result},
    launch,
    ui::{prompt, steps::Step},
};

pub(crate) fn run(ctx: &Ctx) -> Result<()> {
    let (method, command) = launch::install_method(&ctx.home);
    let program: &[&str] = match method {
        "npm" => &["npm", "install", "-g", "@ome-labs/ome@latest"],
        "Homebrew" => &["brew", "upgrade", "ome"],
        "install script" => &["sh", "-c", "curl -fsSL https://ome.network/install.sh | sh"],
        _ => {
            return Err(Error::usage(format!(
                "Ome was installed by {method}: update it with `{command}`."
            )));
        }
    };
    prompt::proceed(ctx, &format!("Update Ome with `{}`?", program.join(" ")))?;
    let step = Step::start(ctx, &format!("Updating Ome ({method})"));
    let updated = Command::new(program[0])
        .args(&program[1..])
        .status()
        .map_err(|e| Error::failed(format!("Could not run {}.", program[0])).cause(e))?;
    if !updated.success() {
        step.fail("The update failed; Ome is unchanged");
        return Err(Error::failed(format!("`{}` failed.", program.join(" "))));
    }
    step.done("Updated");
    // The daemon restarts from the new `ome`, which finds its own daemon and keeper beside it.
    let step = Step::start(ctx, "Restarting the daemon on the new release");
    let restarted = Command::new("ome")
        .args(["--profile", ctx.profile.name(), "daemon", "restart"])
        .status()
        .is_ok_and(|s| s.success());
    if restarted {
        step.done("The daemon runs the new release");
    } else {
        step.fail("The daemon still runs the old release");
        ctx.next("daemon restart");
    }
    if ctx.mode == Mode::Json {
        return ctx.json(
            &serde_json::json!({ "updated": true, "restarted": restarted, "method": method }),
        );
    }
    Ok(())
}
