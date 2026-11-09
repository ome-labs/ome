//! QR codes in the terminal, two modules per character with half blocks, for invites, pairing and receiving.

use qrcode::{EcLevel, QrCode, render::unicode::Dense1x2};

/// `data` as a QR code, or `None` when it is too long for one.
pub(crate) fn render(data: &str) -> Option<String> {
    let code = QrCode::with_error_correction_level(data.as_bytes(), EcLevel::L).ok()?;
    Some(
        code.render::<Dense1x2>()
            .dark_color(Dense1x2::Light)
            .light_color(Dense1x2::Dark)
            .quiet_zone(true)
            .build(),
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn short_data_renders_and_huge_data_does_not() {
        let code = super::render("0x2b1f").unwrap();
        assert!(code.lines().count() > 10);
        assert!(super::render(&"x".repeat(10_000)).is_none());
    }
}
