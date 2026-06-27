use crate::nde;

use super::{Context, Node, Shell, SocketType, SocketValue};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SplitFrameByFrame;

#[typetag::serde]
impl Node for SplitFrameByFrame {
    fn name(&self) -> &'static str {
        "Split frame by frame"
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

        let new_event = event
            .frame_zip(nde::FramedTrack::from_single(()), |_frame, event_val, _| {
                event_val
            });

        Ok(SocketValue::Event(new_event).into_values())
    }
}

inventory::submit! {
    Shell::new(
        &["Split", "Frame by frame"],
        || Box::new(SplitFrameByFrame {})
    )
}
