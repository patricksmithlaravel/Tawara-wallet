//! The bundled fonts (docs/DECISIONS.md D27, item 6) and the faces the
//! screens set text in (`design/TOKENS.md` section 2).
//!
//! The files are in `crates/app/fonts/`, each family with its SIL Open Font
//! License 1.1 beside it; `crates/app/fonts/SOURCES.md` says where each came
//! from. They are loaded into iced's font system at start-up ([`FILES`]), and
//! every piece of text names one of the faces below, so nothing is drawn
//! from a font installed on the machine.

use iced::Font;
use iced::font::{Family, Stretch, Style, Weight};

/// Every bundled font file, to load at start-up.
pub const FILES: [&[u8]; 10] = [
    include_bytes!("../fonts/Poppins-Regular.ttf"),
    include_bytes!("../fonts/Poppins-Medium.ttf"),
    include_bytes!("../fonts/Poppins-SemiBold.ttf"),
    include_bytes!("../fonts/Montserrat-Medium.ttf"),
    include_bytes!("../fonts/Montserrat-SemiBold.ttf"),
    include_bytes!("../fonts/Montserrat-Bold.ttf"),
    include_bytes!("../fonts/Montserrat-MediumItalic.ttf"),
    include_bytes!("../fonts/Montserrat-BoldItalic.ttf"),
    include_bytes!("../fonts/IBMPlexMono-Regular.ttf"),
    include_bytes!("../fonts/IBMPlexMono-Medium.ttf"),
];

const fn face(family: &'static str, weight: Weight, style: Style) -> Font {
    Font {
        family: Family::Name(family),
        weight,
        stretch: Stretch::Normal,
        style,
    }
}

/// Body text: Poppins 400 (`font_body`).
pub const BODY: Font = face("Poppins", Weight::Normal, Style::Normal);
/// Labels, nav, chips: Poppins 500.
pub const BODY_MEDIUM: Font = face("Poppins", Weight::Medium, Style::Normal);
/// Active nav, badges, selected tabs: Poppins 600.
pub const BODY_SEMIBOLD: Font = face("Poppins", Weight::Semibold, Style::Normal);

/// Montserrat 500 (`font_display`), and the frames' unweighted values.
pub const DISPLAY_MEDIUM: Font = face("Montserrat", Weight::Medium, Style::Normal);
/// Section labels, callout titles, amounts in tables: Montserrat 600.
pub const DISPLAY_SEMIBOLD: Font = face("Montserrat", Weight::Semibold, Style::Normal);
/// Titles, stat values, the balance, primary buttons: Montserrat 700.
pub const DISPLAY_BOLD: Font = face("Montserrat", Weight::Bold, Style::Normal);
/// The block poem: Montserrat 500 italic.
pub const DISPLAY_MEDIUM_ITALIC: Font = face("Montserrat", Weight::Medium, Style::Italic);
/// The first-run headline's second line: Montserrat 700 italic.
pub const DISPLAY_BOLD_ITALIC: Font = face("Montserrat", Weight::Bold, Style::Italic);

/// Addresses, tags, hashes, indexes, the library's pages: IBM Plex Mono
/// 400 (`font_mono`).
pub const MONO: Font = face("IBM Plex Mono", Weight::Normal, Style::Normal);
/// Block links and the tag heading: IBM Plex Mono 500.
pub const MONO_MEDIUM: Font = face("IBM Plex Mono", Weight::Medium, Style::Normal);

/// The face for "→" (U+2192) beside Poppins text: Poppins has no such
/// glyph, and the renderings draw it from a system font (`design/TOKENS.md`
/// section 9), which would make the screenshots differ from machine to
/// machine.
pub const ARROW: Font = DISPLAY_MEDIUM;

#[cfg(test)]
mod tests {
    use super::*;

    /// Every file is a TrueType font: the right magic number, and large
    /// enough to hold one.
    #[test]
    fn every_bundled_file_is_a_truetype_font() {
        for bytes in FILES {
            assert!(bytes.len() > 100_000, "{} bytes", bytes.len());
            assert_eq!(&bytes[..4], &[0, 1, 0, 0], "a TrueType outline font");
        }
    }
}
