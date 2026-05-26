use super::{Context, Node, Shell, SocketType, SocketValue};
use crate::model::reticule;
use crate::nde::tags::perspective;
use crate::{message, model, nde, style, subtitle, view};
use glam::DVec2;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct InputQuad {
    pub inner: perspective::Quad,
    pub outer: perspective::Quad,
    /// Whether the outer (ambient plane) quad is shown and its corners are draggable.
    #[serde(default = "bool_true")]
    pub show_outer: bool,
    /// When `show_outer` is true: locks the inner quad so only outer corners can be dragged.
    #[serde(default)]
    pub lock_inner: bool,
    /// Whether to draw a perspective grid overlaid on the inner quad, fading outward.
    #[serde(default)]
    pub show_grid: bool,
}

fn bool_true() -> bool {
    true
}

/// Returns a mutable reference to quad corner `index` (0 = q0 … 3 = q3).
fn quad_corner_mut(quad: &mut perspective::Quad, index: usize) -> &mut DVec2 {
    match index {
        0 => &mut quad.q0,
        1 => &mut quad.q1,
        2 => &mut quad.q2,
        3 => &mut quad.q3,
        _ => panic!("corner index out of range: {index}"),
    }
}

fn uv_update_corner(c1: &mut DVec2, c2: &mut DVec2, new_uv: DVec2, index: usize) {
    match index {
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
        _ => panic!("index {index} out of range for uv_update_corner"),
    }
}

impl InputQuad {
    /// Writes current corner positions into the reticule list.
    ///
    /// Reticule layout depends on the active mode:
    /// - `show_outer = false`: indices 0–3 → inner corners
    /// - `show_outer = true, lock_inner = false`: indices 0–3 → inner, 4–7 → outer
    /// - `show_outer = true, lock_inner = true`: indices 0–3 → outer corners
    fn reticule_update_internal(&self, reticules: &mut [reticule::Reticule]) {
        let inner = [
            &self.inner.q0,
            &self.inner.q1,
            &self.inner.q2,
            &self.inner.q3,
        ];
        let outer = [
            &self.outer.q0,
            &self.outer.q1,
            &self.outer.q2,
            &self.outer.q3,
        ];

        if !self.show_outer {
            assert_eq!(
                reticules.len(),
                4,
                "expected 4 reticules in inner-only mode"
            );
            for (i, &corner) in inner.iter().enumerate() {
                reticules[i].position = (*corner).into();
            }
        } else if !self.lock_inner {
            assert_eq!(
                reticules.len(),
                8,
                "expected 8 reticules in both-quads mode"
            );
            for (i, &corner) in inner.iter().enumerate() {
                reticules[i].position = (*corner).into();
            }
            for (i, &corner) in outer.iter().enumerate() {
                reticules[i + 4].position = (*corner).into();
            }
        } else {
            assert_eq!(
                reticules.len(),
                4,
                "expected 4 reticules in outer-only mode"
            );
            for (i, &corner) in outer.iter().enumerate() {
                reticules[i].position = (*corner).into();
            }
        }
    }
}

