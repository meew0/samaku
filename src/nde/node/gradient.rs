use crate::nde;

use super::{Error, Node, Shell, SocketType, SocketValue};

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct Gradient;

#[typetag::serde]
impl Node for Gradient {
    fn name(&self) -> &'static str {
        "Gradient"
    }

    fn desired_inputs(&self) -> &[SocketType] {
        &[
            SocketType::AnyEvents,
            SocketType::Rectangle,
            SocketType::LocalTags,
        ]
    }

    fn predicted_outputs(&self) -> &[SocketType] {
        &[SocketType::AnyEvents]
    }

    fn run(&self, inputs: &[&SocketValue]) -> Result<Vec<SocketValue>, Error> {
        const RESOLUTION: i32 = 5;

        assert!(inputs.len() > 2);

        super::retrieve!(inputs[1], SocketValue::Rectangle(rectangle));
        super::retrieve!(inputs[2], SocketValue::LocalTags(target_tags));

        if rectangle.x2 < rectangle.x1 {
            return Err(Error::InvertedRectangle);
        }

        let mut events = vec![];

        inputs[0].each_event(|event| {
            let mut x = rectangle.x1;
            let width = rectangle.x2 - rectangle.x1;

            while x < rectangle.x2 {
                let mut new_event = event.clone();
                new_event.global_tags.rectangle_clip =
                    Some(nde::tags::Clip::Contained(nde::tags::Rectangle {
                        x1: x,
                        x2: x + RESOLUTION,
                        ..*rectangle
                    }));

                let power = f64::from(x - rectangle.x1) / f64::from(width);
                for span in &mut new_event.text {
                    match span {
                        nde::Span::Tags(local, _) | nde::Span::Drawing(local, _) => {
                            local.interpolate(target_tags, power);
                        }
                        _ => {}
                    }
                }
                events.push(new_event);

                x += RESOLUTION;
            }
        })?;
        Ok(vec![SocketValue::MultipleEvents(events)])
    }
}

inventory::submit! {
    Shell::new(
        &["Gradient"],
        || Box::new(Gradient {})
    )
}
