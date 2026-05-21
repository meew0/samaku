use super::{Context, Node, Shell, SocketType, SocketValue};
use crate::model::reticule;
use crate::nde::tags::perspective;
use crate::{message, model, nde, style, subtitle, view};
use nalgebra::vector;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct InputQuad {
    pub inner: perspective::Quad,
    pub outer: perspective::Quad,
}

impl InputQuad {
    /// Reticule layout: indices 0–3 are inner corners (q0–q3), 4–7 are outer corners (q0–q3).
    fn reticule_update_internal(&self, reticules: &mut [reticule::Reticule]) {
        assert_eq!(reticules.len(), 8, "8 reticules required");

        let inner = [&self.inner.q0, &self.inner.q1, &self.inner.q2, &self.inner.q3];
        let outer = [&self.outer.q0, &self.outer.q1, &self.outer.q2, &self.outer.q3];

        for (i, corner) in inner.iter().enumerate() {
            reticules[i].position = nde::tags::Position {
                x: corner.x,
                y: corner.y,
            };
        }
        for (i, corner) in outer.iter().enumerate() {
            reticules[i + 4].position = nde::tags::Position {
                x: corner.x,
                y: corner.y,
            };
        }
    }
}

#[typetag::serde]
impl Node for InputQuad {
    fn name(&self) -> &'static str {
        "Input: Perspective quad"
    }

    fn desired_inputs(&self) -> &[SocketType] {
        &[]
    }

    fn predicted_outputs(&self) -> &[SocketType] {
        &[SocketType::Quad]
    }

    fn run(
        &'_ self,
        _inputs: &[&SocketValue],
        _context: &Context,
    ) -> anyhow::Result<Vec<SocketValue<'_>>> {
        Ok(vec![SocketValue::Quad(self.inner.clone())])
    }

    fn content<'a>(
        &self,
        _filter_index: subtitle::ExtradataId,
        _self_index: nde::graph::NodeId,
    ) -> iced::Element<'a, message::Message> {
        let column = iced::widget::column![iced::widget::text("(quad)")];

        column
            .spacing(4.0)
            .width(iced::Length::Fill)
            .align_x(iced::Alignment::Center)
            .into()
    }

    fn reticule_activate(&mut self) -> Vec<reticule::Reticule> {
        let make = |radius| reticule::Reticule {
            shape: reticule::Shape::Circle,
            position: nde::tags::Position::default(),
            radius,
        };

        let mut list = vec![
            make(10.0),
            make(10.0),
            make(10.0),
            make(10.0),
            make(12.0),
            make(12.0),
            make(12.0),
            make(12.0),
        ];
        self.reticule_update_internal(&mut list);
        list
    }

    fn reticule_update(
        &mut self,
        reticules: &mut reticule::Reticules,
        index: reticule::Index,
        new_position: nde::tags::Position,
    ) -> anyhow::Result<nde::tags::Position> {
        let old_position = reticules[index].position;
        let new_pos_vec = vector![new_position.x, new_position.y];

        match index.0 {
            0..=3 => {
                // Moving an inner corner: the inner quad is always a UV-rectangle within the
                // outer quad, so moving corner i adjusts c1/c2 and all four corners are
                // recomputed to keep the rectangle structure intact.
                let mut c1 = self.outer.xy_to_uv(self.inner.q0);
                let mut c2 = self.outer.xy_to_uv(self.inner.q2);
                let new_uv = self.outer.xy_to_uv(new_pos_vec);

                match index.0 {
                    0 => {
                        c1.x = new_uv.x;
                        c1.y = new_uv.y;
                    }
                    1 => {
                        c2.x = new_uv.x;
                        c1.y = new_uv.y;
                    }
                    2 => {
                        c2.x = new_uv.x;
                        c2.y = new_uv.y;
                    }
                    3 => {
                        c1.x = new_uv.x;
                        c2.y = new_uv.y;
                    }
                    _ => unreachable!(),
                }

                let new_inner = self.outer.inner(c1, c2);
                if new_inner.is_convex() {
                    self.inner = new_inner;
                }
            }
            4..=7 => {
                // Moving an outer corner: preserve the inner quad's UV position (c1/c2) so
                // the inner quad follows the ambient plane as it deforms.
                let c1 = self.outer.xy_to_uv(self.inner.q0);
                let c2 = self.outer.xy_to_uv(self.inner.q2);

                let mut new_outer = self.outer.clone();
                match index.0 - 4 {
                    0 => new_outer.q0 = new_pos_vec,
                    1 => new_outer.q1 = new_pos_vec,
                    2 => new_outer.q2 = new_pos_vec,
                    3 => new_outer.q3 = new_pos_vec,
                    _ => unreachable!(),
                }

                if new_outer.is_convex() {
                    self.inner = new_outer.inner(c1, c2);
                    self.outer = new_outer;
                }
            }
            _ => anyhow::bail!("Reticule index out of range: {}", index.0),
        }

        self.reticule_update_internal(&mut reticules.list);
        Ok(old_position)
    }

    fn draw_reticule_base_layer(
        &self,
        canvas_frame: &mut iced::widget::canvas::Frame,
        bounds: iced::Rectangle,
        storage_size: subtitle::Resolution,
        _current_frame: Option<model::FrameNumber>,
        _cursor: iced::mouse::Cursor,
    ) {
        use iced::widget::canvas;

        let to_iced = |corner: &nalgebra::Vector2<f64>| {
            view::frame_coordinates_to_iced(corner.x, corner.y, bounds.size(), storage_size)
        };

        let inner = [&self.inner.q0, &self.inner.q1, &self.inner.q2, &self.inner.q3];
        let outer = [&self.outer.q0, &self.outer.q1, &self.outer.q2, &self.outer.q3];

        let solid_stroke = canvas::Stroke::default()
            .with_color(style::SAMAKU_PRIMARY)
            .with_width(1.5);

        let dash_segments = [6.0_f32, 6.0];
        let dashed_stroke = canvas::Stroke {
            line_dash: canvas::LineDash {
                segments: &dash_segments,
                offset: 0,
            },
            ..canvas::Stroke::default()
                .with_color(style::SAMAKU_PRIMARY)
                .with_width(1.0)
        };

        let inner_path = canvas::Path::new(|path| {
            path.move_to(to_iced(inner[0]));
            for corner in &inner[1..] {
                path.line_to(to_iced(corner));
            }
            path.close();
        });
        canvas_frame.stroke(&inner_path, solid_stroke);

        let outer_path = canvas::Path::new(|path| {
            path.move_to(to_iced(outer[0]));
            for corner in &outer[1..] {
                path.line_to(to_iced(corner));
            }
            path.close();
        });
        canvas_frame.stroke(&outer_path, dashed_stroke);
    }

    fn content_size(&self) -> iced::Size {
        iced::Size::new(200.0, 125.0)
    }
}

inventory::submit! {
    Shell::new(
        &["Input", "Perspective quad"],
        || Box::new(InputQuad {
            inner: perspective::Quad::make_rect(nalgebra::vector![200.0, 200.0], nalgebra::vector![300.0, 300.0]),
            outer: perspective::Quad::make_rect(nalgebra::vector![100.0, 100.0], nalgebra::vector![400.0, 400.0])
        })
    )
}
