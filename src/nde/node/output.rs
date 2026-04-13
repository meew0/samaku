use crate::nde;

use super::{Node, SocketType, SocketValue};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Output;

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

    fn run(&'_ self, inputs: &[&SocketValue]) -> anyhow::Result<Vec<SocketValue<'_>>> {
        let compiled = inputs[0].map_events_into(nde::Event::to_ass_event)?;
        Ok(vec![SocketValue::CompiledEvents(compiled)])
    }

    fn is_output(&self) -> bool {
        true
    }
}

// Do not inventory::submit this node, as the user should not be able to add it manually.
