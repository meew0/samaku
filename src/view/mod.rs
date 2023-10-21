use crate::style;

pub mod menu;
pub mod toast;
pub mod widget;

/// Create a half-pixel thick horizontal separator line.
#[must_use]
pub fn separator() -> iced_aw::quad::Quad {
    iced_aw::quad::Quad {
        width: iced::Length::Fill,
        height: iced::Length::Fixed(0.5),
        color: style::samaku_theme()
            .extended_palette()
            .background
            .weak
            .color,
        inner_bounds: iced_aw::quad::InnerBounds::Ratio(1.0, 1.0),
        ..Default::default()
    }
}

/// Create a text widget that shows an icon.
#[must_use]
pub fn icon<'a, R>(icon: iced_aw::Icon) -> iced::widget::Text<'a, R>
where
    R: iced::advanced::text::Renderer,
    R::Theme: iced::widget::text::StyleSheet,
    R::Font: From<iced::Font>,
{
    iced::widget::text(iced_aw::graphics::icons::icon_to_char(icon).to_string())
        .font(iced_aw::graphics::icons::ICON_FONT)
}
