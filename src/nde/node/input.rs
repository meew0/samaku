use crate::nde;

use super::{Error, LeafInputType, Node, SocketType, SocketValue};

#[derive(Debug, Clone)]
pub struct Sline {}

impl Node for Sline {
    fn name(&self) -> &'static str {
        "Input: Subtitle line"
    }

    fn desired_inputs(&self) -> &[SocketType] {
        &[SocketType::LeafInput(LeafInputType::Sline)]
    }

    fn predicted_outputs(&self) -> &[SocketType] {
        &[SocketType::IndividualEvent]
    }

    fn run(&self, inputs: &[&SocketValue]) -> Result<Vec<SocketValue>, Error> {
        let SocketValue::Sline(sline) = inputs[0] else {
            return Err(Error::MismatchedTypes);
        };

        let (global, spans) = nde::tags::parse(&sline.text);

        let event = nde::Event {
            start: sline.start,
            duration: sline.duration,
            layer_index: sline.layer_index,
            style_index: sline.style_index,
            margins: sline.margins,
            global_tags: *global,
            overrides: nde::tags::Local::empty(),
            text: spans,
        };
        Ok(vec![SocketValue::IndividualEvent(Box::new(event))])
    }
}
