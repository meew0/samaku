use crate::{media, nde};

use super::{Context, Node, Shell, SocketType, SocketValue};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SplitFrameByFrame;

#[typetag::serde]
impl Node for SplitFrameByFrame {
    fn name(&self) -> &'static str {
        "Split frame by frame"
    }

    fn desired_inputs(&self) -> &[SocketType] {
        &[SocketType::IndividualEvent]
    }

    fn predicted_outputs(&self) -> &[SocketType] {
        &[SocketType::MultipleEvents]
    }

    fn run(
        &'_ self,
        inputs: &[&SocketValue],
        _context: &Context,
    ) -> anyhow::Result<Vec<SocketValue<'_>>> {
        const ONE: media::FrameDelta = media::FrameDelta(1);

        super::retrieve!(inputs[0], &SocketValue::IndividualEvent(ref event));

        let mut res: Vec<nde::Event> = vec![];
        let mut frame = event.start;
        let end = event.start + event.duration;

        while frame < end {
            res.push(event.make_static(frame, ONE));
            frame += ONE;
        }

        Ok(vec![SocketValue::MultipleEvents(res)])
    }
}

inventory::submit! {
    Shell::new(
        &["Split", "Frame by frame"],
        || Box::new(SplitFrameByFrame {})
    )
}
