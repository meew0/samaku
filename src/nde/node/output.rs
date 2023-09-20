use crate::nde;

use super::{Node, SocketType, SocketValue};

#[derive(Debug)]
pub struct Output {}

impl Node for Output {
    fn name(&self) -> &'static str {
        "Output"
    }

    fn desired_inputs(&self) -> &[SocketType] {
        &[SocketType::GenericEvents]
    }

    fn predicted_outputs(&self) -> &[SocketType] {
        &[]
    }

    fn run(&self, inputs: &[&SocketValue]) -> Vec<SocketValue> {
        let compiled = inputs[0].map_events_into(nde::Event::to_ass_event);
        vec![SocketValue::CompiledEvents(compiled)]
    }
}