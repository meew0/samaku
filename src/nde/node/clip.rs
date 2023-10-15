use crate::nde;

use super::{Error, Node, SocketType, SocketValue};

#[derive(Debug)]
pub struct Rectangle {}

impl Node for Rectangle {
    fn name(&self) -> &'static str {
        "Rectangular clip"
    }

    fn desired_inputs(&self) -> &[SocketType] {
        &[SocketType::AnyEvents, SocketType::Rectangle]
    }

    fn predicted_outputs(&self) -> &[SocketType] {
        &[SocketType::AnyEvents]
    }

    fn run(&self, inputs: &[&SocketValue]) -> Result<Vec<SocketValue>, Error> {
        super::retrieve!(inputs[1], SocketValue::Rectangle(rectangle));

        let socket_value = inputs[0].map_events(|event| {
            let mut new_event = event.clone();
            new_event.global_tags.rectangle_clip = Some(nde::tags::Clip::Contained(*rectangle));
            new_event
        })?;
        Ok(vec![socket_value])
    }
}
