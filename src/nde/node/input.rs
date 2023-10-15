use crate::{message, model, nde};

use super::{Error, LeafInputType, Node, SocketType, SocketValue};

#[derive(Debug, Clone)]
pub struct Sline {}

impl Node for Sline {
    fn name(&self) -> &'static str {
        "Input: Subtitle line"
    }

    fn desired_inputs(&self) -> &[SocketType] {
        &[SocketType::LeafInput(LeafInputType::Sline)]
    }

    fn predicted_outputs(&self) -> &[SocketType] {
        &[SocketType::IndividualEvent]
    }

    fn run(&self, inputs: &[&SocketValue]) -> Result<Vec<SocketValue>, Error> {
        let SocketValue::Sline(sline) = inputs[0] else {
            return Err(Error::MismatchedTypes);
        };

        let (global, spans) = nde::tags::parse(&sline.text);

        let event = nde::Event {
            start: sline.start,
            duration: sline.duration,
            layer_index: sline.layer_index,
            style_index: sline.style_index,
            margins: sline.margins,
            global_tags: *global,
            overrides: nde::tags::Local::empty(),
            text: spans,
        };
        Ok(vec![SocketValue::IndividualEvent(Box::new(event))])
    }
}

#[derive(Debug, Clone)]
pub struct FrameRate {}

impl Node for FrameRate {
    fn name(&self) -> &'static str {
        "Input: Frame rate"
    }

    fn desired_inputs(&self) -> &[SocketType] {
        &[SocketType::LeafInput(LeafInputType::FrameRate)]
    }

    fn predicted_outputs(&self) -> &[SocketType] {
        &[SocketType::FrameRate]
    }

    fn run(&self, inputs: &[&SocketValue]) -> Result<Vec<SocketValue>, Error> {
        super::retrieve!(inputs[0], SocketValue::FrameRate(frame_rate));
        Ok(vec![SocketValue::FrameRate(*frame_rate)])
    }
}

#[derive(Debug, Clone)]
pub struct Position {
    pub value: nde::tags::Position,
}

impl Node for Position {
    fn name(&self) -> &'static str {
        "Input: Position"
    }

    fn desired_inputs(&self) -> &[SocketType] {
        &[]
    }

    fn predicted_outputs(&self) -> &[SocketType] {
        &[SocketType::Position]
    }

    fn run(&self, _inputs: &[&SocketValue]) -> Result<Vec<SocketValue>, Error> {
        Ok(vec![SocketValue::Position(self.value)])
    }

    fn content<'a>(
        &self,
        self_index: usize,
    ) -> iced::Element<'a, message::Message, iced::Renderer> {
        let button = iced::widget::button("Set").on_press(message::Message::SetReticules(
            model::reticule::Reticules {
                list: vec![model::reticule::Reticule {
                    shape: model::reticule::Shape::Cross,
                    position: self.value,
                    radius: 10.0,
                }],
                source_node_index: self_index,
            },
        ));

        let column = iced::widget::column![
            iced::widget::text(self.name()),
            iced::widget::text(format!("x: {:.1}, y: {:.1}", self.value.x, self.value.y)),
            button
        ];

        column.align_items(iced::Alignment::Center).into()
    }

    fn reticule_update(
        &mut self,
        reticules: &mut model::reticule::Reticules,
        index: usize,
        new_position: nde::tags::Position,
    ) {
        if index != 0 {
            return;
        }

        reticules.list[0].position = new_position;
        self.value = new_position;
    }

    fn content_size(&self) -> iced::Size {
        iced::Size::new(200.0, 125.0)
    }
}

#[derive(Debug, Clone)]
pub struct Rectangle {
    pub value: nde::tags::Rectangle,
}

impl Rectangle {
    fn reticule_update_internal(&self, reticules: &mut [model::reticule::Reticule]) {
        reticules[0].position = nde::tags::Position {
            x: f64::from(self.value.x1),
            y: f64::from(self.value.y1),
        };
        reticules[1].position = nde::tags::Position {
            x: f64::from(self.value.x2),
            y: f64::from(self.value.y1),
        };
        reticules[2].position = nde::tags::Position {
            x: f64::from(self.value.x1),
            y: f64::from(self.value.y2),
        };
        reticules[3].position = nde::tags::Position {
            x: f64::from(self.value.x2),
            y: f64::from(self.value.y2),
        };
    }
}

impl Node for Rectangle {
    fn name(&self) -> &'static str {
        "Input: Rectangle"
    }

    fn desired_inputs(&self) -> &[SocketType] {
        &[]
    }

    fn predicted_outputs(&self) -> &[SocketType] {
        &[SocketType::Rectangle]
    }

    fn run(&self, _inputs: &[&SocketValue]) -> Result<Vec<SocketValue>, Error> {
        Ok(vec![SocketValue::Rectangle(self.value)])
    }

    fn content<'a>(
        &self,
        self_index: usize,
    ) -> iced::Element<'a, message::Message, iced::Renderer> {
        let mut reticules = vec![
            model::reticule::Reticule {
                shape: model::reticule::Shape::CornerTopLeft,
                position: nde::tags::Position::default(),
                radius: 7.0,
            },
            model::reticule::Reticule {
                shape: model::reticule::Shape::CornerTopRight,
                position: nde::tags::Position::default(),
                radius: 7.0,
            },
            model::reticule::Reticule {
                shape: model::reticule::Shape::CornerBottomLeft,
                position: nde::tags::Position::default(),
                radius: 7.0,
            },
            model::reticule::Reticule {
                shape: model::reticule::Shape::CornerBottomRight,
                position: nde::tags::Position::default(),
                radius: 7.0,
            },
        ];

        self.reticule_update_internal(&mut reticules);

        let button = iced::widget::button("Set").on_press(message::Message::SetReticules(
            model::reticule::Reticules {
                list: reticules,
                source_node_index: self_index,
            },
        ));

        let column = iced::widget::column![
            iced::widget::text(self.name()),
            iced::widget::text(format!(
                "({:.1}, {:.1}; {:.1}, {:.1})",
                self.value.x1, self.value.y1, self.value.x2, self.value.y2,
            )),
            button
        ];

        column.align_items(iced::Alignment::Center).into()
    }

    fn reticule_update(
        &mut self,
        reticules: &mut model::reticule::Reticules,
        index: usize,
        new_position: nde::tags::Position,
    ) {
        #[allow(clippy::cast_possible_truncation)]
        match index {
            0 => {
                // top left
                self.value.x1 = new_position.x as i32;
                self.value.y1 = new_position.y as i32;
            }
            1 => {
                // top right
                self.value.x2 = new_position.x as i32;
                self.value.y1 = new_position.y as i32;
            }
            2 => {
                // bottom left
                self.value.x1 = new_position.x as i32;
                self.value.y2 = new_position.y as i32;
            }
            3 => {
                // bottom right
                self.value.x2 = new_position.x as i32;
                self.value.y2 = new_position.y as i32;
            }
            _ => {}
        }

        self.reticule_update_internal(&mut reticules.list);
    }

    fn content_size(&self) -> iced::Size {
        iced::Size::new(200.0, 125.0)
    }
}
