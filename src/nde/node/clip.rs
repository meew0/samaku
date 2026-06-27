use crate::nde;

use super::{Context, Node, Shell, SocketType, SocketValue, SocketValues};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ClipRectangle;

#[typetag::serde]
impl Node for ClipRectangle {
    fn name(&self) -> &'static str {
        "Rectangular clip"
    }

    fn desired_inputs(&self) -> &[SocketType] {
        &[SocketType::Event, SocketType::Rectangle]
    }

    fn predicted_outputs(&self) -> &[SocketType] {
        &[SocketType::Event]
    }

    fn run<'a>(
        &'_ self,
        mut inputs: SocketValues<'a>,
        _context: &'a Context,
    ) -> anyhow::Result<SocketValues<'a>> {
        assert!(
            inputs.len() > 1,
            "the required number of inputs should be present"
        ); // Elide bounds checks
        super::retrieve!(&mut inputs[0], SocketValue::Event(event));
        super::retrieve!(&mut inputs[1], SocketValue::Rectangle(rectangle));

        let new_event = event.generic_zip(rectangle, |mut event_val, rectangle_opt| {
            if let Some(rectangle_cow) = rectangle_opt {
                event_val.global_tags.rectangle_clip =
                    Some(nde::tags::Clip::Contained(rectangle_cow.into_owned()));
            }
            event_val
        });

        Ok(SocketValue::Event(new_event).into_values())
    }
}

inventory::submit! {
    Shell::new(
        &["Clip", "Rectangular"],
        || Box::new(ClipRectangle {})
    )
}
