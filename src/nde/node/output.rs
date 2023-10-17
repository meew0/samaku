use crate::nde;

use super::{Error, Node, SocketType, SocketValue};

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct Output {}

#[typetag::serde]
impl Node for Output {
    fn name(&self) -> &'static str {
        "Output"
    }

    fn desired_inputs(&self) -> &[SocketType] {
        &[SocketType::AnyEvents]
    }

    fn predicted_outputs(&self) -> &[SocketType] {
        &[]
    }

    fn run(&self, inputs: &[&SocketValue]) -> Result<Vec<SocketValue>, Error> {
        let compiled = inputs[0].map_events_into(nde::Event::to_ass_event)?;
        Ok(vec![SocketValue::CompiledEvents(compiled)])
    }
}

// Do not inventory::submit this node, as the user should not be able to add it manually.
