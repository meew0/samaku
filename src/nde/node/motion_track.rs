use crate::{message, model, nde};

use super::{Error, Node, SocketType, SocketValue};

#[derive(Debug)]
pub struct MotionTrack {}

impl Node for MotionTrack {
    fn name(&self) -> &'static str {
        "Motion track [NYI]"
    }

    fn desired_inputs(&self) -> &[SocketType] {
        &[SocketType::GenericEvents]
    }

    fn predicted_outputs(&self) -> &[SocketType] {
        &[SocketType::GenericEvents]
    }

    fn run(&self, inputs: &[&SocketValue]) -> Result<Vec<SocketValue>, Error> {
        let cloned = inputs[0].map_events(std::clone::Clone::clone)?;
        Ok(vec![cloned])
    }

    fn content<'a>(
        &self,
        self_index: usize,
    ) -> iced::Element<'a, message::Message, iced::Renderer> {
        let button = iced::widget::button("Track").on_press(message::Message::None);

        let column = iced::widget::column![iced::widget::text(self.name()), button];

        column.align_items(iced::Alignment::Center).into()
    }

    fn reticule_update(
        &mut self,
        reticules: &mut model::reticule::Reticules,
        index: usize,
        new_position: nde::tags::Position,
    ) {
        // TODO
    }

    fn content_size(&self) -> iced::Size {
        iced::Size::new(200.0, 125.0)
    }
}
