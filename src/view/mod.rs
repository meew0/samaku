use crate::{message, style, subtitle};

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
