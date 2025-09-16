use crate::media::FrameRate;
use crate::{message, model, style, subtitle, view};
use iced::widget::canvas;
use iced::widget::canvas::event;
use iced::{Renderer, Theme, mouse};
use std::cell::RefCell;

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct State {
    position: Position,
}

#[typetag::serde(name = "text_editor")]
impl super::LocalState for State {
    fn view<'a>(
        &'a self,
        self_pane: super::Pane,
        global_state: &'a crate::Samaku,
    ) -> super::View<'a> {
        let canvas_data = CanvasData {
            pane: self_pane,
            position: self.position,
            frame_rate: global_state
                .video_metadata
                .map(|video_metadata| video_metadata.frame_rate),
            playback_position: global_state.shared.playback_position.subtitle_time(),
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

    fn update(&mut self, pane_message: message::Pane) -> iced::Task<message::Message> {
        match pane_message {
            message::Pane::TimelineDragged(new_position) => {
                self.position = new_position;
            }
            _ => {}
        }

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
pub struct Position {
    pub left: subtitle::StartTime,
    pub right: subtitle::StartTime,
}

impl Position {
    fn time_delta(&self, time: subtitle::StartTime) -> f32 {
        #[expect(
            clippy::cast_precision_loss,
            reason = "precision loss acceptable within the precision limits of the timeline"
        )]
        let time_delta = (time - self.left).0 as f32;

        time_delta
    }

    fn pixel_per_ms(&self, pixel_width: f32) -> f32 {
        let timeline_width = self.right - self.left;
        #[expect(
            clippy::cast_precision_loss,
            reason = "precision loss acceptable within the precision limits of the timeline"
        )]
        let result = pixel_width / timeline_width.0 as f32;
        result
    }

    fn ms_from_left(&self, position: iced::Point, pixel_width: f32) -> subtitle::Duration {
        let x_factor = position.x / pixel_width;
        let timeline_width = self.right - self.left;
        #[expect(
            clippy::cast_possible_truncation,
            reason = "allowed within the precision limits of the timeline"
        )]
        #[expect(
            clippy::cast_precision_loss,
            reason = "precision loss acceptable within the precision limits of the timeline"
        )]
        let result = subtitle::Duration(((timeline_width.0 as f32) * x_factor) as i64);
        result
    }

    #[must_use]
    pub fn offset(&self, delta: subtitle::Duration) -> Self {
        Self {
            left: self.left + delta,
            right: self.right + delta,
        }
    }
}

impl Default for Position {
    fn default() -> Self {
        Self {
            left: subtitle::StartTime(-10000),
            right: subtitle::StartTime(10000),
        }
    }
}

struct CanvasData {
    pane: super::Pane,
    position: Position,
    frame_rate: Option<FrameRate>,
    playback_position: subtitle::StartTime,
}

#[derive(Default)]
struct CanvasState {
    drag_start: Option<iced::Point>,
    drag_mode: DragMode,
    moved: bool,
    view_state: RefCell<ViewState>,
}

#[derive(Default)]
enum DragMode {
    #[default]
    None,
    Pan(Position),
    Cursor,
}

#[derive(Default)]
struct ViewState {
    cursor_x: Option<f32>,
}

impl ViewState {
    fn can_grab_cursor(&self, mouse_position: iced::Point) -> bool {
        if let Some(cursor_x) = self.cursor_x
            && (mouse_position.x - cursor_x).abs() < 6.0
        {
            return true;
        }

        false
    }
}

impl canvas::Program<message::Message> for CanvasData {
    type State = CanvasState;

