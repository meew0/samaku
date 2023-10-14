use super::{Error, Node, SocketType, SocketValue};
use crate::nde;

#[derive(Debug)]
pub struct SetPosition {}

impl Node for SetPosition {
    fn name(&self) -> &'static str {
        "Set position"
    }

    fn desired_inputs(&self) -> &[SocketType] {
        &[SocketType::GenericEvents, SocketType::Position]
    }

    fn predicted_outputs(&self) -> &[SocketType] {
        &[SocketType::GenericEvents]
    }

    fn run(&self, inputs: &[&SocketValue]) -> Result<Vec<SocketValue>, Error> {
        if let SocketValue::Position(position) = inputs[1] {
            let socket_value = inputs[0].map_events(|event| {
                let mut new_event = event.clone();
                new_event.global_tags.position =
                    Some(nde::tags::PositionOrMove::Position(*position));
                new_event
            })?;
            Ok(vec![socket_value])
        } else {
            Err(Error::MismatchedTypes)
        }
    }
}
