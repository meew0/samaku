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
        &[SocketType::AnyEvents, SocketType::Quad]
    }

    fn predicted_outputs(&self) -> &[SocketType] {
        &[SocketType::AnyEvents]
    }

    fn run(
        &'_ self,
        inputs: &[&SocketValue],
        context: &Context,
    ) -> anyhow::Result<Vec<SocketValue<'_>>> {
        assert!(
            inputs.len() > 1,
            "the required number of inputs should be present"
        ); // Elide bounds checks

        super::retrieve!(inputs[1], &SocketValue::Quad(ref quad));

        let socket_value = inputs[0].map_events(|event| {
            let mut new_event = event.clone();

            // Clear rotation and shear
            new_event.overrides.text_rotation.x = Resettable::Override(0.0);
            new_event.overrides.text_rotation.y = Resettable::Override(0.0);
            new_event.overrides.text_rotation.z = Resettable::Override(0.0);
            new_event.overrides.text_shear.x = Resettable::Override(0.0);
            new_event.overrides.text_shear.y = Resettable::Override(0.0);

            let style = context.get_event_style(&new_event);
            let bounding_box = nde::util::measure(&new_event, style);
            let alignment = *event.global_tags.alignment.override_or(&style.alignment);
            let screen_z = perspective::rescale_screen_z(
                context.playback_resolution,
                context.layout_resolution,
            );

            let perspective = perspective::quad_to_tags(
                quad,
                perspective::OrgMode::Center,
                alignment,
                bounding_box,
                screen_z,
            );

            let (font_scale, border, shadow) = (
                new_event.effective_font_scale(style),
                new_event.effective_border(style),
                new_event.effective_shadow(style),
            );
            if let Some(new_local) =
                perspective.apply(&mut new_event.global_tags, font_scale, border, shadow)
            {
                new_event.overrides.override_from(&new_local, false);
            }

            new_event
        })?;
        Ok(vec![socket_value])
    }
}

inventory::submit! {
    Shell::new(
        &["Perspective"],
        || Box::new(Perspective {})
    )
}
