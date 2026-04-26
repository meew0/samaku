use crate::{style, subtitle};

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

#[must_use]
pub fn frame_coordinates_to_iced(
    frame_x: f64,
    frame_y: f64,
    size: iced::Size,
    storage_size: subtitle::Resolution,
) -> iced::Point {
    let ui_x: f64 = frame_x * f64::from(size.width) / f64::from(storage_size.x);
    let ui_y: f64 = frame_y * f64::from(size.height) / f64::from(storage_size.y);
    #[expect(
        clippy::cast_possible_truncation,
        reason = "extreme precision not needed in UI-adjacent code"
    )]
    let point = iced::Point::new(ui_x as f32, ui_y as f32);
    point
}