    fn update(
        &self,
        state: &mut Self::State,
        event: canvas::Event,
        bounds: iced::Rectangle,
        cursor: mouse::Cursor,
    ) -> (iced::event::Status, Option<message::Message>) {
        match event {
            canvas::Event::Mouse(mouse_event) => {
                match mouse_event {
                    mouse::Event::ButtonPressed(mouse::Button::Left) => {
                        state.drag_start = cursor.position_in(bounds);
                        if let Some(mouse_position) = state.drag_start {
                            state.drag_mode =
                                if state.view_state.borrow().can_grab_cursor(mouse_position) {
                                    DragMode::Cursor
                                } else {
                                    DragMode::Pan(self.position)
                                };
                        }
                        state.moved = false;
                    }
                    mouse::Event::ButtonReleased(mouse::Button::Left) => {
                        state.drag_start = None;
                        if !state.moved
                            && let Some(mouse_position) = cursor.position_in(bounds)
                        {
                            let new_time = self.position.left
                                + self.position.ms_from_left(mouse_position, bounds.width);
                            return (
                                event::Status::Captured,
                                Some(message::Message::PlaybackSetPosition(new_time)),
                            );
                        }
                    }
                    mouse::Event::CursorMoved { .. } => {
                        if let Some(mouse_position) = cursor.position_in(bounds) {
                            state.moved = true;
                            if let Some(drag_start) = state.drag_start {
                                let x_from_start = drag_start.x - mouse_position.x;
                                let pixel_per_ms = self.position.pixel_per_ms(bounds.width);
                                #[expect(
                                    clippy::cast_possible_truncation,
                                    reason = "allowed within the precision limits of the timeline"
                                )]
                                let ms_dragged =
                                    subtitle::Duration((x_from_start / pixel_per_ms) as i64);

                                match state.drag_mode {
                                    DragMode::Pan(start_position) => {
                                        return (
                                            event::Status::Captured,
                                            Some(message::Message::Pane(
                                                self.pane,
                                                message::Pane::TimelineDragged(
                                                    start_position.offset(ms_dragged),
                                                ),
                                            )),
                                        );
                                    }
                                    DragMode::Cursor => {
                                        let new_time = self.position.left
                                            + self
                                                .position
                                                .ms_from_left(mouse_position, bounds.width);
                                        return (
                                            event::Status::Captured,
                                            Some(message::Message::PlaybackSetPosition(new_time)),
                                        );
                                    }
                                    DragMode::None => {}
                                }
                            }
                        }
                    }
                    mouse::Event::WheelScrolled { delta } => {
                        if cursor.position_in(bounds).is_some() {
                            return self.calculate_zoom(bounds, cursor, delta);
                        }
                    }

                    _ => {}
                }

                return (event::Status::Captured, None);
            }
            canvas::Event::Touch(_) | canvas::Event::Keyboard(_) => {}
        }

        (event::Status::Ignored, None)
    }

    fn draw(
        &self,
        state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: iced::Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry<Renderer>> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());

        draw_background(bounds, &mut frame, self.position);

        if let Some(frame_rate) = self.frame_rate
            && self.position.pixel_per_ms(bounds.width) > 0.4
        {
            draw_frame_ticks(&mut frame, self.position, frame_rate);
        }
        draw_seconds_ticks(&mut frame, self.position);

        if self.frame_rate.is_some() {
            draw_cursor(
                &mut frame,
                self.position,
                self.playback_position,
                &mut state.view_state.borrow_mut(),
            );
        } else {
            state.view_state.borrow_mut().cursor_x = None;
        }

        vec![frame.into_geometry()]
    }

    fn mouse_interaction(
        &self,
        state: &Self::State,
        bounds: iced::Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if state.drag_start.is_some() {
            return mouse::Interaction::Grabbing;
        }

        if let Some(mouse_position) = cursor.position_in(bounds) {
            return if state.view_state.borrow().can_grab_cursor(mouse_position) {
                mouse::Interaction::ResizingHorizontally
            } else {
                mouse::Interaction::Grab
            };
        }

        mouse::Interaction::default()
    }
}

impl CanvasData {
    fn calculate_zoom(
        &self,
        bounds: iced::Rectangle,
        cursor: mouse::Cursor,
        delta: mouse::ScrollDelta,
    ) -> (iced::event::Status, Option<message::Message>) {
        let y = match delta {
            mouse::ScrollDelta::Lines { y, .. } => y,
            mouse::ScrollDelta::Pixels { y, .. } => y / 100.0, // TODO is this reasonable?
        };

        let modifier_factor = 1.2_f32.powf(y);
        let offset_factor = modifier_factor - 1.0;

        let x_factor = if let Some(mouse_position) = cursor.position_in(bounds) {
            mouse_position.x / bounds.width
        } else {
            0.5
        };

        #[expect(
            clippy::cast_precision_loss,
            reason = "precision loss acceptable within the precision limits of the timeline"
        )]
        let timeline_width_f = (self.position.right - self.position.left).0 as f32;

        #[expect(
            clippy::cast_possible_truncation,
            reason = "acceptable within the precision limits of the timeline"
        )]
        let new_left = self.position.left
            + subtitle::Duration((timeline_width_f * x_factor * offset_factor) as i64);
        #[expect(
            clippy::cast_possible_truncation,
            reason = "acceptable within the precision limits of the timeline"
        )]
        let new_right = self.position.right
            - subtitle::Duration((timeline_width_f * (1.0 - x_factor) * offset_factor) as i64);
        let new_position = Position {
            left: new_left,
            right: new_right,
        };

        let new_pixel_per_ms = new_position.pixel_per_ms(bounds.width);
        if new_pixel_per_ms < 1.0 && new_pixel_per_ms > 0.001 {
            return (
                event::Status::Captured,
                Some(message::Message::Pane(
                    self.pane,
                    message::Pane::TimelineDragged(new_position),
                )),
            );
        }

        (event::Status::Captured, None)
    }
}

