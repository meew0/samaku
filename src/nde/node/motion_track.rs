use std::collections::HashMap;

use crate::{media, message, model, nde, subtitle};
use model::reticule;

use super::{Node, Shell, SocketType, SocketValue};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
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

    fn run(&'_ self, inputs: &[&SocketValue]) -> anyhow::Result<Vec<SocketValue<'_>>> {
        assert!(
            inputs.len() > 1,
            "the required number of inputs should be present"
        ); // Elide bounds checks

        super::retrieve!(inputs[0], &SocketValue::MultipleEvents(ref events));
        super::retrieve!(inputs[1], &SocketValue::FrameRate(ref frame_rate));

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

    fn content<'a>(
        &self,
        filter_index: subtitle::ExtradataId,
        self_index: nde::graph::NodeId,
    ) -> iced::Element<'a, message::Message> {
        let initial_point = media::motion::Point {
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

    fn reticule_activate(&mut self) -> Vec<reticule::Reticule> {
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
        new_position: nde::tags::Position,
    ) -> anyhow::Result<nde::tags::Position> {
        if index.0 != 0 {
            anyhow::bail!("Reticule index out of range: {index}");
        }

        let old_position = std::mem::replace(&mut reticules[index].position, new_position);
        self.region_center = new_position;

        Ok(old_position)
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