#[typetag::serde]
impl Node for InputQuad {
    fn name(&self) -> &'static str {
        "Input: Perspective quad"
    }

    fn category(&self) -> super::Category {
        super::Category::Input
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
        filter_index: subtitle::ExtradataId,
        self_index: nde::graph::NodeId,
    ) -> iced::Element<'a, message::Message> {
        let show_outer_cb = iced::widget::checkbox(self.show_outer)
            .label("Outer plane")
            .on_toggle(move |_| {
                message::Message::Node(filter_index, self_index, message::Node::ToggleSetting(0))
            });

        let lock_inner_cb = iced::widget::checkbox(self.lock_inner)
            .label("Lock inner")
            .on_toggle_maybe(self.show_outer.then_some(move |_: bool| {
                message::Message::Node(filter_index, self_index, message::Node::ToggleSetting(1))
            }));

        let show_grid_cb = iced::widget::checkbox(self.show_grid)
            .label("Grid")
            .on_toggle(move |_| {
                message::Message::Node(filter_index, self_index, message::Node::ToggleSetting(2))
            });

        iced::widget::column![show_outer_cb, lock_inner_cb, show_grid_cb]
            .spacing(4.0)
            .width(iced::Length::Fill)
            .into()
    }

    fn update(&mut self, message: message::Node) -> anyhow::Result<()> {
        let message::Node::ToggleSetting(index) = message else {
            anyhow::bail!("InputQuad does not handle message {message:?}");
        };
        match index {
            0 => {
                self.show_outer = !self.show_outer;
                if self.show_outer {
                    // Rebuild outer from the current inner using Aegisub's default UV coords.
                    self.outer = self
                        .inner
                        .outer(DVec2::new(0.25, 0.25), DVec2::new(0.75, 0.75));
                }
            }
            1 => self.lock_inner = !self.lock_inner,
            2 => self.show_grid = !self.show_grid,
            _ => anyhow::bail!("Unknown setting index: {index}"),
        }
        Ok(())
    }

    fn reticule_activate(&mut self) -> Vec<reticule::Reticule> {
        let circle = |radius| reticule::Reticule {
            shape: reticule::Shape::Circle,
            position: nde::tags::Position::default(),
            radius,
        };

        let mut list = if !self.show_outer {
            vec![circle(10.0), circle(10.0), circle(10.0), circle(10.0)]
        } else if !self.lock_inner {
            vec![
                circle(10.0),
                circle(10.0),
                circle(10.0),
                circle(10.0),
                circle(12.0),
                circle(12.0),
                circle(12.0),
                circle(12.0),
            ]
        } else {
            vec![circle(12.0), circle(12.0), circle(12.0), circle(12.0)]
        };

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
        let new_pos_vec = DVec2::new(new_position.x, new_position.y);

        if !self.show_outer {
            // Inner-only mode: drag inner corners freely (no UV-rect constraint).
            if index.0 >= 4 {
                anyhow::bail!("Reticule index out of range: {}", index.0);
            }
            let mut new_inner = self.inner.clone();
            *quad_corner_mut(&mut new_inner, index.0) = new_pos_vec;
            if new_inner.is_convex() {
                self.inner = new_inner;
            }
        } else if !self.lock_inner {
            // Both-quads mode.
            match index.0 {
                0..=3 => {
                    // Inner corner drag: maintain UV-rect structure so moving corner i adjusts
                    // only the c1/c2 axes it controls, then all four inner corners are recomputed.
                    let mut c1 = self.outer.xy_to_uv(self.inner.q0);
                    let mut c2 = self.outer.xy_to_uv(self.inner.q2);
                    let new_uv = self.outer.xy_to_uv(new_pos_vec);
                    uv_update_corner(&mut c1, &mut c2, new_uv, index.0);
                    let new_inner = self.outer.inner(c1, c2);
                    if new_inner.is_convex() {
                        self.inner = new_inner;
                    }
                }
                4..=7 => {
                    // Outer corner drag: extract c1/c2, move outer corner, recompute inner so
                    // the inner quad stays at the same UV position within the new outer shape.
                    let c1 = self.outer.xy_to_uv(self.inner.q0);
                    let c2 = self.outer.xy_to_uv(self.inner.q2);
                    let mut new_outer = self.outer.clone();
                    *quad_corner_mut(&mut new_outer, index.0 - 4) = new_pos_vec;
                    if new_outer.is_convex() {
                        self.inner = new_outer.inner(c1, c2);
                        self.outer = new_outer;
                    }
                }
                _ => anyhow::bail!("Reticule index out of range: {}", index.0),
            }
        } else {
            // Outer-only mode: "resize" the outer plane while keeping the inner quad fixed.
            // Mirrors Aegisub's OuterLocked() drag: dragging corner i only adjusts the UV
            // axis that corner controls (d1 or d2), then ALL outer corners are recomputed from
            // the updated inverse UV coords. This gives a rectangle-like resize in projected space.
            if index.0 >= 4 {
                anyhow::bail!("Reticule index out of range: {}", index.0);
            }

            // c1/c2: UV of inner.q0 and inner.q2 within the outer quad.
            let c1 = self.outer.xy_to_uv(self.inner.q0);
            let c2 = self.outer.xy_to_uv(self.inner.q2);
            // d1/d2: inverse — UV of outer.q0 and outer.q2 within the inner quad.
            let denom = c2 - c1;
            let mut d1 = -c1 / denom;
            let mut d2 = (DVec2::new(1.0_f64, 1.0_f64) - c1) / denom;

            // Move only the axes controlled by the dragged corner.
            let new_uv = self.inner.xy_to_uv(new_pos_vec);
            uv_update_corner(&mut d1, &mut d2, new_uv, index.0);

            // Back-compute c1/c2 and recompute all four outer corners.
            let d_denom = d2 - d1;
            let new_c1 = -d1 / d_denom;
            let new_c2 = (DVec2::new(1.0_f64, 1.0_f64) - d1) / d_denom;
            let new_outer = self.inner.outer(new_c1, new_c2);
            if new_outer.is_convex() {
                self.outer = new_outer;
            }
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

        let to_iced = |corner: &DVec2| {
            view::frame_coordinates_to_iced(corner.x, corner.y, bounds.size(), storage_size)
        };

        let inner = [
            &self.inner.q0,
            &self.inner.q1,
            &self.inner.q2,
            &self.inner.q3,
        ];
        let outer = [
            &self.outer.q0,
            &self.outer.q1,
            &self.outer.q2,
            &self.outer.q3,
        ];

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

        let quad_path = |corners: &[&DVec2; 4]| {
            canvas::Path::new(|path| {
                path.move_to(to_iced(corners[0]));
                for corner in &corners[1..] {
                    path.line_to(to_iced(corner));
                }
                path.close();
            })
        };

        if self.show_grid {
            // How far beyond the inner quad's UV [0, 1] the grid extends on each side.
            // Increase for a wider grid, decrease for a tighter one (e.g. 0.4 = just outside).
            const GRID_MARGIN: f64 = 1.0;
            const GRID_MIN: f64 = -GRID_MARGIN;
            const GRID_MAX: f64 = 1.0 + GRID_MARGIN;
            const GRID_STEP: f64 = 0.1;
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "GRID_MARGIN is a small positive constant; N_LINES fits in u8"
            )]
            const N_LINES: u8 = ((GRID_MAX - GRID_MIN) / GRID_STEP + 1.0) as u8;
            const N_SAMPLES: u8 = 12;
            let samp_step = (GRID_MAX - GRID_MIN) / f64::from(N_SAMPLES);

            for line_idx in 0..N_LINES {
                let uv_t = f64::from(line_idx).mul_add(GRID_STEP, GRID_MIN);
                #[expect(
                    clippy::cast_possible_truncation,
                    reason = "alpha is in [0, 0.45], which fits in f32"
                )]
                let alpha =
                    ((1.0 - (uv_t - 0.5).abs() / (GRID_MARGIN + 0.5)).max(0.0) as f32) * 0.45;
                if alpha <= 0.001 {
                    continue;
                }

                let grid_path = canvas::Path::new(|builder| {
                    builder.move_to(to_iced(&self.inner.uv_to_xy(DVec2::new(GRID_MIN, uv_t))));
                    for samp_idx in 1..=N_SAMPLES {
                        let uv_u = f64::from(samp_idx).mul_add(samp_step, GRID_MIN);
                        builder.line_to(to_iced(&self.inner.uv_to_xy(DVec2::new(uv_u, uv_t))));
                    }
                    builder.move_to(to_iced(&self.inner.uv_to_xy(DVec2::new(uv_t, GRID_MIN))));
                    for samp_idx in 1..=N_SAMPLES {
                        let uv_v = f64::from(samp_idx).mul_add(samp_step, GRID_MIN);
                        builder.line_to(to_iced(&self.inner.uv_to_xy(DVec2::new(uv_t, uv_v))));
                    }
                });
                canvas_frame.stroke(
                    &grid_path,
                    canvas::Stroke::default()
                        .with_color(style::SAMAKU_PRIMARY.scale_alpha(alpha))
                        .with_width(0.75),
                );
            }
        }

        if self.show_outer {
            // Inner quad solid, outer quad dashed.
            canvas_frame.stroke(&quad_path(&inner), solid_stroke);
            canvas_frame.stroke(&quad_path(&outer), dashed_stroke);
        } else {
            // Inner quad only, drawn dashed to indicate no outer context.
            canvas_frame.stroke(&quad_path(&inner), dashed_stroke);
        }
    }

    fn content_size(&self) -> iced::Size {
        iced::Size::new(200.0, 150.0)
    }
}

inventory::submit! {
    Shell::new(
        &["Input", "Perspective quad"],
        || Box::new(InputQuad {
            inner: perspective::Quad::make_rect(DVec2::new(200.0, 200.0), DVec2::new(300.0, 300.0)),
            outer: perspective::Quad::make_rect(DVec2::new(100.0, 100.0), DVec2::new(400.0, 400.0)),
            show_outer: true,
            lock_inner: false,
            show_grid: false,
        })
    )
}
