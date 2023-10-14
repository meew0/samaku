use crate::{nde, subtitle};

#[derive(Debug, Clone)]
pub struct Reticules {
    pub list: Vec<Reticule>,
    pub source_node_index: usize,
}

#[derive(Debug, Clone)]
pub struct Reticule {
    pub shape: Shape,
    pub position: nde::tags::Position,
    pub radius: f32,
}

impl Reticule {
    #[must_use]
    pub fn iced_position(
        &self,
        size: iced::Size,
        storage_size: subtitle::Resolution,
    ) -> iced::Point {
        let x: f64 = self.position.x * f64::from(size.width) / f64::from(storage_size.x);
        let y: f64 = self.position.y * f64::from(size.height) / f64::from(storage_size.y);
        #[allow(clippy::cast_possible_truncation)]
        let point = iced::Point::new(x as f32, y as f32);
        point
    }

    #[must_use]
    pub fn position_from_iced(
        iced_point: iced::Point,
        offset: iced::Vector,
        size: iced::Size,
        storage_size: subtitle::Resolution,
    ) -> nde::tags::Position {
        let x: f64 = (f64::from(iced_point.x) - f64::from(offset.x)) * f64::from(storage_size.x)
            / f64::from(size.width);
        let y: f64 = (f64::from(iced_point.y) - f64::from(offset.y)) * f64::from(storage_size.y)
            / f64::from(size.height);

        nde::tags::Position { x, y }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Shape {
    Cross,
}
