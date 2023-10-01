use crate::nde;

use super::{Node, NodeError, SocketType, SocketValue};

#[derive(Debug)]
pub struct Italic {}

impl Node for Italic {
    fn name(&self) -> &'static str {
        "Italicise"
    }

    fn desired_inputs(&self) -> &[SocketType] {
        &[SocketType::GenericEvents]
    }

    fn predicted_outputs(&self) -> &[SocketType] {
        &[SocketType::GenericEvents]
    }

    fn run(&self, inputs: &[&SocketValue]) -> Result<Vec<SocketValue>, NodeError> {
        let socket_value = inputs[0].map_events(|event| {
            let mut new_event = event.clone();
            new_event.overrides.italic = nde::tags::Resettable::Override(true);
            new_event
        })?;
        Ok(vec![socket_value])
    }
}
