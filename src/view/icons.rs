//! Icon characters and the font that should be used to render them.
//!
//! The icon font we use is Bootstrap's (see [`resources::BOOTSTRAP_ICONS`]).

use crate::message;

/// The icon font that should be used to render the icon characters,
/// such that they actually show up as icons rather than unrelated
/// fallback glyphs.
pub const FONT: iced::Font = iced::Font::with_name("bootstrap-icons");

#[repr(u32)]
#[expect(clippy::min_ident_chars, reason = "match bootstrap names")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    // https://icons.getbootstrap.com
    ArrowBarLeft = 0xf113,
    ArrowBarRight = 0xf114,
    BoxArrowInLeft = 0xf1bd,
    BoxArrowInRight = 0xf1be,
    CaretLeftFill = 0xf22d,
    CaretRightFill = 0xf231,
    Dash = 0xf2ea,
    List = 0xf479,
    Pause = 0xf4c4,
    Play = 0xf4f5,
    Plus = 0xf4fe,
    PlusLg = 0xf64d,
    Trash = 0xf5de,
    X = 0xf62a,
}

impl Icon {
    /// Get the code point representing this icon in the [`FONT`].
    ///
    /// # Panics
    /// This function should never panic.
    #[must_use]
    pub fn character(self) -> char {
        char::from_u32(self as u32).unwrap()
    }

    /// Create a text widget containing this icon.
    #[must_use]
    pub fn text<'a>(self) -> iced::widget::Text<'a> {
        iced::widget::text(self.character()).font(FONT)
    }

    /// Create a button with this icon as its label.
    #[must_use]
    pub fn button<'a>(self) -> iced::widget::Button<'a, message::Message> {
        iced::widget::button(
            self.text()
                .align_x(iced::alignment::Horizontal::Center)
                .align_y(iced::alignment::Vertical::Center),
        )
    }
}
