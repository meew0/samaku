use crate::{nde, subtitle};

#[derive(Debug, Clone)]
pub struct Reticules {
    pub list: Vec<Reticule>,
    pub source_node_index: nde::graph::NodeId,
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
        #[expect(
            clippy::cast_possible_truncation,
            reason = "extreme precision not needed in UI-adjacent code"
        )]
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
    CornerTopLeft,
    CornerTopRight,
    CornerBottomLeft,
    CornerBottomRight,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_reticule(x: f64, y: f64) -> Reticule {
        Reticule {
            shape: Shape::Cross,
            position: nde::tags::Position { x, y },
            radius: 5.0,
        }
    }

    #[test]
    fn iced_position_identity() {
        // Same storage and display size: coordinates should be unchanged.
        let reticule = make_reticule(100.0, 200.0);
        let storage = subtitle::Resolution { x: 1920, y: 1080 };
        let size = iced::Size {
            width: 1920.0,
            height: 1080.0,
        };
        let point = reticule.iced_position(size, storage);
        assert!((f64::from(point.x) - 100.0).abs() < 0.01);
        assert!((f64::from(point.y) - 200.0).abs() < 0.01);
    }

    #[test]
    fn iced_position_scaled_down() {
        // Display is half the storage resolution: coordinates should be halved.
        let reticule = make_reticule(960.0, 540.0);
        let storage = subtitle::Resolution { x: 1920, y: 1080 };
        let size = iced::Size {
            width: 960.0,
            height: 540.0,
        };
        let point = reticule.iced_position(size, storage);
        assert!((f64::from(point.x) - 480.0).abs() < 0.01);
        assert!((f64::from(point.y) - 270.0).abs() < 0.01);
    }

    #[test]
    fn position_from_iced_no_offset() {
        // With a 1:1 ratio and no offset, iced point maps directly to storage position.
        let storage = subtitle::Resolution { x: 1920, y: 1080 };
        let size = iced::Size {
            width: 1920.0,
            height: 1080.0,
        };
        let point = iced::Point { x: 350.0, y: 720.0 };
        let position =
            Reticule::position_from_iced(point, iced::Vector { x: 0.0, y: 0.0 }, size, storage);
        assert!((position.x - 350.0).abs() < 0.01);
        assert!((position.y - 720.0).abs() < 0.01);
    }

    #[test]
    fn position_from_iced_with_offset() {
        // The offset is subtracted before the scale, so point (200, 300) with offset (100, 50)
        // maps to effective point (100, 250).
        let storage = subtitle::Resolution { x: 1920, y: 1080 };
        let size = iced::Size {
            width: 1920.0,
            height: 1080.0,
        };
        let point = iced::Point { x: 200.0, y: 300.0 };
        let offset = iced::Vector { x: 100.0, y: 50.0 };
        let position = Reticule::position_from_iced(point, offset, size, storage);
        assert!((position.x - 100.0).abs() < 0.01);
        assert!((position.y - 250.0).abs() < 0.01);
    }

    #[test]
    fn iced_position_round_trip() {
        // Converting storage → iced → storage should recover the original position.
        let storage = subtitle::Resolution { x: 1920, y: 1080 };
        let size = iced::Size {
            width: 1280.0,
            height: 720.0,
        };
        let original = nde::tags::Position { x: 480.0, y: 270.0 };
        let reticule = make_reticule(original.x, original.y);
        let iced_point = reticule.iced_position(size, storage);
        let back = Reticule::position_from_iced(
            iced_point,
            iced::Vector { x: 0.0, y: 0.0 },
            size,
            storage,
        );
        assert!((back.x - original.x).abs() < 0.1);
        assert!((back.y - original.y).abs() < 0.1);
    }
}
