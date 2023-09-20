use super::{Node, SocketType, SocketValue};

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

    fn run(&self, inputs: &[&SocketValue]) -> Vec<SocketValue> {
        vec![inputs[0].map_events(|event| {
            let mut new_event = event.clone();
            new_event.overrides.italic = Some(true);
            new_event
        })]
    }
}
