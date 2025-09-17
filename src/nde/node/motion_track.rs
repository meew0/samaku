use std::collections::HashMap;

use crate::{media, message, model, nde};

use super::{Error, Node, Shell, SocketType, SocketValue};

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct MotionTrack {
    pub region_center: nde::tags::Position,
    pub track: HashMap<model::FrameNumber, media::motion::Region>,
}

#[typetag::serde]
impl Node for MotionTrack {
    fn name(&self) -> &'static str {
        "Motion track"
    }

    fn desired_inputs(&self) -> &[SocketType] {
        &[
            SocketType::MultipleEvents,
            SocketType::LeafInput(super::LeafInputType::FrameRate),
        ]
    }

    fn predicted_outputs(&self) -> &[SocketType] {
        &[SocketType::MultipleEvents]
    }

    fn run(&'_ self, inputs: &[&SocketValue]) -> Result<Vec<SocketValue<'_>>, Error> {
        assert!(inputs.len() > 1); // Elide bounds checks

        super::retrieve!(inputs[0], SocketValue::MultipleEvents(events));
        super::retrieve!(inputs[1], SocketValue::FrameRate(frame_rate));

        let mut new_events: Vec<nde::Event> = vec![];

        for event in events {
            let mut cloned = event.clone();
            let frame = frame_rate.ms_to_frame(cloned.start.0);
            if let Some(region) = self.track.get(&frame) {
                cloned.global_tags.position =
                    Some(nde::tags::PositionOrMove::Position(nde::tags::Position {
                        x: region.center.x,
                        y: region.center.y,
                    }));
            }
            new_events.push(cloned);
        }

        Ok(vec![SocketValue::MultipleEvents(new_events)])
    }

    fn content<'a>(&self, self_index: usize) -> iced::Element<'a, message::Message> {
        let set_marker_button = iced::widget::button("Set marker").on_press(
            message::Message::SetReticules(model::reticule::Reticules {
                list: vec![model::reticule::Reticule {
                    shape: model::reticule::Shape::Cross,
                    position: self.region_center,
                    radius: 15.0,
                }],
                source_node_index: self_index,
            }),
        );

        let initial_point = media::motion::Point {
            x: self.region_center.x,
            y: self.region_center.y,
        };
        let initial_region = media::motion::Region::from_center_and_radius(initial_point, 20.0);
        let track_button = iced::widget::button("Track").on_press(
            message::Message::TrackMotionForNode(self_index, initial_region),
        );

        let column = iced::widget::column![
            iced::widget::text(self.name()),
            iced::widget::text(format!("{} frame(s) tracked", self.track.len())),
            set_marker_button,
            track_button,
        ];

        column.align_x(iced::Alignment::Center).into()
    }

    fn update(&mut self, message: message::Node) {
        if let message::Node::MotionTrackUpdate(relative_frame, region) = message {
            self.track.insert(relative_frame, region);
        }
    }

    fn reticule_update(
        &mut self,
        reticules: &mut model::reticule::Reticules,
        index: usize,
        new_position: nde::tags::Position,
    ) {
        if index != 0 {
            return;
        }

        reticules.list[0].position = new_position;
        self.region_center = new_position;
    }

    fn content_size(&self) -> iced::Size {
        iced::Size::new(200.0, 150.0)
    }
}

inventory::submit! {
    Shell::new(
        &["Motion track"],
        || Box::new(MotionTrack {
            region_center: nde::tags::Position {
                x: 100.0,
                y: 100.0,
            },
            track: HashMap::new(),
        })
    )
}
