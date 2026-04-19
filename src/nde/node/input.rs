use super::{BasicError, LeafInputType, Node, Shell, SocketType, SocketValue};
use crate::model::reticule;
use crate::{message, nde, subtitle};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct InputEvent;

#[typetag::serde]
impl Node for InputEvent {
    fn name(&self) -> &'static str {
        "Input: Subtitle line"
    }

    fn desired_inputs(&self) -> &[SocketType] {
        &[SocketType::LeafInput(LeafInputType::Event)]
    }

    fn predicted_outputs(&self) -> &[SocketType] {
        &[SocketType::IndividualEvent]
    }

    fn run(&'_ self, inputs: &[&SocketValue]) -> anyhow::Result<Vec<SocketValue<'_>>> {
        let SocketValue::SourceEvent(source_event) = inputs[0] else {
            return Err(BasicError::MismatchedTypes.into());
        };

        let (global, spans) = nde::tags::parse(&source_event.text);

        let event = nde::Event {
            start: source_event.start,
            duration: source_event.duration,
            layer_index: source_event.layer_index,
            style_index: source_event.style_index,
            margins: source_event.margins,
            global_tags: *global,
            overrides: nde::tags::Local::empty(),
            text: spans,
        };
        Ok(vec![SocketValue::IndividualEvent(Box::new(event))])
    }
}

inventory::submit! {
    Shell::new(
        &["Input", "Subtitle line"],
        || Box::new(InputEvent {})
    )
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct InputFrameRate;

#[typetag::serde]
impl Node for InputFrameRate {
    fn name(&self) -> &'static str {
        "Input: Frame rate"
    }

    fn desired_inputs(&self) -> &[SocketType] {
        &[SocketType::LeafInput(LeafInputType::FrameRate)]
    }

    fn predicted_outputs(&self) -> &[SocketType] {
        &[SocketType::FrameRate]
    }

    fn run(&'_ self, inputs: &[&SocketValue]) -> anyhow::Result<Vec<SocketValue<'_>>> {
        super::retrieve!(inputs[0], SocketValue::FrameRate(frame_rate));
        Ok(vec![SocketValue::FrameRate(*frame_rate)])
    }
}

inventory::submit! {
    Shell::new(
        &["Input", "Frame rate"],
        || Box::new(InputFrameRate {})
    )
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct InputPosition {
    pub value: nde::tags::Position,
}

#[typetag::serde]
impl Node for InputPosition {
    fn name(&self) -> &'static str {
        "Input: Position"
    }

    fn desired_inputs(&self) -> &[SocketType] {
        &[]
    }

    fn predicted_outputs(&self) -> &[SocketType] {
        &[SocketType::Position]
    }

    fn run(&'_ self, _inputs: &[&SocketValue]) -> anyhow::Result<Vec<SocketValue<'_>>> {
        Ok(vec![SocketValue::Position(self.value)])
    }

    fn content<'a>(
        &self,
        filter_index: subtitle::ExtradataId,
        self_index: nde::graph::NodeId,
    ) -> iced::Element<'a, message::Message> {
        let button = iced::widget::button("Set").on_press(message::Message::SetReticules(
            reticule::Reticules::new(
                vec![reticule::Reticule {
                    shape: reticule::Shape::Cross,
                    position: self.value,
                    radius: 15.0,
                }],
                filter_index,
                self_index,
            ),
        ));

        let column = iced::widget::column![
            iced::widget::text(format!("x: {:.1}, y: {:.1}", self.value.x, self.value.y)),
            button
        ];

        column
            .spacing(4.0)
            .width(iced::Length::Fill)
            .align_x(iced::Alignment::Center)
            .into()
    }

    fn reticule_update(
        &mut self,
        reticules: &mut reticule::Reticules,
        index: reticule::Index,
        new_position: nde::tags::Position,
    ) -> anyhow::Result<nde::tags::Position> {
        if index.0 != 0 {
            anyhow::bail!("Reticule index out of range: {index}");
        }

        let old_position = std::mem::replace(&mut reticules[index].position, new_position);
        self.value = new_position;

        Ok(old_position)
    }

    fn content_size(&self) -> iced::Size {
        iced::Size::new(200.0, 125.0)
    }
}

