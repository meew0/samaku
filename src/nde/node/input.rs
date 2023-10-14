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
        let button = iced::widget::button("Set").on_press(message::Message::SetReticules(vec![
            model::reticule::Reticule {
                shape: model::reticule::Shape::Cross,
                position: self.value,
                radius: 10.0,
                source_node_index: self_index,
            },
        ]));

        let column = iced::widget::column![
            iced::widget::text(self.name()),
            iced::widget::text(format!("x: {:.1}, y: {:.1}", self.value.x, self.value.y)),
            button
        ];

        column.align_items(iced::Alignment::Center).into()
    }

    fn reticule_update(&mut self, reticules: &[model::reticule::Reticule]) {
        self.value = reticules[0].position;
    }

    fn content_size(&self) -> iced::Size {
        iced::Size::new(200.0, 125.0)
    }
}
