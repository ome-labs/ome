//! Colour for meaning only: green done, yellow attention, red failed or refused, cyan what you can type, dim for
//! secondary detail, bold for amounts. 16-colour safe; off for pipes, `NO_COLOR`, `TERM=dumb` and `--no-color`.

use console::{StyledObject, style};

/// Turns colour off for both streams when asked, or when the terminal cannot show it.
pub(crate) fn configure(no_color: bool) {
    let dumb = std::env::var("TERM").is_ok_and(|t| t == "dumb");
    let env = std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty());
    if no_color || dumb || env {
        console::set_colors_enabled(false);
        console::set_colors_enabled_stderr(false);
    }
}

pub(crate) fn ok<D>(text: D) -> StyledObject<D> {
    style(text).green()
}

pub(crate) fn warn<D>(text: D) -> StyledObject<D> {
    style(text).yellow()
}

pub(crate) fn bad<D>(text: D) -> StyledObject<D> {
    style(text).red()
}

/// Commands, aliases and names you can type.
pub(crate) fn name<D>(text: D) -> StyledObject<D> {
    style(text).cyan()
}

pub(crate) fn dim<D>(text: D) -> StyledObject<D> {
    style(text).dim()
}

pub(crate) fn bold<D>(text: D) -> StyledObject<D> {
    style(text).bold()
}

/// `✓`, `▲` and `✗`, coloured.
pub(crate) fn check(passed: bool) -> StyledObject<&'static str> {
    if passed { ok("✓") } else { bad("✗") }
}

pub(crate) fn caution() -> StyledObject<&'static str> {
    warn("▲")
}