fn draw_background(
    bounds: iced::Rectangle,
    frame: &mut canvas::Frame<Renderer>,
    position: Position,
) {
    let zero = subtitle::StartTime(0);
    if zero < position.left {
        // Entire timeline is in the positive region
        frame.fill_rectangle(
            iced::Point::ORIGIN,
            frame.size(),
            style::SAMAKU_BACKGROUND_WEAK,
        );
    } else if zero > position.right {
        // Entire timeline is in the negative region
        frame.fill_rectangle(iced::Point::ORIGIN, frame.size(), style::SAMAKU_BACKGROUND);
    } else {
        // Part of the timeline is in the positive region: draw the part to the left of it darker than the part right of it
        let midpoint_x = position.time_delta(zero) * position.pixel_per_ms(bounds.width);
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
    let pixel_per_ms = position.pixel_per_ms(frame.width());
    #[expect(
        clippy::cast_possible_truncation,
        reason = "truncation desired in this case"
    )]
    let step = ((1000.0 * 2_f32.powi(-3 - pixel_per_ms.log2().round() as i32)) as i64).max(500);

    // Find first full second to the left of the right bound.
    let mut tick_ms = subtitle::StartTime(position.right.0 - (position.right.0.rem_euclid(step)));

    while tick_ms >= subtitle::StartTime(0) && tick_ms >= position.left {
        let tick_x = position.time_delta(tick_ms) * pixel_per_ms;

        frame.fill_rectangle(
            iced::Point::new(tick_x, 20.0),
            iced::Size::new(1.0, frame.height()),
            style::SAMAKU_TEXT,
        );

        frame.fill_text(canvas::Text {
            content: tick_ms.format_short(),
            position: iced::Point::new(tick_x, 2.0),
            color: style::SAMAKU_TEXT,
            font: crate::DEFAULT_FONT,
            size: iced::Pixels(14.0),
            horizontal_alignment: iced::alignment::Horizontal::Center,
            ..Default::default()
        });

        tick_ms = tick_ms - subtitle::Duration(step);
    }
}

fn draw_frame_ticks(
    frame: &mut canvas::Frame<Renderer>,
    position: Position,
    frame_rate: FrameRate,
) {
    let pixel_per_ms = position.pixel_per_ms(frame.width());
    let first_frame = model::FrameNumber(frame_rate.ms_to_frame(position.left.0).0.max(0));

    for (frame_number, time_ms) in frame_rate.iter_from(first_frame) {
        if time_ms > position.right.0 {
            break;
        }

        let tick_x = position.time_delta(subtitle::StartTime(time_ms)) * pixel_per_ms;

        frame.fill_rectangle(
            iced::Point::new(tick_x, 30.0),
            iced::Size::new(1.0, frame.height()),
            style::SAMAKU_TEXT_WEAK,
        );

        frame.fill_text(canvas::Text {
            content: format!("{}", frame_number.0),
            position: iced::Point::new(tick_x, 17.0),
            color: style::SAMAKU_TEXT_WEAK,
            font: crate::DEFAULT_FONT,
            size: iced::Pixels(9.0),
            horizontal_alignment: iced::alignment::Horizontal::Center,
            ..Default::default()
        });
    }
}

fn draw_cursor(
    frame: &mut canvas::Frame<Renderer>,
    position: Position,
    playback_position: subtitle::StartTime,
    view_state: &mut ViewState,
) {
    let cursor_x = position.time_delta(playback_position) * position.pixel_per_ms(frame.width());
    view_state.cursor_x = Some(cursor_x);
    frame.fill_rectangle(
        iced::Point::new(cursor_x - 1.0, 0.0),
        iced::Size::new(2.0, frame.height()),
        style::SAMAKU_PRIMARY,
    );
    draw_equilateral_triangle(
        frame,
        iced::Point::new(cursor_x, 0.0),
        iced::Point::new(cursor_x, 10.0),
        style::SAMAKU_PRIMARY,
    );
}

fn draw_equilateral_triangle(
    frame: &mut canvas::Frame<Renderer>,
    base_center: iced::Point,
    apex: iced::Point,
    fill: impl Into<canvas::Fill>,
) {
    // Vector from apex to base center (the altitude)
    let vx = base_center.x - apex.x;
    let vy = base_center.y - apex.y;
    let height = vx.hypot(vy);

    // Degenerate case: nothing to draw
    if height <= f32::EPSILON {
        return;
    }

    // Half of the base length: h / √3
    let half_base = height / 3.0_f32.sqrt();

    // Unit vector perpendicular to the altitude
    let ux = -vy / height;
    let uy = vx / height;

    // Base vertices on either side of the base center
    let b1 = iced::Point::new(
        ux.mul_add(half_base, base_center.x),
        uy.mul_add(half_base, base_center.y),
    );
    let b2 = iced::Point::new(
        ux.mul_add(-half_base, base_center.x),
        uy.mul_add(-half_base, base_center.y),
    );

    // Build a closed triangle path: apex -> b1 -> b2
    let tri = canvas::Path::new(|builder| {
        builder.move_to(apex);
        builder.line_to(b1);
        builder.line_to(b2);
        builder.close();
    });

    frame.fill(&tri, fill);
}

fn top_bar<'a>(
    _pane_state: &'a State,
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
