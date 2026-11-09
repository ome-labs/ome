//! Prompts that never trap: a value already given is used as is; with prompts off (`--no-input`, `--json`,
//! `--plain`, no terminal) a missing value is an error naming its flag.

use std::str::FromStr;

use zeroize::Zeroizing;

use crate::{
    context::Ctx,
    error::{Error, Result},
};

/// A text value: `given`, or asked for with `label`.
pub(crate) fn text(
    ctx: &Ctx,
    given: Option<String>,
    flag: &str,
    label: &str,
    default: Option<&str>,
    validate: impl Fn(&str) -> std::result::Result<(), String> + 'static,
) -> Result<String> {
    if let Some(value) = given {
        validate(&value).map_err(Error::usage)?;
        return Ok(value);
    }
    if !ctx.interactive {
        return Err(Error::missing(label, flag));
    }
    let mut input = cliclack::input(label).validate(move |value: &String| validate(value));
    if let Some(default) = default {
        input = input.default_input(default);
    }
    Ok(input.interact::<String>()?)
}

/// A number in `range`: `given`, or asked for.
pub(crate) fn number<T>(
    ctx: &Ctx,
    given: Option<T>,
    flag: &str,
    label: &str,
    default: T,
    range: std::ops::RangeInclusive<T>,
) -> Result<T>
where
    T: FromStr + PartialOrd + Copy + std::fmt::Display + 'static,
{
    let check = {
        let range = range.clone();
        move |value: T| {
            if range.contains(&value) {
                Ok(())
            } else {
                Err(format!(
                    "Use a number from {} to {}.",
                    range.start(),
                    range.end()
                ))
            }
        }
    };
    if let Some(value) = given {
        check(value).map_err(|e| Error::usage(format!("{flag} {value}: {e}")))?;
        return Ok(value);
    }
    if !ctx.interactive {
        return Err(Error::missing(label, flag));
    }
    let value = cliclack::input(label)
        .default_input(&default.to_string())
        .validate(move |input: &String| {
            input
                .trim()
                .parse::<T>()
                .map_err(|_| "Type a number.".to_owned())
                .and_then(&check)
        })
        .interact::<String>()?;
    value
        .trim()
        .parse()
        .map_err(|_| Error::usage("not a number"))
}

/// One of `items` (value, label, hint).
pub(crate) fn select<T: Clone + Eq>(
    ctx: &Ctx,
    flag: &str,
    label: &str,
    items: Vec<(T, String, String)>,
) -> Result<T> {
    if !ctx.interactive {
        return Err(Error::missing(label, flag));
    }
    let mut select = cliclack::select(label);
    if items.len() > 7 {
        select = select.filter_mode();
    }
    for (value, label, hint) in items {
        select = select.item(value, label, hint);
    }
    Ok(select.interact()?)
}

/// A yes/no question; `--yes` answers yes, and with prompts off it must.
pub(crate) fn confirm(ctx: &Ctx, label: &str, default: bool) -> Result<bool> {
    if ctx.yes {
        return Ok(true);
    }
    if !ctx.interactive {
        return Err(Error::usage(format!(
            "{label} Pass --yes to confirm without a prompt."
        )));
    }
    Ok(cliclack::confirm(label).initial_value(default).interact()?)
}

/// Asks for a yes, or fails as declined.
pub(crate) fn proceed(ctx: &Ctx, label: &str) -> Result<()> {
    if confirm(ctx, label, true)? {
        Ok(())
    } else {
        Err(Error::declined())
    }
}

/// For the irreversible: the person types `expected` (a vault or profile name); scripts pass `--confirm <name>`.
/// `--yes` is not enough.
pub(crate) fn typed(ctx: &Ctx, expected: &str, given: Option<&str>, what: &str) -> Result<()> {
    if let Some(given) = given {
        return if given == expected {
            Ok(())
        } else {
            Err(Error::usage(format!(
                "--confirm {given:?} does not match {expected:?}."
            )))
        };
    }
    if !ctx.interactive {
        return Err(Error::usage(format!("{what} cannot be undone."))
            .hint(format!("Pass --confirm {expected:?} to go ahead.")));
    }
    let expected = expected.to_owned();
    cliclack::input(format!(
        "{what} cannot be undone. Type {expected:?} to go ahead"
    ))
    .validate(move |value: &String| {
        if value.trim() == expected {
            Ok(())
        } else {
            Err(format!("Type {expected} exactly."))
        }
    })
    .interact::<String>()?;
    Ok(())
}

/// A secret, never echoed.
pub(crate) fn password(label: &str) -> Result<Zeroizing<String>> {
    Ok(Zeroizing::new(
        cliclack::password(label).mask('▪').interact()?,
    ))
}
