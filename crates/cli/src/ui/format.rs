//! How values read on screen: amounts in token units, short addresses, relative times, durations.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ome_api::{Hex32, Point};

use crate::error::{Error, Result};

/// Unix seconds now.
pub(crate) fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// `0x2b1f…9e04`.
pub(crate) fn short(hex: &Hex32) -> String {
    short_str(&hex.to_string())
}

/// `0x2b1f…9e04` for any `0x` value longer than 14 characters.
pub(crate) fn short_str(text: &str) -> String {
    if text.len() <= 14 || !text.is_ascii() {
        return text.to_owned();
    }
    format!("{}…{}", &text[..6], &text[text.len() - 4..])
}

/// A call argument as people read it: a field that fits in 128 bits as its number, anything else shortened.
pub(crate) fn arg(text: &str, verbose: bool) -> String {
    if let Some(digits) = text.strip_prefix("0x") {
        let significant = digits.trim_start_matches('0');
        if significant.len() <= 32
            && let Ok(value) = u128::from_str_radix(
                if significant.is_empty() {
                    "0"
                } else {
                    significant
                },
                16,
            )
        {
            return group_thousands(&value.to_string());
        }
    }
    if verbose {
        text.to_owned()
    } else {
        short_str(text)
    }
}

/// At most `limit` lines, then how many more `--verbose` shows; all of them with `verbose`.
pub(crate) fn limited(lines: Vec<String>, verbose: bool, limit: usize) -> Vec<String> {
    if verbose || lines.len() <= limit {
        return lines;
    }
    let more = lines.len() - limit;
    let mut out: Vec<String> = lines.into_iter().take(limit).collect();
    out.push(format!("… {more} more (--verbose shows them)"));
    out
}

/// The first 8 hex digits of an id, as people type it: `4f2ac91e`.
pub(crate) fn id(hex: &Hex32) -> String {
    hex.to_string()[2..10].to_owned()
}

/// A point as `0x<x>:0x<y>`, the form `member swap-key` takes.
pub(crate) fn point(point: &Point) -> String {
    format!("{}:{}", point.x, point.y)
}

/// A short point: `0x1a2b…77ff`, from its `x`.
pub(crate) fn short_point(point: &Point) -> String {
    short(&point.x)
}

/// Parses `0x<x>:0x<y>`, `<x>,<y>` or the 64 bytes as one hex value.
pub(crate) fn parse_point(text: &str) -> Result<Point> {
    let bad = || Error::usage(format!("{text:?} is not a key: use 0x<x>:0x<y>"));
    let (x, y) = if let Some(pair) = text.split_once([':', ',']) {
        pair
    } else {
        let digits = text.strip_prefix("0x").unwrap_or(text);
        if digits.len() != 128 || !digits.is_ascii() {
            return Err(bad());
        }
        digits.split_at(64)
    };
    Ok(Point {
        x: x.trim().parse().map_err(|_| bad())?,
        y: y.trim().parse().map_err(|_| bad())?,
    })
}

/// Parses a decimal amount (`25`, `0.5`, `1_000.25`) into base units with `decimals` places.
pub(crate) fn parse_amount(input: &str, decimals: u8) -> Result<u128> {
    let bad = || Error::usage(format!("{input:?} is not an amount"));
    let cleaned: String = input.chars().filter(|c| *c != '_' && *c != ',').collect();
    let (whole, fraction) = cleaned.split_once('.').unwrap_or((&cleaned, ""));
    if whole.is_empty() && fraction.is_empty() {
        return Err(bad());
    }
    if !whole
        .bytes()
        .chain(fraction.bytes())
        .all(|b| b.is_ascii_digit())
    {
        return Err(bad());
    }
    let fraction = fraction.trim_end_matches('0');
    if fraction.len() > usize::from(decimals) {
        return Err(Error::usage(format!(
            "{input:?} has more than {decimals} decimal places, the token's precision"
        )));
    }
    let too_big = || Error::usage(format!("{input:?} is too large"));
    let scale = 10u128
        .checked_pow(u32::from(decimals))
        .ok_or_else(too_big)?;
    let whole: u128 = if whole.is_empty() {
        0
    } else {
        whole.parse().map_err(|_| too_big())?
    };
    let mut frac: u128 = 0;
    if !fraction.is_empty() {
        frac = fraction.parse().map_err(|_| bad())?;
        // The fraction has at most `decimals` digits, so the shift is at most `decimals`.
        let shift = u32::from(decimals) - u32::try_from(fraction.len()).map_err(|_| bad())?;
        frac *= 10u128.pow(shift);
    }
    whole
        .checked_mul(scale)
        .and_then(|w| w.checked_add(frac))
        .ok_or_else(too_big)
}

/// Base units in token units, trailing zeros trimmed and thousands grouped: `1,000.25`.
pub(crate) fn amount(base_units: u128, decimals: u8) -> String {
    let digits = base_units.to_string();
    let decimals = usize::from(decimals);
    let padded = format!("{digits:0>width$}", width = decimals + 1);
    let (whole, fraction) = padded.split_at(padded.len() - decimals);
    let fraction = fraction.trim_end_matches('0');
    let whole = group_thousands(whole);
    if fraction.is_empty() {
        whole
    } else {
        format!("{whole}.{fraction}")
    }
}

