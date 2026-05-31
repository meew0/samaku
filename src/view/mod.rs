use crate::{message, subtitle};

pub mod icons;
pub mod menu;
pub mod toast;
pub mod widget;

pub use icons::Icon;

/// Create a half-pixel thick horizontal separator line.
#[must_use]
pub fn separator() -> iced::widget::rule::Rule<'static> {
    iced::widget::rule::horizontal(0.5).style(iced::widget::rule::weak)
}

/// Create a half-pixel thick vertical separator line.
#[must_use]
pub fn vertical_separator() -> iced::widget::rule::Rule<'static> {
    iced::widget::rule::vertical(0.5).style(iced::widget::rule::weak)
}

#[must_use]
pub fn frame_coordinates_to_iced(
    frame_point: glam::DVec2,
    size: iced::Size,
    storage_size: subtitle::Resolution,
) -> iced::Point {
    let ui_x: f64 = frame_point.x * f64::from(size.width) / f64::from(storage_size.x);
    let ui_y: f64 = frame_point.y * f64::from(size.height) / f64::from(storage_size.y);
    #[expect(
        clippy::cast_possible_truncation,
        reason = "extreme precision not needed in UI-adjacent code"
    )]
    let point = iced::Point::new(ui_x as f32, ui_y as f32);
    point
}

/// Make the given element display a tooltip when hovered.
#[must_use]
pub fn tooltip<
    'a,
    E: Into<iced::Element<'a, message::Message>>,
    T: iced::widget::text::IntoFragment<'a>,
>(
    content: E,
    tooltip: T,
) -> iced::Element<'a, message::Message> {
    iced::widget::tooltip(
        content,
        iced::widget::container(iced::widget::text(tooltip))
            .padding(10)
            .style(iced::widget::container::bordered_box),
        iced::widget::tooltip::Position::Top,
    )
    .into()
}

/// Returns a small section header.
#[must_use]
pub fn section_label(label: &str) -> iced::widget::Text<'_> {
    iced::widget::text(label).size(13)
}

/// Element that can be opened and closed, to reveal some content inside.
pub fn expando<'a, E: Into<iced::Element<'a, message::Message>>>(
    open: bool,
    self_pane: crate::pane::Pane,
    toggle_pane_message: message::Pane,
    header: &'a str,
    content: E,
) -> iced::Element<'a, message::Message> {
    let chevron = if open {
        Icon::ChevronDown
    } else {
        Icon::ChevronRight
    };

    let header_element = iced::widget::mouse_area(
        iced::widget::row![chevron.text().size(11.0), section_label(header),]
            .align_y(iced::Alignment::Center)
            .spacing(5.0),
    )
    .on_press(message::Message::Pane(self_pane, toggle_pane_message))
    .interaction(iced::mouse::Interaction::Pointer);

    let content_element = content.into();

    if open {
        iced::widget::column![header_element, content_element]
            .spacing(5.0)
            .into()
    } else {
        header_element.into()
    }
}
