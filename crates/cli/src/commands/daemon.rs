//! `ome daemon start|stop|restart|logs`. Commands start the daemon on their own when needed; these are for
//! doing it by hand.

use std::{
    io::{Read, Seek, SeekFrom},
    path::Path,
    time::Duration,
};

use crate::{
    args::{DaemonCommand, LogArgs},
    context::{Ctx, Mode},
    error::{Error, Result},
    launch,
    ui::{steps::Step, theme},
};

pub(crate) async fn run(ctx: &mut Ctx, command: DaemonCommand) -> Result<()> {
    match command {
        DaemonCommand::Start { foreground } => {
            if foreground {
                return launch::foreground(ctx);
            }
            if let Some(info) = launch::running(&ctx.profile).await {
                ctx.info(&format!("The daemon is already running · pid {}", info.pid));
                return if ctx.mode == Mode::Json {
                    ctx.json(&info)
                } else {
                    Ok(())
                };
            }
            let info = launch::start(ctx).await?;
            if ctx.mode == Mode::Json {
                return ctx.json(&info);
            }
            Ok(())
        }
        DaemonCommand::Stop => stop(ctx).await,
        DaemonCommand::Restart => {
            stop(ctx).await?;
            let info = launch::start(ctx).await?;
            if ctx.mode == Mode::Json {
                return ctx.json(&info);
            }
            Ok(())
        }
        DaemonCommand::Logs(args) => logs(ctx, &launch::log_path(&ctx.profile), &args).await,
    }
}

async fn stop(ctx: &mut Ctx) -> Result<()> {
    ctx.drop_client();
    let step = Step::start(ctx, "Stopping the daemon and its keeper");
    if let Some(pid) = launch::stop(&ctx.profile).await? {
        step.done(&format!("Daemon stopped · pid {pid}"));
    } else {
        step.clear();
        ctx.info("The daemon was not running.");
    }
    if ctx.mode == Mode::Json {
        return ctx.json(&serde_json::json!({ "running": false }));
    }
    Ok(())
}

/// The last lines of a log, then, with `--follow`, every line as it is written.
pub(crate) async fn logs(ctx: &Ctx, path: &Path, args: &LogArgs) -> Result<()> {
    if !path.is_file() {
        return Err(Error::usage(format!(
            "There is no log at {} yet.",
            path.display()
        )));
    }
    if ctx.human() {
        eprintln!("{}", theme::dim(path.display()));
    }
    let tail = launch::tail(path, args.lines);
    if !tail.is_empty() {
        ctx.out(&tail);
    }
    if !args.follow {
        return Ok(());
    }
    let mut file = std::fs::File::open(path)?;
    let mut position = file.seek(SeekFrom::End(0))?;
    let mut pending = String::new();
    loop {
        tokio::time::sleep(Duration::from_millis(400)).await;
        let length = std::fs::metadata(path)?.len();
        if length < position {
            position = 0;
        }
        if length == position {
            continue;
        }
        file.seek(SeekFrom::Start(position))?;
        let mut chunk = String::new();
        file.read_to_string(&mut chunk)?;
        position = length;
        pending.push_str(&chunk);
        while let Some(end) = pending.find('\n') {
            let line: String = pending.drain(..=end).collect();
            ctx.out(line.trim_end());
        }
    }
}
