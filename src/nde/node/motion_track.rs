use crate::{media, message, model, nde, style, subtitle, view};
use iced::Rectangle;
use iced::mouse::Cursor;
use iced::widget::canvas;
use model::reticule;
use std::collections::BTreeMap;

use super::{Context, Node, Shell, SocketType, SocketValue};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MotionTrack {
    pub region_center: glam::DVec2,
    pub track: BTreeMap<model::FrameNumber, media::motion::Region>,
}

#[typetag::serde]
impl Node for MotionTrack {
    fn name(&self) -> &'static str {
        "Motion track"
    }

    fn desired_inputs(&self) -> &[SocketType] {
        &[SocketType::MultipleEvents]
    }

    fn predicted_outputs(&self) -> &[SocketType] {
        &[SocketType::MultipleEvents]
    }

    fn run(
        &'_ self,
        inputs: &[&SocketValue],
        context: &Context,
    ) -> anyhow::Result<Vec<SocketValue<'_>>> {
        super::retrieve!(inputs[0], &SocketValue::MultipleEvents(ref events));
        let frame_rate = context.frame_rate;

        let mut new_events: Vec<nde::Event> = vec![];

        for event in events {
            let mut cloned = event.clone();
            let frame = frame_rate.ms_to_frame(cloned.start.0);
            if let Some(region) = self.track.get(&frame) {
                cloned.global_tags.position =
                    Some(nde::tags::PositionOrMove::Position(glam::DVec2 {
                        x: region.center.x,
                        y: region.center.y,
                    }));
            }
            new_events.push(cloned);
        }

        Ok(vec![SocketValue::MultipleEvents(new_events)])
    }

    fn content<'a>(
        &self,
        filter_index: subtitle::ExtradataId,
        self_index: nde::graph::NodeId,
    ) -> iced::Element<'a, message::Message> {
        let initial_point = glam::DVec2 {
            x: self.region_center.x,
            y: self.region_center.y,
        };
        let initial_region = media::motion::Region::from_center_and_radius(initial_point, 20.0);
        let track_button = iced::widget::button("Track").on_press(
            message::Message::TrackMotionForNode(filter_index, self_index, initial_region),
        );

        let column = iced::widget::column![
            iced::widget::text(format!("{} frame(s) tracked", self.track.len())),
            track_button,
        ];

        column
            .spacing(4.0)
            .width(iced::Length::Fill)
            .align_x(iced::Alignment::Center)
            .into()
    }

    fn update(&mut self, message: message::Node) -> anyhow::Result<()> {
        if let message::Node::MotionTrackUpdate(relative_frame, region) = message {
            self.track.insert(relative_frame, region);
            Ok(())
        } else {
            anyhow::bail!("Invalid message type, expected MotionTrackUpdate");
        }
    }

    fn reticule_activate(&mut self, _context: &Context) -> Vec<reticule::Reticule> {
        vec![reticule::Reticule {
            shape: reticule::Shape::Cross,
            position: self.region_center,
            radius: 15.0,
        }]
    }

    fn reticule_update(
        &mut self,
        reticules: &mut reticule::Reticules,
        index: reticule::Index,
        new_position: glam::DVec2,
    ) -> anyhow::Result<glam::DVec2> {
        if index.0 != 0 {
            anyhow::bail!("Reticule index out of range: {index}");
        }

        let old_position = std::mem::replace(&mut reticules[index].position, new_position);
        self.region_center = new_position;

        Ok(old_position)
    }

    fn draw_reticule_base_layer(
        &self,
        canvas_frame: &mut canvas::Frame,
        bounds: Rectangle,
        storage_size: subtitle::Resolution,
        current_frame: Option<model::FrameNumber>,
        _cursor: Cursor,
    ) {
        if !self.track.is_empty() {
            let (first_frame, _) = self.track.first_key_value().unwrap();
            let (last_frame, _) = self.track.last_key_value().unwrap();

            for (frame_number, region) in &self.track {
                let iced_point =
                    view::frame_coordinates_to_iced(region.center, bounds.size(), storage_size);

                #[expect(
                    clippy::cast_sign_loss,
                    reason = "frame numbers should not be negative"
                )]
                let (red, green, blue) = colorous::VIRIDIS
                    .eval_rational(
                        (frame_number.0 - first_frame.0) as usize,
                        (last_frame.0 - first_frame.0) as usize + 1_usize,
                    )
                    .as_tuple();
                let iced_color = iced::Color::from_rgb8(red, green, blue);

                let circle = canvas::Path::circle(iced_point, 3.0_f32);

                if current_frame == Some(*frame_number) {
                    // Highlight the current frame with another yellow-orange border.
                    canvas_frame.stroke(
                        &circle,
                        canvas::Stroke::default()
                            .with_color(iced::Color::WHITE)
                            .with_width(9.0_f32),
                    );
                    canvas_frame.stroke(
                        &circle,
                        canvas::Stroke::default()
                            .with_color(style::SAMAKU_PRIMARY)
                            .with_width(6.0_f32),
                    );
                }

                canvas_frame.stroke(
                    &circle,
                    canvas::Stroke::default()
                        .with_color(iced::Color::WHITE)
                        .with_width(3.0_f32),
                );
                canvas_frame.fill(&circle, iced_color);
            }
        }
    }

    fn content_size(&self) -> iced::Size {
        iced::Size::new(200.0, 150.0)
    }
}

inventory::submit! {
    Shell::new(
        &["Motion track"],
        || Box::new(MotionTrack {
            region_center: glam::DVec2 {
                x: 100.0,
                y: 100.0,
            },
            track: BTreeMap::new(),
        })
    )
}
