use super::{Context, Node, Shell, SocketType, SocketValue};
use crate::media::motion;
use crate::{message, model, nde, subtitle, view};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MotionTrack {
    pub track_id: Option<motion::TrackId>,
    #[serde(skip)]
    pub blend_box_state: view::widget::blend_box::State,
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

        let Some(track_id) = self.track_id else {
            anyhow::bail!("No motion track selected");
        };
        let Some(tracks) = context.motion_tracks else {
            anyhow::bail!("Missing motion tracks in compile context");
        };
        let Some(track) = tracks.get(track_id) else {
            anyhow::bail!("Invalid motion track ID");
        };

        for event in events {
            let mut cloned = event.clone();
            let frame = frame_rate.ms_to_frame(cloned.start.0);
            if let Some(marker) = track.get_marker(frame) {
                cloned.global_tags.position =
                    Some(nde::tags::PositionOrMove::Position(marker.region.center));
            }
            new_events.push(cloned);
        }

        Ok(vec![SocketValue::MultipleEvents(new_events)])
    }

    fn content<'a>(
        &'a self,
        global_state: &'a crate::Samaku,
        filter_index: subtitle::ExtradataId,
        self_index: nde::graph::NodeId,
    ) -> iced::Element<'a, message::Message> {
        let (text, selection) = if let Some(track_id) = self.track_id {
            if let Some(track) = global_state.motion_tracks.get(track_id) {
                let count = track.count();
                let s_str = if count == 1 { "" } else { "s" };
                let selection = model::NamedEntry {
                    id: track_id,
                    name: model::Named::name(track),
                };

                (
                    iced::widget::text(format!("{count} frame{s_str} tracked")),
                    Some(selection),
                )
            } else {
                (iced::widget::text("Invalid track"), None)
            }
        } else {
            (iced::widget::text("No track selected"), None)
        };

        let track_button = view::widget::blend_box(
            &self.blend_box_state,
            &global_state.motion_tracks,
            "Motion track",
            selection,
            move |new_selection| {
                message::Message::Node(
                    filter_index,
                    self_index,
                    message::Node::MotionTrackSelect(new_selection),
                )
            },
        );

        let column = iced::widget::column![text, track_button];

        column
            .spacing(4.0)
            .width(iced::Length::Fill)
            .align_x(iced::Alignment::Center)
            .into()
    }

    fn update(&mut self, message: message::Node) -> anyhow::Result<()> {
        if let message::Node::MotionTrackSelect(new_selection) = message {
            self.track_id = Some(new_selection);
            Ok(())
        } else {
            anyhow::bail!("Invalid message type, expected MotionTrackSelect");
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
        track_id: None,blend_box_state: view::widget::blend_box::State::default(),})
    )
}
