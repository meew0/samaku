use crate::{model, nde, subtitle};

use super::{Error, Node, Shell, SocketType, SocketValue};

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct SplitFrameByFrame {}

#[typetag::serde]
impl Node for SplitFrameByFrame {
    fn name(&self) -> &'static str {
        "Split frame by frame"
    }

    fn desired_inputs(&self) -> &[SocketType] {
        &[SocketType::IndividualEvent, SocketType::FrameRate]
    }

    fn predicted_outputs(&self) -> &[SocketType] {
        &[SocketType::MultipleEvents]
    }

    fn run(&self, inputs: &[&SocketValue]) -> Result<Vec<SocketValue>, Error> {
        super::retrieve!(inputs[0], SocketValue::IndividualEvent(event));
        super::retrieve!(inputs[1], SocketValue::FrameRate(frame_rate));

        let mut res: Vec<nde::Event> = vec![];
        let mut frame = frame_rate.ms_to_frame(event.start.0);
        let end = event.start.0 + event.duration.0;
        let static_duration = subtitle::Duration(frame_rate.frame_time_ms());

        loop {
            let static_start = frame_rate.frame_to_ms(frame);

            if static_start < event.start.0 {
                continue;
            }
            if static_start > end {
                break;
            }

            res.push(event.make_static(subtitle::StartTime(static_start), static_duration));

            frame += model::FrameDelta(1);
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
