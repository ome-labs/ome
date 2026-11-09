//! QR codes in the terminal, only where a phone scans them (pairing, receiving): two modules per character with half
//! blocks, a two-module margin instead of the usual four, and nothing when the code would not fit the terminal.

use qrcode::{EcLevel, QrCode, render::unicode::Dense1x2};

/// Light modules on either side; scanners read two as reliably as the standard four on a screen.
const MARGIN: usize = 2;

/// `data` as a QR code, or `None` when it is too long for one or too wide for the terminal.
pub(crate) fn render(data: &str) -> Option<String> {
    let columns = console::Term::stderr()
        .size_checked()
        .map(|(_, c)| usize::from(c));
    render_within(data, columns)
}

fn render_within(data: &str, columns: Option<usize>) -> Option<String> {
    let code = QrCode::with_error_correction_level(data.as_bytes(), EcLevel::L).ok()?;
    // Light on dark: the terminal's dark background is the code's dark modules.
    let body = code
        .render::<Dense1x2>()
        .dark_color(Dense1x2::Light)
        .light_color(Dense1x2::Dark)
        .quiet_zone(false)
        .build();
    let width = code.width() + 2 * MARGIN;
    // The rail and its indent take 3 columns.
    if columns.is_some_and(|c| width + 3 > c) {
        return None;
    }
    let side = "█".repeat(MARGIN);
    let edge = "█".repeat(width);
    let mut lines = vec![edge.clone()];
    lines.extend(body.lines().map(|line| format!("{side}{line}{side}")));
    lines.push(edge);
    Some(lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::render_within;

    #[test]
    fn a_short_code_is_compact_and_a_long_one_is_left_out() {
        let code = render_within("0x2b1f", None).unwrap();
        let width = code.lines().next().unwrap().chars().count();
        // Version 1 is 21 modules, plus the margins.
        assert_eq!(width, 25);
        assert!(code.lines().all(|l| l.chars().count() == width));
        assert!(render_within(&"x".repeat(10_000), None).is_none());
        assert!(render_within("0x2b1f", Some(20)).is_none());
    }
}
