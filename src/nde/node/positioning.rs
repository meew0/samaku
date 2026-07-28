use crate::nde;

use super::{Context, Node, Shell, SocketType, SocketValue};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SetPosition;

#[typetag::serde]
impl Node for SetPosition {
    fn name(&self) -> &'static str {
        "Set position"
    }

    fn desired_inputs(&self) -> &[SocketType] {
        &[SocketType::Event, SocketType::Position]
    }

    fn predicted_outputs(&self) -> &[SocketType] {
        &[SocketType::Event]
    }

    fn run<'a>(
        &'_ self,
        mut inputs: super::SocketValues<'a>,
        _context: &'a Context,
    ) -> anyhow::Result<super::SocketValues<'a>> {
        assert!(
            inputs.len() > 1,
            "the required number of inputs should be present"
        ); // Elide bounds checks

        super::retrieve!(&mut inputs[0], SocketValue::Event(event));

        // If we are given a motion track marker, get the position of its region center
        let position = super::marker_or_position(&mut inputs[1])?;

        let new_event = event.generic_zip_same(position, |mut event_val, position_opt| {
            if let Some(position_val) = position_opt {
                event_val.global_tags.position =
                    Some(nde::tags::PositionOrMove::Position(*position_val));
            }
            event_val
        });

        Ok(SocketValue::Event(new_event).into_values())
    }
}

inventory::submit! {
    Shell::new(
        &["Set position"],
        || Box::new(SetPosition {})
    )
}
