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
        quad_color: iced::Background::Color(
            style::samaku_theme()
                .extended_palette()
                .background
                .weak
                .color,
        ),
        inner_bounds: iced_aw::widgets::common::InnerBounds::Ratio(1.0, 1.0),
        ..Default::default()
    }
}

/// Create a text widget that shows an icon.
#[must_use]
pub fn icon<'a, Renderer>(icon: iced_aw::Bootstrap) -> iced::widget::Text<'a, iced::Theme, Renderer>
where
    Renderer: iced::advanced::text::Renderer,
    Renderer::Font: From<iced::Font>,
{
    iced::widget::text(iced_aw::core::icons::bootstrap::icon_to_char(icon).to_string())
        .font(iced_aw::BOOTSTRAP_FONT)
        .horizontal_alignment(iced::alignment::Horizontal::Center)
        .width(iced::Length::Fill)
}
