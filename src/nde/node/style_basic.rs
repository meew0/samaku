use crate::nde;

use super::{Context, Node, Shell, SocketType, SocketValue};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Italic;

#[typetag::serde]
impl Node for Italic {
    fn name(&self) -> &'static str {
        "Italicise"
    }

    fn desired_inputs(&self) -> &[SocketType] {
        &[SocketType::Event]
    }

    fn predicted_outputs(&self) -> &[SocketType] {
        &[SocketType::Event]
    }

    fn run<'a>(
        &'_ self,
        mut inputs: super::SocketValues<'a>,
        _context: &'a Context,
    ) -> anyhow::Result<super::SocketValues<'a>> {
        super::retrieve!(&mut inputs[0], SocketValue::Event(event));

        let new_event = event.map_same(|mut event_val| {
            event_val.overrides.italic = nde::tags::Resettable::Override(true);
            event_val
        });

        Ok(SocketValue::Event(new_event).into_values())
    }
}

inventory::submit! {
    Shell::new(
        &["Style", "Italicise"],
        || Box::new(Italic {})
    )
}
