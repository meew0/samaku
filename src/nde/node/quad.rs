use super::{Context, Node, Shell, SocketType, SocketValue};
use crate::nde::tags::perspective;

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