/// `41203` → `41,203`.
pub(crate) fn group_thousands(digits: &str) -> String {
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// `90s`, `15m`, `24h`, `7d`, `2w` → seconds.
pub(crate) fn parse_duration(input: &str) -> Result<u64> {
    let bad = || {
        Error::usage(format!(
            "{input:?} is not a duration: use e.g. 15m, 24h or 7d"
        ))
    };
    let split = input.find(|c: char| !c.is_ascii_digit()).ok_or_else(bad)?;
    let (number, unit) = input.split_at(split);
    let number: u64 = number.parse().map_err(|_| bad())?;
    let unit = match unit {
        "s" => 1,
        "m" => 60,
        "h" => 3600,
        "d" => 86_400,
        "w" => 604_800,
        _ => return Err(bad()),
    };
    match number.checked_mul(unit) {
        Some(0) | None => Err(bad()),
        Some(secs) => Ok(secs),
    }
}

/// `5h 12m`, `42m`, `30s`, `3d 4h`: the two largest units.
pub(crate) fn span(secs: u64) -> String {
    const UNITS: [(u64, &str); 4] = [(86_400, "d"), (3600, "h"), (60, "m"), (1, "s")];
    let mut parts = Vec::new();
    let mut rest = secs;
    for (size, unit) in UNITS {
        if rest >= size || (parts.is_empty() && size == 1) {
            parts.push(format!("{}{unit}", rest / size));
            rest %= size;
        }
        if parts.len() == 2 {
            break;
        }
    }
    parts.join(" ")
}

/// `in 5h 12m` or `2m ago`, from unix seconds.
pub(crate) fn relative(at: u64) -> String {
    relative_to(at, now())
}

pub(crate) fn relative_to(at: u64, now: u64) -> String {
    match at.cmp(&now) {
        std::cmp::Ordering::Greater => format!("in {}", span(at - now)),
        _ if now - at < 5 => "just now".into(),
        _ => format!("{} ago", span(now - at)),
    }
}

/// An elapsed time: `0.2s`, `4.1s`, `1m 12s`.
pub(crate) fn elapsed(duration: Duration) -> String {
    let secs = duration.as_secs_f64();
    if secs < 60.0 {
        format!("{secs:.1}s")
    } else {
        span(duration.as_secs())
    }
}

/// `1 member`, `3 members`.
pub(crate) fn plural(count: impl Into<u64>, one: &str, many: &str) -> String {
    let count = count.into();
    format!("{count} {}", if count == 1 { one } else { many })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn amounts_to_base_units() {
        assert_eq!(parse_amount("25", 18).unwrap(), 25_000_000_000_000_000_000);
        assert_eq!(parse_amount("0.5", 2).unwrap(), 50);
        assert_eq!(parse_amount(".5", 1).unwrap(), 5);
        assert_eq!(parse_amount("1_000.25", 2).unwrap(), 100_025);
        assert_eq!(parse_amount("1,000", 0).unwrap(), 1000);
        assert_eq!(parse_amount("1.10", 1).unwrap(), 11);
        assert_eq!(parse_amount("3.", 6).unwrap(), 3_000_000);
        assert!(
            parse_amount("0.001", 2)
                .err()
                .unwrap()
                .message
                .contains("more than 2 decimal places")
        );
        for bad in ["", ".", "-1", "1e3", "abc", "1.2.3"] {
            assert!(parse_amount(bad, 18).is_err(), "{bad}");
        }
        assert!(parse_amount("1000000000000000000000000000000", 18).is_err());
    }

    #[test]
    fn amounts_in_token_units() {
        assert_eq!(amount(25_000_000_000_000_000_000, 18), "25");
        assert_eq!(amount(1500, 3), "1.5");
        assert_eq!(amount(10_000_000, 2), "100,000");
        assert_eq!(amount(5, 2), "0.05");
        assert_eq!(amount(0, 18), "0");
        assert_eq!(amount(123, 0), "123");
    }

    #[test]
    fn durations_and_times() {
        assert_eq!(parse_duration("15m").unwrap(), 900);
        assert_eq!(parse_duration("7d").unwrap(), 604_800);
        for bad in ["0m", "15", "m", "1y"] {
            assert!(parse_duration(bad).is_err(), "{bad}");
        }
        assert_eq!(span(5 * 3600 + 12 * 60 + 9), "5h 12m");
        assert_eq!(span(42), "42s");
        assert_eq!(span(0), "0s");
        assert_eq!(span(3 * 86_400 + 4 * 3600), "3d 4h");
        assert_eq!(relative_to(1000 + 300, 1000), "in 5m");
        assert_eq!(relative_to(1000, 1000 + 120), "2m ago");
        assert_eq!(relative_to(1000, 1002), "just now");
        assert_eq!(elapsed(Duration::from_millis(4100)), "4.1s");
    }

    #[test]
    fn addresses_and_keys() {
        let hex: Hex32 = "0x2b1f000000000000000000000000000000000000000000000000000000009e04"
            .parse()
            .unwrap();
        assert_eq!(short(&hex), "0x2b1f…9e04");
        assert_eq!(id(&hex), "2b1f0000");
        let point = Point { x: hex, y: hex };
        assert_eq!(parse_point(&self::point(&point)).unwrap(), point);
        let joined = format!("0x{}{}", &hex.to_string()[2..], &hex.to_string()[2..]);
        assert_eq!(parse_point(&joined).unwrap(), point);
        assert!(parse_point("0x12").is_err());
    }
}
