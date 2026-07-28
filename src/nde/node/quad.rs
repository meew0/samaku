use super::{Context, Node, Shell, SocketType, SocketValue};
use crate::nde::tags::perspective;
use crate::{message, nde, subtitle, view};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MotionQuad;

#[typetag::serde]
impl Node for MotionQuad {
    fn name(&self) -> &'static str {
        "Motion quad"
    }

    fn desired_inputs(&self) -> &[SocketType] {
        &[
            SocketType::Marker,
            SocketType::Marker,
            SocketType::Marker,
            SocketType::Marker,
        ]
    }

    fn predicted_outputs(&self) -> &[SocketType] {
        &[SocketType::Quad]
    }

    fn run<'a>(
        &'_ self,
        mut inputs: super::SocketValues<'a>,
        _context: &'a Context,
    ) -> anyhow::Result<super::SocketValues<'a>> {
        assert!(
            inputs.len() > 3,
            "the required number of inputs should be present"
        ); // Elide bounds checks

        let top_left = super::marker_or_position(&mut inputs[0])?;
        let top_right = super::marker_or_position(&mut inputs[1])?;
        let bottom_right = super::marker_or_position(&mut inputs[2])?;
        let bottom_left = super::marker_or_position(&mut inputs[3])?;

        let step1 = top_left.generic_zip(top_right, |tl, tr| {
            tr.map(|tr_cow| (tl, tr_cow.into_owned()))
        });
        let step2 = step1.generic_zip(bottom_right, |opt, br| {
            opt.zip(br)
                .map(|((tl, tr), br_cow)| (tl, tr, br_cow.into_owned()))
        });
        let quad = step2.generic_zip(bottom_left, |opt, bl| {
            opt.zip(bl).map(|((tl, tr, br), bl_cow)| perspective::Quad {
                q0: tl,
                q1: tr,
                q2: br,
                q3: bl_cow.into_owned(),
            })
        });

        let Some(quad_flat) = quad.flatten() else {
            anyhow::bail!("No quad found")
        };

        Ok(SocketValue::Quad(quad_flat).into_values())
    }
}

inventory::submit! {
    Shell::new(
        &["Motion quad"],
        || Box::new(MotionQuad {})
    )
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UvRescale {
    u1: f64,
    v1: f64,
    u2: f64,
    v2: f64,
}

// Indices for value changed messages
const U1: usize = 0;
const V1: usize = 1;
const U2: usize = 2;
const V2: usize = 3;

#[typetag::serde]
impl Node for UvRescale {
    fn name(&self) -> &'static str {
        "UV rescale"
    }

    fn desired_inputs(&self) -> &[SocketType] {
        &[SocketType::Quad]
    }

    fn predicted_outputs(&self) -> &[SocketType] {
        &[SocketType::Quad]
    }

    fn content<'a>(
        &'a self,
        _global_state: &'a crate::Samaku,
        filter_index: subtitle::ExtradataId,
        self_index: nde::graph::NodeId,
    ) -> iced::Element<'a, message::Message> {
        // TODO add a control to un-limit the bounds
        let bounds = 0.0..=1.0;

        let u1 = view::widget::number_dragger(self.u1, bounds.clone(), move |value| {
            message::Message::Node(
                filter_index,
                self_index,
                message::Node::FloatValueChanged(U1, value),
            )
        })
        .step_and_drag_speed(0.01)
        .width(iced::Length::FillPortion(1));
        let u2 = view::widget::number_dragger(self.u2, bounds.clone(), move |value| {
            message::Message::Node(
                filter_index,
                self_index,
                message::Node::FloatValueChanged(U2, value),
            )
        })
        .step_and_drag_speed(0.01)
        .width(iced::Length::FillPortion(1));
        let u_row = iced::widget::row![u1, u2];

        let v1 = view::widget::number_dragger(self.v1, bounds.clone(), move |value| {
            message::Message::Node(
                filter_index,
                self_index,
                message::Node::FloatValueChanged(V1, value),
            )
        })
        .step_and_drag_speed(0.01)
        .width(iced::Length::FillPortion(1));
        let v2 = view::widget::number_dragger(self.v2, bounds, move |value| {
            message::Message::Node(
                filter_index,
                self_index,
                message::Node::FloatValueChanged(V2, value),
            )
        })
        .step_and_drag_speed(0.01)
        .width(iced::Length::FillPortion(1));
        let v_row = iced::widget::row![v1, v2];

        let column = iced::widget::column!["U start/end", u_row, "V start/end", v_row];

        column
            .spacing(4.0)
            .width(iced::Length::Fill)
            .align_x(iced::Alignment::Center)
            .into()
    }

    fn update(&mut self, message: message::Node) -> anyhow::Result<()> {
        let message::Node::FloatValueChanged(index, value) = message else {
            anyhow::bail!("UvRescale does not handle message {message:?}");
        };

        match index {
            U1 => self.u1 = value,
            V1 => self.v1 = value,
            U2 => self.u2 = value,
            V2 => self.v2 = value,
            _ => anyhow::bail!("Unknown setting index: {index}"),
        }
        Ok(())
    }

    fn run<'a>(
        &'_ self,
        mut inputs: super::SocketValues<'a>,
        _context: &'a Context,
    ) -> anyhow::Result<super::SocketValues<'a>> {
        super::retrieve!(&mut inputs[0], SocketValue::Quad(quad));

        let top_left_uv = glam::DVec2::new(self.u1, self.v1);
        let bottom_right_uv = glam::DVec2::new(self.u2, self.v2);

        let rescaled = quad.map_same(|quad_val| quad_val.inner(top_left_uv, bottom_right_uv));

        Ok(SocketValue::Quad(rescaled).into_values())
    }

    fn content_size(&self) -> iced::Size {
        iced::Size::new(200.0, 150.0)
    }
}

inventory::submit! {
    Shell::new(
        &["UV rescale"],
        || Box::new(UvRescale {
            u1: 0.0,
            v1: 0.0,
            u2: 1.0,
            v2: 1.0,
        })
    )
}
