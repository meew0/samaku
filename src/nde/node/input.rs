use crate::nde;

use super::{LeafInputType, Node, SocketType, SocketValue};

#[derive(Debug, Clone)]
pub struct InputSline {}

impl Node for InputSline {
    fn name(&self) -> &'static str {
        "Input: Subtitle line"
    }

    fn desired_inputs(&self) -> &[SocketType] {
        &[SocketType::LeafInput(LeafInputType::Sline)]
    }

    fn predicted_outputs(&self) -> &[SocketType] {
        &[SocketType::IndividualEvent]
    }

    fn run(&self, inputs: &[&SocketValue]) -> Vec<SocketValue> {
        let sline = match inputs[0] {
            SocketValue::Sline(sline) => sline,
            _ => panic!("expected sline"),
        };
        let event = nde::Event {
            start: sline.start,
            duration: sline.duration,
            layer_index: sline.layer_index,
            style_index: sline.style_index,
            margins: sline.margins,
            global_tags: nde::tags::Global::empty(),
            overrides: nde::tags::Local::empty(),

            // TODO in the far future: parse ASS tags into span?
            text: vec![nde::Span::Tags(
                nde::tags::Local::empty(),
                sline.text.clone(),
            )],
        };
        vec![SocketValue::IndividualEvent(Box::new(event))]
    }
}