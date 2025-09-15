use crate::media::FrameRate;
use crate::{message, style, subtitle, view};
use iced::widget::canvas;
use iced::widget::canvas::event;
use iced::{mouse, Renderer, Theme};

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct State {
    position: Position,
}

#[typetag::serde(name = "text_editor")]
impl super::LocalState for State {
    fn view<'a>(
        &'a self,
        _self_pane: super::Pane,
        global_state: &'a crate::Samaku,
    ) -> super::View<'a> {
        let canvas_data = CanvasData {
            position: self.position,
            frame_rate: global_state
                .video_metadata
                .map(|video_metadata| video_metadata.frame_rate),
        };

        let canvas = canvas(canvas_data)
            .width(iced::Length::Fill)
            .height(iced::Length::Fill);

        let top_bar = top_bar(self, global_state);

        let content = iced::widget::column![top_bar, view::separator(), canvas];

        super::View {
            title: iced::widget::text("Timeline").into(),
            content: iced::widget::container(content)
                .center_x(iced::Length::Fill)
                .center_y(iced::Length::Fill)
                .padding(5.0)
                .into(),
        }
    }

    fn update(&mut self, _pane_message: message::Pane) -> iced::Task<message::Message> {
        iced::Task::none()
    }
}

inventory::submit! {
    super::Shell::new(
        "Timeline",
        || Box::new(State::default())
    )
}

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
struct Position {
    center: subtitle::StartTime,

    /// How many pixels one millisecond should represent
    zoom_factor: f32,
}

impl Position {
    fn time_delta(&self, time: subtitle::StartTime) -> f32 {
        #[expect(
            clippy::cast_precision_loss,
            reason = "precision loss acceptable here since for very large values, the points will be very far outside the drawn area"
        )]
        let time_delta = (time - self.center).0 as f32;

        time_delta
    }

    fn time_to_point(
        &self,
        time: subtitle::StartTime,
        bounds: iced::Rectangle,
        y_factor: f32,
    ) -> iced::Point {
        iced::Point {
            x: self
                .time_delta(time)
                .mul_add(self.zoom_factor, bounds.x + bounds.width / 2.0),
            y: bounds.height.mul_add(y_factor, bounds.y),
        }
    }
}

impl Default for Position {
    fn default() -> Self {
        Self {
            center: subtitle::StartTime(0_i64),
            zoom_factor: 0.04,
        }
    }
}

struct CanvasData {
    position: Position,
    frame_rate: Option<FrameRate>,
}

impl canvas::Program<message::Message> for CanvasData {
    type State = ();

    fn update(
        &self,
        _state: &mut Self::State,
        _event: canvas::Event,
        _bounds: iced::Rectangle,
        _cursor: mouse::Cursor,
    ) -> (iced::event::Status, Option<message::Message>) {
        (event::Status::Ignored, None)
    }

    fn draw(
        &self,
        state: &Self::State,
        renderer: &Renderer,
        theme: &Theme,
        bounds: iced::Rectangle,
        cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry<Renderer>> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());

        draw_background(bounds, &mut frame, self.position);
        draw_seconds_ticks(&mut frame, self.position);

        vec![frame.into_geometry()]
    }

    fn mouse_interaction(
        &self,
        _state: &Self::State,
        _bounds: iced::Rectangle,
        _cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        iced::advanced::mouse::Interaction::default()
    }
}

fn draw_background(
    bounds: iced::Rectangle,
    frame: &mut canvas::Frame<Renderer>,
    position: Position,
) {
    let zero_point = position.time_to_point(subtitle::StartTime(0), bounds, 0.0);

    if zero_point.x < bounds.x {
        // Entire timeline is in the positive region
        frame.fill_rectangle(
            iced::Point::ORIGIN,
            frame.size(),
            style::SAMAKU_BACKGROUND_WEAK,
        );
    } else if zero_point.x > bounds.x + bounds.width {
        // Entire timeline is in the negative region
        frame.fill_rectangle(iced::Point::ORIGIN, frame.size(), style::SAMAKU_BACKGROUND);
    } else {
        // Part of the timeline is in the positive region: draw the part to the left of it darker than the part right of it
        let midpoint_x = zero_point.x - bounds.x;
        frame.fill_rectangle(
            iced::Point::ORIGIN,
            iced::Size {
                width: midpoint_x,
                height: frame.height(),
            },
            style::SAMAKU_BACKGROUND,
        );
        frame.fill_rectangle(
            iced::Point {
                x: midpoint_x,
                y: 0.0,
            },
            iced::Size {
                width: frame.width() - midpoint_x,
                height: frame.height(),
            },
            style::SAMAKU_BACKGROUND_WEAK,
        );
    }
}

fn draw_seconds_ticks(frame: &mut canvas::Frame<Renderer>, position: Position) {
    let stroke = canvas::Stroke {
        style: canvas::stroke::Style::Solid(style::SAMAKU_TEXT),
        width: 1.0,
        ..Default::default()
    };

    // Find first full second to the left of the right bound.
    let half_frame_ms_f32 = frame.width() * 1000.0 / (2.0 * position.zoom_factor);
    #[expect(
        clippy::cast_possible_truncation,
        reason = "truncating the sub-millisecond part is fine since the timeline cannot be more accurate than 1 ms"
    )]
    let half_frame_ms = subtitle::Duration(half_frame_ms_f32 as i64);
    let left_edge_ms = position.center - half_frame_ms;
    let right_edge_ms = position.center + half_frame_ms;
    let mut tick_ms = subtitle::StartTime(right_edge_ms.0 - (right_edge_ms.0.rem_euclid(1000)));

    while tick_ms >= subtitle::StartTime(0) && tick_ms >= left_edge_ms {
        let tick_x = position
            .time_delta(tick_ms)
            .mul_add(position.zoom_factor, frame.width() / 2.0);
        frame.stroke(
            &canvas::Path::line(
                iced::Point::new(tick_x, 0.0),
                iced::Point::new(tick_x, frame.height()),
            ),
            stroke,
        );

        tick_ms = tick_ms - subtitle::Duration(1000);
    }
}

fn top_bar<'a>(
    pane_state: &'a State,
    global_state: &'a crate::Samaku,
) -> iced::Element<'a, message::Message> {
    let play_button = iced::widget::button("Play").on_press(message::Message::TogglePlayback);

    let frame_number_text = if let Some(metadata) = global_state.video_metadata {
        let frame_number = global_state
            .shared
            .playback_position
            .current_frame(metadata.frame_rate)
            .0;
        format!("{frame_number}")
    } else {
        "No video loaded".to_owned()
    };

    let frame_number_text_widget = iced::widget::text(frame_number_text);

    iced::widget::container(
        iced::widget::row![play_button, frame_number_text_widget,]
            .spacing(5.0)
            .align_y(iced::Alignment::Center),
    )
    .padding(5.0)
    .into()
}
