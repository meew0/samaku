use crate::nde;

use super::{Context, Node, Shell, SocketType, SocketValue};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Gradient;

#[typetag::serde]
impl Node for Gradient {
    fn name(&self) -> &'static str {
        "Gradient"
    }

    fn desired_inputs(&self) -> &[SocketType] {
        &[
            SocketType::Event,
            SocketType::Rectangle,
            SocketType::LocalTags,
        ]
    }

    fn predicted_outputs(&self) -> &[SocketType] {
        &[SocketType::Event]
    }

    fn run<'a>(
        &'_ self,
        mut inputs: super::SocketValues<'a>,
        _context: &'a Context,
    ) -> anyhow::Result<super::SocketValues<'a>> {
        const RESOLUTION: i32 = 5;

        assert!(
            inputs.len() > 2,
            "the required number of inputs should be present"
        ); // Elide bounds checks

        super::retrieve!(&mut inputs[0], SocketValue::Event(event));
        super::retrieve!(&mut inputs[1], SocketValue::Rectangle(rectangle));
        super::retrieve!(&mut inputs[2], SocketValue::LocalTags(target_tags));

        let gradient = rectangle.generic_zip(target_tags, |rectangle_val, target_tags_opt| {
            target_tags_opt.map(|target_tags_cow| (rectangle_val, target_tags_cow.into_owned()))
        });

        // Combine the event with the gradient parameters into a single track, then
        // `expand` each event into the strips covering its frame range. For a
        // `Single` event this turns into a `Stack`, which the output node flattens
        // back into the individual strip events.
        let combined = event.generic_zip(gradient, |event_val, gradient_opt| {
            (
                event_val,
                gradient_opt.and_then(std::borrow::Cow::into_owned),
            )
        });

        let new_event = combined.expand(|(event_val, gradient_opt)| {
            // TODO inner error handling
            // if rectangle.x2 < rectangle.x1 || rectangle.y2 < rectangle.y1 {
            //     anyhow::bail!("Input rectangle is inverted");
            // }

            let Some((rectangle_val, target_tags_val)) = gradient_opt else {
                // Without gradient parameters, pass the event through unchanged.
                return vec![event_val];
            };

            let mut x = rectangle_val.x1;
            let width = rectangle_val.x2 - rectangle_val.x1;

            let mut events = vec![];

            while x < rectangle_val.x2 {
                let mut new_event = event_val.clone();
                new_event.global_tags.rectangle_clip =
                    Some(nde::tags::Clip::Contained(nde::tags::Rectangle {
                        x1: x,
                        x2: x + RESOLUTION,
                        ..rectangle_val
                    }));

                let power = f64::from(x - rectangle_val.x1) / f64::from(width);
                for span in &mut new_event.text {
                    if let &mut (nde::Span::Tags(ref mut local, _)
                    | nde::Span::Drawing(ref mut local, _)) = span
                    {
                        local.interpolate(&target_tags_val, power);
                    }
                }
                events.push(new_event);

                x += RESOLUTION;
            }

            // `expand` requires a non-empty result. A zero- or negative-width
            // rectangle produces no strips, so fall back to the unmodified event.
            if events.is_empty() {
                events.push(event_val);
            }

            events
        });

        Ok(SocketValue::Event(new_event).into_values())
    }
}

inventory::submit! {
    Shell::new(
        &["Gradient"],
        || Box::new(Gradient {})
    )
}