inventory::submit! {
    Shell::new(
        &["Input", "Position"],
        || Box::new(InputPosition {
            value: nde::tags::Position {
                x: 100.0,
                y: 100.0,
            }
        })
    )
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct InputRectangle {
    pub value: nde::tags::Rectangle,
}

impl InputRectangle {
    fn reticule_update_internal(&self, reticules: &mut [reticule::Reticule]) {
        assert!(reticules.len() > 3); // Elide bounds checks

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

#[typetag::serde]
impl Node for InputRectangle {
    fn name(&self) -> &'static str {
        "Input: Rectangle"
    }

    fn desired_inputs(&self) -> &[SocketType] {
        &[]
    }

    fn predicted_outputs(&self) -> &[SocketType] {
        &[SocketType::Rectangle]
    }

    fn run(&'_ self, _inputs: &[&SocketValue]) -> anyhow::Result<Vec<SocketValue<'_>>> {
        Ok(vec![SocketValue::Rectangle(self.value)])
    }

    fn content<'a>(
        &self,
        filter_index: subtitle::ExtradataId,
        self_index: nde::graph::NodeId,
    ) -> iced::Element<'a, message::Message> {
        let mut reticules = vec![
            reticule::Reticule {
                shape: reticule::Shape::CornerTopLeft,
                position: nde::tags::Position::default(),
                radius: 15.0,
            },
            reticule::Reticule {
                shape: reticule::Shape::CornerTopRight,
                position: nde::tags::Position::default(),
                radius: 15.0,
            },
            reticule::Reticule {
                shape: reticule::Shape::CornerBottomLeft,
                position: nde::tags::Position::default(),
                radius: 15.0,
            },
            reticule::Reticule {
                shape: reticule::Shape::CornerBottomRight,
                position: nde::tags::Position::default(),
                radius: 15.0,
            },
        ];

        self.reticule_update_internal(&mut reticules);

        let button = iced::widget::button("Set").on_press(message::Message::SetReticules(
            reticule::Reticules::new(reticules, filter_index, self_index),
        ));

        let column = iced::widget::column![
            iced::widget::text(format!(
                "({:.1}, {:.1}; {:.1}, {:.1})",
                self.value.x1, self.value.y1, self.value.x2, self.value.y2,
            )),
            button
        ];

        column
            .spacing(4.0)
            .width(iced::Length::Fill)
            .align_x(iced::Alignment::Center)
            .into()
    }

    fn reticule_update(
        &mut self,
        reticules: &mut reticule::Reticules,
        index: reticule::Index,
        new_position: nde::tags::Position,
    ) -> anyhow::Result<nde::tags::Position> {
        let mut new_value = self.value;

        let (x_mut, y_mut) = match index.0 {
            0 => {
                // top left
                (&mut new_value.x1, &mut new_value.y1)
            }
            1 => {
                // top right
                (&mut new_value.x2, &mut new_value.y1)
            }
            2 => {
                // bottom left
                (&mut new_value.x1, &mut new_value.y2)
            }
            3 => {
                // bottom right
                (&mut new_value.x2, &mut new_value.y2)
            }
            _ => {
                anyhow::bail!("Reticule index out of range: {}", index.0);
            }
        };

        #[expect(
            clippy::cast_possible_truncation,
            reason = "extremely large values not expected in UI code"
        )]
        let old_x = std::mem::replace(x_mut, new_position.x as i32);

        #[expect(
            clippy::cast_possible_truncation,
            reason = "extremely large values not expected in UI code"
        )]
        let old_y = std::mem::replace(y_mut, new_position.y as i32);

        if new_value.is_positive() {
            self.value = new_value;
            self.reticule_update_internal(&mut reticules.list);
        }

        Ok(nde::tags::Position {
            x: f64::from(old_x),
            y: f64::from(old_y),
        })
    }

    fn content_size(&self) -> iced::Size {
        iced::Size::new(200.0, 125.0)
    }
}

inventory::submit! {
    Shell::new(
        &["Input", "Rectangle"],
        || Box::new(InputRectangle {
            value: nde::tags::Rectangle {
                x1: 100,
                y1: 100,
                x2: 200,
                y2: 200,
            }
        })
    )
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct InputTags {
    pub value: String,
}

#[typetag::serde]
impl Node for InputTags {
    fn name(&self) -> &'static str {
        "Input: Tags"
    }

    fn desired_inputs(&self) -> &[SocketType] {
        &[]
    }

    fn predicted_outputs(&self) -> &[SocketType] {
        &[SocketType::LocalTags, SocketType::GlobalTags]
    }

    fn run(&'_ self, _inputs: &[&SocketValue]) -> anyhow::Result<Vec<SocketValue<'_>>> {
        if self.value.contains('{') || self.value.contains('}') {
            anyhow::bail!("Input tags contain brackets");
        }

        let block = format!("{{{}}}", self.value);
        let (global, spans) = nde::tags::parse_raw(&block);

        assert_eq!(spans.len(), 2);
        let nde::Span::Tags(local, _) = spans.into_iter().nth(1).unwrap() else {
            panic!("span should be `Tags`")
        };

        Ok(vec![
            SocketValue::LocalTags(Box::new(local)),
            SocketValue::GlobalTags(global),
        ])
    }

    fn content<'a>(
        &self,
        filter_index: subtitle::ExtradataId,
        self_index: nde::graph::NodeId,
    ) -> iced::Element<'a, message::Message> {
        let input =
            iced::widget::text_input("\\1c&HFF0000&", &self.value).on_input(move |new_text| {
                message::Message::Node(
                    filter_index,
                    self_index,
                    message::Node::TextInputChanged(new_text),
                )
            });

        let column = iced::widget::column![input];

        column
            .spacing(4.0)
            .width(iced::Length::Fill)
            .align_x(iced::Alignment::Center)
            .into()
    }

    fn update(&mut self, message: message::Node) -> anyhow::Result<()> {
        if let message::Node::TextInputChanged(value) = message {
            self.value = value;
            Ok(())
        } else {
            anyhow::bail!("Invalid message type, expected TextInputChanged");
        }
    }

    fn content_size(&self) -> iced::Size {
        iced::Size::new(400.0, 125.0)
    }
}

inventory::submit! {
    Shell::new(
        &["Input", "Tags"],
        || Box::new(InputTags {
            value: String::new()
        })
    )
}
