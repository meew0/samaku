use super::{Context, Node, Shell, SocketType, SocketValue};
use crate::nde::{
    self,
    tags::{Resettable, perspective},
};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Perspective;

#[typetag::serde]
impl Node for Perspective {
    fn name(&self) -> &'static str {
        "Perspective"
    }

    fn desired_inputs(&self) -> &[SocketType] {
        &[SocketType::Event, SocketType::Quad]
    }

    fn predicted_outputs(&self) -> &[SocketType] {
        &[SocketType::Event]
    }

    fn run<'a>(
        &'_ self,
        mut inputs: super::SocketValues<'a>,
        context: &'a Context,
    ) -> anyhow::Result<super::SocketValues<'a>> {
        assert!(
            inputs.len() > 1,
            "the required number of inputs should be present"
        ); // Elide bounds checks

        super::retrieve!(&mut inputs[0], SocketValue::Event(event));
        super::retrieve!(&mut inputs[1], SocketValue::Quad(quad));

        let new_event = event.frame_zip(quad, |_, mut event_val, quad_opt| {
            if let Some(quad_val) = quad_opt {
                // Clear rotation and shear
                event_val.overrides.text_rotation.x = Resettable::Override(0.0);
                event_val.overrides.text_rotation.y = Resettable::Override(0.0);
                event_val.overrides.text_rotation.z = Resettable::Override(0.0);
                event_val.overrides.text_shear.x = Resettable::Override(0.0);
                event_val.overrides.text_shear.y = Resettable::Override(0.0);

                let style = context.get_event_style(&event_val);
                let bounding_box = nde::util::measure(&event_val, style);
                let alignment = *event_val
                    .global_tags
                    .alignment
                    .override_or(&style.alignment);
                let screen_z = perspective::rescale_screen_z(
                    context.playback_resolution,
                    context.layout_resolution,
                );

                let perspective = perspective::quad_to_tags(
                    &quad_val,
                    perspective::OrgMode::Center,
                    alignment,
                    bounding_box,
                    screen_z,
                );

                let (font_scale, border, shadow) = (
                    event_val.effective_font_scale(style),
                    event_val.effective_border(style),
                    event_val.effective_shadow(style),
                );
                if let Some(new_local) =
                    perspective.apply(&mut event_val.global_tags, font_scale, border, shadow)
                {
                    event_val.overrides.override_from(&new_local, false);
                }
            }

            event_val
        });

        Ok(SocketValue::Event(new_event).into_values())
    }
}

inventory::submit! {
    Shell::new(
        &["Perspective"],
        || Box::new(Perspective {})
    )
}
