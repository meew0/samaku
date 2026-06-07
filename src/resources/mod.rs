/// The TTF file for Barlow, our default font for the UI and for subtitles.
///
/// For details on the font, see https://github.com/jpt/barlow.
pub const BARLOW_REGULAR: &[u8] = include_bytes!("barlow/Barlow-Regular.ttf");
pub const BARLOW_ITALIC: &[u8] = include_bytes!("barlow/Barlow-Italic.ttf");
pub const BARLOW_BOLD: &[u8] = include_bytes!("barlow/Barlow-Bold.ttf");
pub const BARLOW_BOLD_ITALIC: &[u8] = include_bytes!("barlow/Barlow-BoldItalic.ttf");

pub const BARLOW_VARIANTS: &[&[u8]] = &[
    BARLOW_REGULAR,
    BARLOW_ITALIC,
    BARLOW_BOLD,
    BARLOW_BOLD_ITALIC,
];

/// The TTF file for Bootstrap's icon font which we use to render icons in the UI.
pub const BOOTSTRAP_ICONS: &[u8] = include_bytes!("bootstrap-icons/bootstrap-icons-1.13.1.ttf");

/// The samaku logo, as SVG bytes.
pub const LOGO: &[u8] = include_bytes!("logo.svg");
