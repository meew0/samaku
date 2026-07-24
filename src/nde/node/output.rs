use super::{Context, Node, SocketType, SocketValue};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Output;

#[typetag::serde]
impl Node for Output {
    fn name(&self) -> &'static str {
        "Output"
    }

    fn category(&self) -> super::Category {
        super::Category::Output
    }

    fn desired_inputs(&self) -> &[SocketType] {
        &[SocketType::Event]
    }

    fn predicted_outputs(&self) -> &[SocketType] {
        &[]
    }

    fn run<'a>(
        &'_ self,
        mut inputs: super::SocketValues<'a>,
        context: &'a Context,
    ) -> anyhow::Result<super::SocketValues<'a>> {
        super::retrieve!(&mut inputs[0], SocketValue::Event(event));

        let compiled = event.into_vec(|nde_event| nde_event.to_ass_event(context.frame_rate));

        Ok(SocketValue::CompiledEvents(compiled).into_values())
    }
}

// Do not inventory::submit this node, as the user should not be able to add it manually.
