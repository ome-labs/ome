//! Long steps named and timed as they happen: a spinner that resolves into a line with its time. Silent outside the
//! human mode, so `--json` and `--plain` output stays clean.

use std::{collections::HashMap, io::IsTerminal, time::Instant};

use ome_api::daemon::StepStatus;

use crate::{
    context::Ctx,
    ui::{format, theme},
};

/// One running step. Without a terminal there is no spinner, but finished steps still print a line, so logs read
/// like the screen.
pub(crate) struct Step {
    spinner: Option<cliclack::ProgressBar>,
    /// Human mode without a terminal: lines only.
    lines: bool,
    started: Instant,
}

impl Step {
    pub(crate) fn start(ctx: &Ctx, label: &str) -> Self {
        let terminal = std::io::stderr().is_terminal();
        let spinner = (ctx.human() && terminal).then(|| {
            let spinner = cliclack::spinner();
            spinner.start(label);
            spinner
        });
        Self {
            spinner,
            lines: ctx.human() && !terminal,
            started: Instant::now(),
        }
    }

    pub(crate) fn message(&self, text: &str) {
        if let Some(spinner) = &self.spinner {
            spinner.set_message(text);
        }
    }

    /// Resolves into `text` with the time it took.
    pub(crate) fn done(self, text: &str) {
        self.finish(&format!(
            "{text}  {}",
            theme::dim(format::elapsed(self.started.elapsed()))
        ));
    }

    /// Resolves into `text` as is, without the time.
    pub(crate) fn finish(&self, text: &str) {
        if let Some(spinner) = &self.spinner {
            spinner.stop(text);
        } else if self.lines {
            let _ = cliclack::log::success(text);
        }
    }

    pub(crate) fn fail(self, text: &str) {
        if let Some(spinner) = &self.spinner {
            spinner.error(text);
        } else if self.lines {
            let _ = cliclack::log::error(text);
        }
    }

    /// Removes the spinner without a line.
    pub(crate) fn clear(self) {
        if let Some(spinner) = &self.spinner {
            spinner.clear();
        }
    }
}

/// A step as a long-running flow reports it.
pub(crate) struct Tracked {
    pub(crate) key: &'static str,
    pub(crate) label: String,
    pub(crate) status: StepStatus,
    pub(crate) detail: Option<String>,
    /// Seconds the step took, when the flow knows.
    pub(crate) took: Option<u64>,
}

/// Follows a flow's steps (founding, rotation) from repeated snapshots: one spinner for the active step, a line for
/// each step as it finishes, including steps that started and finished between two snapshots.
#[derive(Default)]
pub(crate) struct Tracker {
    active: Option<(&'static str, Step)>,
    finished: HashMap<&'static str, StepStatus>,
}

impl Tracker {
    pub(crate) fn update(&mut self, ctx: &Ctx, steps: impl IntoIterator<Item = Tracked>) {
        for step in steps {
            if self.finished.contains_key(step.key) {
                continue;
            }
            let label = match &step.detail {
                Some(detail) => format!("{} {}", step.label, theme::dim(format!("· {detail}"))),
                None => step.label.clone(),
            };
            match step.status {
                StepStatus::Pending => {}
                StepStatus::Active => match &self.active {
                    Some((key, running)) if *key == step.key => running.message(&label),
                    _ => {
                        if let Some((_, previous)) = self.active.take() {
                            previous.clear();
                        }
                        self.active = Some((step.key, Step::start(ctx, &label)));
                    }
                },
                StepStatus::Done | StepStatus::Failed => {
                    let running = match self.active.take() {
                        Some((key, running)) if key == step.key => running,
                        other => {
                            self.active = other;
                            Step::start(ctx, &label)
                        }
                    };
                    let took = step
                        .took
                        .map(|secs| format!("  {}", theme::dim(format::span(secs))))
                        .unwrap_or_default();
                    if step.status == StepStatus::Done {
                        running.finish(&format!("{label}{took}"));
                    } else {
                        running.fail(&label);
                    }
                    self.finished.insert(step.key, step.status);
                }
            }
        }
    }

    /// Stops a spinner still running, as when the flow failed elsewhere.
    pub(crate) fn finish(&mut self) {
        if let Some((_, running)) = self.active.take() {
            running.clear();
        }
    }
}
