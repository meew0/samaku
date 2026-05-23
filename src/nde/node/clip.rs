use crate::nde;

use super::{Context, Node, Shell, SocketType, SocketValue};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ClipRectangle;

#[typetag::serde]
impl Node for ClipRectangle {
    fn name(&self) -> &'static str {
        "Rectangular clip"
    }

    fn desired_inputs(&self) -> &[SocketType] {
        &[SocketType::AnyEvents, SocketType::Rectangle]
    }

    fn predicted_outputs(&self) -> &[SocketType] {
        &[SocketType::AnyEvents]
    }

    fn run(
        &'_ self,
        inputs: &[&SocketValue],
        _context: &Context,
    ) -> anyhow::Result<Vec<SocketValue<'_>>> {
        assert!(
            inputs.len() > 1,
            "the required number of inputs should be present"
        ); // Elide bounds checks

        super::retrieve!(inputs[1], &SocketValue::Rectangle(ref rectangle));

        let socket_value = inputs[0].map_events(|event| {
            let mut new_event = event.clone();
            new_event.global_tags.rectangle_clip = Some(nde::tags::Clip::Contained(*rectangle));
            new_event
        })?;
        Ok(vec![socket_value])
    }
}

inventory::submit! {
    Shell::new(
        &["Clip", "Rectangular"],
        || Box::new(ClipRectangle {})
    )
}
