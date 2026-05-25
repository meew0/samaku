//! Icon characters and the font that should be used to render them.
//!
//! The icon font we use is Bootstrap's (see [`resources::BOOTSTRAP_ICONS`]).

/// The icon font that should be used to render the icon characters,
/// such that they actually show up as icons rather than unrelated
/// fallback glyphs.
pub const FONT: iced::Font = iced::Font::with_name("bootstrap-icons");

// https://icons.getbootstrap.com
pub const CARET_RIGHT_FILL: char = '\u{f231}';
pub const DASH: char = '\u{f2ea}';
pub const LIST: char = '\u{f479}';
pub const PLUS: char = '\u{f4fe}';
pub const PLUS_LG: char = '\u{f64d}';
pub const TRASH: char = '\u{f5de}';
#[expect(clippy::min_ident_chars, reason = "matches bootstrap name")]
pub const X: char = '\u{f62a}';
