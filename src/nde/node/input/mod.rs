use super::{BasicError, Category, Context, Node, Shell, SocketType, SocketValue, SocketValues};
use crate::model::reticule;
use crate::{media, message, nde, subtitle};

pub use motion_track::InputMotionTrack;
pub use perspective::InputQuad;
pub use rectangle::InputRectangle;

mod motion_track;
mod perspective;
mod rectangle;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct InputEvent;

#[typetag::serde]
impl Node for InputEvent {
    fn name(&self) -> &'static str {
        "Input: Event"
    }

    fn category(&self) -> Category {
        Category::Input
    }

    fn desired_inputs(&self) -> &[SocketType] {
        &[]
    }

    fn predicted_outputs(&self) -> &[SocketType] {
        &[SocketType::Event]
    }

    fn run<'a>(
        &'_ self,
        _inputs: SocketValues<'a>,
        context: &'a Context,
    ) -> anyhow::Result<SocketValues<'a>> {
        let Some(source_event) = context.source_event else {
            return Err(BasicError::MissingInput.into());
        };

        let nde_event = nde::Event::from_ass_event(source_event, context.frame_rate);

        let start = context
            .frame_rate
            .time_at_frame(nde_event.start, media::TimeMode::Exact);
        let duration = context
            .frame_rate
            .time_at_frame(nde_event.start + nde_event.duration, media::TimeMode::Exact)
            - start;

        let adapter = move |event: &mut nde::Event, frame| {
            let time = nde::tags::bake::TimeContext {
                start,
                duration,
                now: context
                    .frame_rate
                    .time_at_frame(frame, media::TimeMode::Exact),
            };
            let style_lookup = move |name: &str| {
                let index = context.styles.find_by_name(name)?;
                context.styles.get(index)
            };

            nde::tags::bake(
                time,
                context.get_event_style(event),
                &style_lookup,
                &mut event.global_tags,
                &mut event.text,
                context.playback_resolution,
                None,
            );
        };

        let track = nde::FramedTrack::from_single_with_adapter(nde_event, adapter);
        Ok(SocketValue::Event(track).into_values())
    }
}

inventory::submit! {
    Shell::new(
        &["Input", "Event"],
        || Box::new(InputEvent {})
    )
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct InputPosition {
    pub value: glam::DVec2,
}

#[typetag::serde]
impl Node for InputPosition {
    fn name(&self) -> &'static str {
        "Input: Position"
    }

    fn category(&self) -> Category {
        Category::Input
    }

    fn desired_inputs(&self) -> &[SocketType] {
        &[]
    }

    fn predicted_outputs(&self) -> &[SocketType] {
        &[SocketType::Position]
    }

    fn run<'a>(
        &'_ self,
        _inputs: SocketValues<'a>,
        _context: &'a Context,
    ) -> anyhow::Result<SocketValues<'a>> {
        Ok(SocketValue::Position(nde::FramedTrack::from_single(self.value)).into_values())
    }

    fn content<'a>(
        &'a self,
        _global_state: &'a crate::Samaku,
        _filter_index: subtitle::ExtradataId,
        _self_index: nde::graph::NodeId,
    ) -> iced::Element<'a, message::Message> {
        let column = iced::widget::column![iced::widget::text(format!(
            "x: {:.1}, y: {:.1}",
            self.value.x, self.value.y
        ))];

        column
            .spacing(4.0)
            .width(iced::Length::Fill)
            .align_x(iced::Alignment::Center)
            .into()
    }

    fn reticule_activate(&mut self, _context: &Context) -> Vec<reticule::Reticule> {
        // TODO start with active event position
        vec![reticule::Reticule {
            shape: reticule::Shape::Cross,
            position: self.value,
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
        self.value = new_position;

        Ok(old_position)
    }

    fn content_size(&self) -> iced::Size {
        iced::Size::new(200.0, 125.0)
    }
}

inventory::submit! {
    Shell::new(
        &["Input", "Position"],
        || Box::new(InputPosition {
            value: glam::DVec2 {
                x: 100.0,
                y: 100.0,
            }
        })
    )
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct InputTags {
    pub value: String,
}

#[typetag::serde]
impl Node for InputTags {
    fn name(&self) -> &'static str {
        "Input: Tags"
    }

    fn category(&self) -> Category {
        Category::Input
    }

    fn desired_inputs(&self) -> &[SocketType] {
        &[]
    }

    fn predicted_outputs(&self) -> &[SocketType] {
        &[SocketType::LocalTags, SocketType::GlobalTags]
    }

    fn run<'a>(
        &'_ self,
        _inputs: SocketValues<'a>,
        _context: &'a Context,
    ) -> anyhow::Result<SocketValues<'a>> {
        if self.value.contains('{') || self.value.contains('}') {
            anyhow::bail!("Input tags contain brackets");
        }

        // Turns "a" into "{a}"
        let block = format!("{{{}}}", self.value);
        let (global, spans) = nde::tags::parse_raw(&block);

        assert_eq!(
            spans.len(),
            2,
            "since the input is guaranteed to contain no brackets, there should be exactly 2 spans"
        );
        let nde::Span::Tags(local, _) = spans.into_iter().nth(1).unwrap() else {
            panic!("span should be `Tags`")
        };

        Ok(smallvec::smallvec![
            SocketValue::LocalTags(nde::FramedTrack::from_single(local)),
            SocketValue::GlobalTags(nde::FramedTrack::from_single(*global)),
        ])
    }

    fn content<'a>(
        &'a self,
        _global_state: &'a crate::Samaku,
        filter_index: subtitle::ExtradataId,
        self_index: nde::graph::NodeId,
    ) -> iced::Element<'a, message::Message> {
        let input =
            iced::widget::text_input("\\1c&HFF0000&", &self.value).on_input(move |new_text| {
                message::Message::Node(
                    filter_index,
                    self_index,
                    message::Node::TextInputChanged(new_text),
                )
            });

        let column = iced::widget::column![input];

        column
            .spacing(4.0)
            .width(iced::Length::Fill)
            .align_x(iced::Alignment::Center)
            .into()
    }

    fn update(&mut self, message: message::Node) -> anyhow::Result<()> {
        if let message::Node::TextInputChanged(value) = message {
            self.value = value;
            Ok(())
        } else {
            anyhow::bail!("Invalid message type, expected TextInputChanged");
        }
    }

    fn content_size(&self) -> iced::Size {
        iced::Size::new(400.0, 125.0)
    }
}

inventory::submit! {
    Shell::new(
        &["Input", "Tags"],
        || Box::new(InputTags {
            value: String::new()
        })
    )
}
