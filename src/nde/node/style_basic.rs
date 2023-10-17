use crate::nde;

use super::{Error, Node, Shell, SocketType, SocketValue};

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct Italic {}

#[typetag::serde]
impl Node for Italic {
    fn name(&self) -> &'static str {
        "Italicise"
    }

    fn desired_inputs(&self) -> &[SocketType] {
        &[SocketType::AnyEvents]
    }

    fn predicted_outputs(&self) -> &[SocketType] {
        &[SocketType::AnyEvents]
    }

    fn run(&self, inputs: &[&SocketValue]) -> Result<Vec<SocketValue>, Error> {
        let socket_value = inputs[0].map_events(|event| {
            let mut new_event = event.clone();
            new_event.overrides.italic = nde::tags::Resettable::Override(true);
            new_event
        })?;
        Ok(vec![socket_value])
    }
}

inventory::submit! {
    Shell::new(
        &["Style", "Italicise"],
        || Box::new(Italic {})
    )
}
