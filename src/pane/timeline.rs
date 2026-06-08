use crate::media::FrameRate;
use crate::{media, message, pane, style, subtitle, view};
use iced::keyboard::Modifiers;
use iced::widget::{Action, canvas};
use iced::{Renderer, Theme, keyboard, mouse};
use std::cell::RefCell;

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct State {
    position: Position,
}

#[typetag::serde(name = "timeline")]
impl super::LocalState for State {
    fn view<'a>(
        &'a self,
        self_pane: super::Pane,
        global_state: &'a crate::Samaku,
    ) -> super::View<'a> {
        let video_start = subtitle::StartTime(0);

        let canvas_data = CanvasData {
            pane: self_pane,
            position: self.position,
            frame_rate: global_state
                .video_metadata
                .as_ref()
                .map(|video_metadata| &video_metadata.frame_rate),
            video_bounds: VideoBounds {
                start: video_start,
                end: video_start
                    + global_state
                        .video_metadata
                        .as_ref()
                        .map_or(subtitle::Duration(0), |video_metadata| {
                            video_metadata.duration
                        }),
            },
            playback_position: global_state.shared.playback_position.subtitle_time(),
            events: global_state
                .subtitles
                .events
                .iter_range(&(self.position.left..self.position.right))
                .map(|index| {
                    let event = &global_state.subtitles.events[index];
                    EventReference {
                        index,
                        start: event.start,
                        duration: event.duration,
                        selected: global_state.selected_events.contains(index),
                    }
                })
                .collect(),
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

struct CanvasData<'a> {
    pane: super::Pane,
    position: Position,
    frame_rate: Option<&'a FrameRate>,
    playback_position: subtitle::StartTime,
    video_bounds: VideoBounds,
    events: Vec<EventReference>,
}

#[derive(Debug, Clone, Copy)]
struct VideoBounds {
    start: subtitle::StartTime,
    end: subtitle::StartTime,
}

#[derive(Clone)]
struct EventReference {
    index: subtitle::EventIndex,
    start: subtitle::StartTime,
    duration: subtitle::Duration,
    selected: bool,
}

impl EventReference {
    fn overlaps(&self, other: &EventReference) -> bool {
        self.start < (other.start + other.duration) && other.start < (self.start + self.duration)
    }
}

#[derive(Default)]
struct CanvasState {
    drag_start: Option<iced::Point>,
    drag_mode: DragMode,
    moved: bool,
    view_state: RefCell<ViewState>,
    control_held: bool,
}

#[derive(Default)]
enum DragMode {
    #[default]
    None,
    Pan(Position),
    Cursor,
    Event(EventDragAction, EventReference),
}

#[derive(Default)]
struct ViewState {
    cursor_x: Option<f32>,
    subtitle_areas: Vec<(iced::Rectangle, EventDragAction, EventReference)>,
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

    fn subtitle_to_grab(
        &self,
        mouse_position: iced::Point,
    ) -> Option<(EventDragAction, EventReference)> {
        for &(ref bounds, drag_action, ref event_reference) in &self.subtitle_areas {
            if bounds.contains(mouse_position) {
                return Some((drag_action, event_reference.clone()));
            }
        }
        None
    }
}

#[derive(Debug, Clone, Copy)]
enum EventDragAction {
    Left,
    Center,
    Right,
}

impl canvas::Program<message::Message> for CanvasData<'_> {
    type State = CanvasState;

    fn update(
        &self,
        state: &mut Self::State,
        event: &canvas::Event,
        bounds: iced::Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<Action<message::Message>> {
        match *event {
            canvas::Event::Mouse(ref mouse_event) => {
                match *mouse_event {
                    mouse::Event::ButtonPressed(mouse::Button::Left) => {
                        state.drag_start = cursor.position_in(bounds);
                        if let Some(mouse_position) = state.drag_start {
                            state.drag_mode = if let Some((drag_action, event_reference)) =
                                state.view_state.borrow().subtitle_to_grab(mouse_position)
                            {
                                DragMode::Event(drag_action, event_reference)
                            } else if state.view_state.borrow().can_grab_cursor(mouse_position) {
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
                            return match state.drag_mode {
                                DragMode::None | DragMode::Pan(_) | DragMode::Cursor => {
                                    let new_time = self.position.left
                                        + self.position.ms_from_left(mouse_position, bounds.width);
                                    let new_time_bounded = new_time
                                        .max(self.video_bounds.start)
                                        .min(self.video_bounds.end - subtitle::Duration(1));
                                    let message =
                                        message::Message::PlaybackSetPosition(new_time_bounded);
                                    Some(Action::publish(message).and_capture())
                                }
                                DragMode::Event(_, ref event_reference) => {
                                    let message = if state.control_held {
                                        message::Message::ToggleEventSelection(
                                            event_reference.index,
                                        )
                                    } else {
                                        message::Message::SelectOnlyEvent(event_reference.index)
                                    };
                                    Some(Action::publish(message).and_capture())
                                }
                            };
                        }
                    }
                    mouse::Event::CursorMoved { .. } => {
                        if let Some(mouse_position) = cursor.position_in(bounds) {
                            state.moved = true;
                            if let Some(drag_start) = state.drag_start {
                                let action =
                                    self.handle_drag(state, bounds, mouse_position, drag_start);
                                return action;
                            }
                        }
                    }
                    mouse::Event::WheelScrolled { delta }
                        if cursor.position_in(bounds).is_some() =>
                    {
                        return Some(self.calculate_zoom(bounds, cursor, delta));
                    }

                    _ => {}
                }

                return Some(Action::capture());
            }
            canvas::Event::Keyboard(ref keyboard_event) => match *keyboard_event {
                keyboard::Event::ModifiersChanged(ref modifiers) => {
                    state.control_held = modifiers.contains(Modifiers::CTRL);
                }
                _ => {}
            },
            canvas::Event::Touch(_) | canvas::Event::Window(_) | canvas::Event::InputMethod(_) => {}
        }

        None
    }

    fn draw(
        &self,
        state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: iced::Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry<Renderer>> {
        let draw_bounds = bounds;
        let mut frame = canvas::Frame::new(renderer, draw_bounds.size());

        draw_background(draw_bounds, &mut frame, self.video_bounds, self.position);

        if let Some(frame_rate) = self.frame_rate
            && self.position.pixel_per_ms(draw_bounds.width) > 0.4
        {
            draw_frame_ticks(&mut frame, self.video_bounds, self.position, frame_rate);
        }
        draw_seconds_ticks(&mut frame, self.video_bounds, self.position);

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

        draw_subtitle_stack(
            &mut frame,
            self.position,
            &self.events,
            &mut state.view_state.borrow_mut(),
        );

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
            return if let Some((drag_action, _)) =
                state.view_state.borrow().subtitle_to_grab(mouse_position)
            {
                match drag_action {
                    EventDragAction::Left | EventDragAction::Right => {
                        mouse::Interaction::ResizingHorizontally
                    }
                    EventDragAction::Center => mouse::Interaction::Pointer,
                }
            } else if state.view_state.borrow().can_grab_cursor(mouse_position) {
                mouse::Interaction::ResizingHorizontally
            } else {
                mouse::Interaction::Grab
            };
        }

        mouse::Interaction::default()
    }
}

impl CanvasData<'_> {
    fn calculate_zoom(
        &self,
        bounds: iced::Rectangle,
        cursor: mouse::Cursor,
        delta: mouse::ScrollDelta,
    ) -> Action<message::Message> {
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
        if new_pixel_per_ms < 1.0 && new_pixel_per_ms > 0.0001 {
            let message =
                message::Message::Pane(self.pane, message::Pane::TimelineDragged(new_position));

            return Action::publish(message).and_capture();
        }

        Action::capture()
    }

    fn handle_drag(
        &self,
        state: &CanvasState,
        bounds: iced::Rectangle,
        mouse_position: iced::Point,
        drag_start: iced::Point,
    ) -> Option<Action<message::Message>> {
        const MIN_DURATION: i64 = 10;

        let x_from_start = drag_start.x - mouse_position.x;
        let pixel_per_ms = self.position.pixel_per_ms(bounds.width);
        #[expect(
            clippy::cast_possible_truncation,
            reason = "allowed within the precision limits of the timeline"
        )]
        let ms_dragged = subtitle::Duration((x_from_start / pixel_per_ms) as i64);

        match state.drag_mode {
            DragMode::Pan(start_position) => {
                let message = message::Message::Pane(
                    self.pane,
                    message::Pane::TimelineDragged(start_position.offset(ms_dragged)),
                );
                return Some(Action::publish(message).and_capture());
            }
            DragMode::Cursor => {
                let new_time =
                    self.position.left + self.position.ms_from_left(mouse_position, bounds.width);
                let new_time_bounded = new_time
                    .max(self.video_bounds.start)
                    .min(self.video_bounds.end - subtitle::Duration(1));
                let message = message::Message::PlaybackSetPosition(new_time_bounded);
                return Some(Action::publish(message).and_capture());
            }
            DragMode::Event(drag_action, ref event_reference) => {
                let new_time =
                    self.position.left + self.position.ms_from_left(mouse_position, bounds.width);
                let message = match drag_action {
                    EventDragAction::Left => {
                        let new_duration = subtitle::Duration(
                            (event_reference.duration.0 - (new_time - event_reference.start).0)
                                .max(MIN_DURATION),
                        );
                        message::Message::SetEventStartTimeAndDuration(
                            event_reference.index,
                            event_reference.start + event_reference.duration - new_duration,
                            new_duration,
                        )
                    }
                    EventDragAction::Center => {
                        let drag_start_time = self.position.left
                            + self.position.ms_from_left(drag_start, bounds.width);
                        let offset = new_time - drag_start_time;
                        message::Message::SetEventStartTimeAndDuration(
                            event_reference.index,
                            event_reference.start + offset,
                            event_reference.duration,
                        )
                    }
                    EventDragAction::Right => message::Message::SetEventStartTimeAndDuration(
                        event_reference.index,
                        event_reference.start,
                        subtitle::Duration(
                            (event_reference.duration.0
                                - ((event_reference.start + event_reference.duration) - new_time)
                                    .0)
                                .max(10),
                        ),
                    ),
                };
                return Some(Action::publish(message).and_capture());
            }
            DragMode::None => {}
        }
        None
    }
}

fn background_x_bounds(position: Position, video_bounds: VideoBounds, width: f32) -> (f32, f32) {
    let pixel_per_ms = position.pixel_per_ms(width);
    let start_x = (position.time_delta(video_bounds.start) * pixel_per_ms).clamp(0.0, width);
    let end_x = (position.time_delta(video_bounds.end) * pixel_per_ms).clamp(0.0, width);
    (start_x, end_x)
}

fn seconds_tick_positions(
    position: Position,
    video_bounds: VideoBounds,
    step: i64,
) -> Vec<subtitle::StartTime> {
    let right_limit = position.right.0.min(video_bounds.end.0.saturating_sub(1));
    let mut tick_ms = subtitle::StartTime(right_limit - right_limit.rem_euclid(step));
    let mut ticks = Vec::new();
    while tick_ms >= position.left && tick_ms >= video_bounds.start {
        ticks.push(tick_ms);
        tick_ms = tick_ms - subtitle::Duration(step);
    }
    ticks
}

fn frame_tick_bounds(
    position: Position,
    video_bounds: VideoBounds,
) -> (subtitle::StartTime, subtitle::StartTime) {
    let left_bound = position.left.0.max(video_bounds.start.0);
    let right_bound = position.right.0.min(video_bounds.end.0);
    (
        subtitle::StartTime(left_bound),
        subtitle::StartTime(right_bound),
    )
}

fn draw_background(
    draw_bounds: iced::Rectangle,
    frame: &mut canvas::Frame<Renderer>,
    video_bounds: VideoBounds,
    position: Position,
) {
    let (start_x, end_x) = background_x_bounds(position, video_bounds, draw_bounds.width);

    // Dark region before video start
    if start_x > 0.0 {
        frame.fill_rectangle(
            iced::Point::ORIGIN,
            iced::Size {
                width: start_x,
                height: frame.height(),
            },
            style::SAMAKU_BACKGROUND,
        );
    }
    // Lighter region within video bounds
    if end_x > start_x {
        frame.fill_rectangle(
            iced::Point { x: start_x, y: 0.0 },
            iced::Size {
                width: end_x - start_x,
                height: frame.height(),
            },
            style::SAMAKU_BACKGROUND_WEAK,
        );
    }
    // Dark region after video end
    if end_x < draw_bounds.width {
        frame.fill_rectangle(
            iced::Point { x: end_x, y: 0.0 },
            iced::Size {
                width: draw_bounds.width - end_x,
                height: frame.height(),
            },
            style::SAMAKU_BACKGROUND,
        );
    }
}

fn draw_seconds_ticks(
    frame: &mut canvas::Frame<Renderer>,
    video_bounds: VideoBounds,
    position: Position,
) {
    let pixel_per_ms = position.pixel_per_ms(frame.width());
    #[expect(
        clippy::cast_possible_truncation,
        reason = "truncation desired in this case"
    )]
    let step = ((1000.0 * 2_f32.powi(-3 - pixel_per_ms.log2().round() as i32)) as i64).max(500);

    for tick_ms in seconds_tick_positions(position, video_bounds, step) {
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
            align_x: iced::widget::text::Alignment::Center,
            ..Default::default()
        });
    }
}

fn draw_frame_ticks(
    frame: &mut canvas::Frame<Renderer>,
    video_bounds: VideoBounds,
    position: Position,
    frame_rate: &FrameRate,
) {
    let pixel_per_ms = position.pixel_per_ms(frame.width());
    let (left_bound, right_bound) = frame_tick_bounds(position, video_bounds);
    let first_frame = media::FrameNumber(
        frame_rate
            .frame_at_time(left_bound, media::TimeMode::Exact)
            .0
            .max(0),
    );

    // Draw frame ticks from left to right.
    for (frame_number, time) in frame_rate.iter_from(first_frame) {
        if time >= right_bound {
            break;
        }

        let tick_x = position.time_delta(time) * pixel_per_ms;

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
            align_x: iced::widget::text::Alignment::Center,
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

const LAYER_HEIGHT: f32 = 24.0;
const SUBTITLE_FULL_HEIGHT: f32 = 18.0;
const SUBTITLE_CENTER_HEIGHT: f32 = 12.0;
const HALF_SQRT_3: f32 = 0.866_025_4;
const TAN_15_DEG: f32 = 0.267_949_2;

fn draw_subtitle_stack(
    frame: &mut canvas::Frame<Renderer>,
    position: Position,
    events: &[EventReference],
    view_state: &mut ViewState,
) {
    view_state.subtitle_areas.clear();

    #[expect(clippy::cast_sign_loss, reason = "clamped to zero")]
    #[expect(clippy::cast_possible_truncation, reason = "rounded")]
    let num_layers = (frame.height() / LAYER_HEIGHT - 1.0).floor().max(0.0) as usize;

    let mut layers: Vec<Vec<&EventReference>> = Vec::with_capacity(num_layers);
    layers.push(vec![]);

    'outer: for event in events {
        let mut layer_index = 0;
        let mut layer = &mut layers[layer_index];

        while layer.iter().any(|other_event| other_event.overlaps(event)) {
            layer_index += 1;
            if layer_index >= num_layers {
                // Skip drawing this subtitle if we don't have a layer available
                continue 'outer;
            }
            if layer_index >= layers.len() {
                layers.push(vec![]);
            }
            layer = &mut layers[layer_index];
        }

        layer.push(event);
        draw_one_subtitle(frame, position, event, view_state, layer_index);
    }
}

fn draw_one_subtitle(
    frame: &mut canvas::Frame<Renderer>,
    position: Position,
    event: &EventReference,
    view_state: &mut ViewState,
    layer_index: usize,
) {
    let pixel_per_ms = position.pixel_per_ms(frame.width());
    #[expect(
        clippy::cast_precision_loss,
        reason = "acceptable for timeline precision"
    )]
    let true_width = event.duration.0 as f32 * pixel_per_ms;
    let left_x = position.time_delta(event.start) * pixel_per_ms;
    let pt_x = (SUBTITLE_FULL_HEIGHT - SUBTITLE_CENTER_HEIGHT) / (2.0 * TAN_15_DEG);
    #[expect(
        clippy::cast_precision_loss,
        reason = "acceptable for timeline precision"
    )]
    let layer_start_y = (layer_index + 1) as f32 * LAYER_HEIGHT;
    let base_point = iced::Point::new(
        left_x,
        layer_start_y + (LAYER_HEIGHT - SUBTITLE_FULL_HEIGHT) / 2.0,
    );

    let path = if true_width > 2.0 * pt_x {
        // If we have enough space, draw a full “dumbbell” with a connecting rectangle in the middle
        draw_subtitle_wide(event, view_state, true_width, pt_x, base_point)
    } else {
        // If there's not enough space, draw a “squeezed dumbbell”, like a spindle
        draw_subtitle_squeezed(event, view_state, true_width, base_point)
    };

    frame.fill(&path, style::SAMAKU_BACKGROUND_WEAK);

    let stroke_style = if event.selected {
        style::SAMAKU_PRIMARY
    } else {
        style::SAMAKU_TEXT_WEAK
    };
    frame.stroke(
        &path,
        canvas::Stroke {
            style: canvas::Style::Solid(stroke_style),
            width: 1.0,
            ..Default::default()
        },
    );
}

fn draw_subtitle_wide(
    event: &EventReference,
    view_state: &mut ViewState,
    true_width: f32,
    pt_x: f32,
    base_point: iced::Point,
) -> canvas::Path {
    // If we have enough space, draw a full “dumbbell” with a connecting rectangle in the middle
    let pt_y = (SUBTITLE_FULL_HEIGHT - SUBTITLE_CENTER_HEIGHT) / 2.0;

    let triangle_width = SUBTITLE_FULL_HEIGHT * HALF_SQRT_3;
    let triangle_size = iced::Size::new(triangle_width, SUBTITLE_FULL_HEIGHT);
    view_state.subtitle_areas.push((
        iced::Rectangle::new(base_point, triangle_size),
        EventDragAction::Left,
        event.clone(),
    ));
    view_state.subtitle_areas.push((
        iced::Rectangle::new(
            base_point + iced::Vector::new(triangle_width, 0.0),
            iced::Size::new(
                2.0_f32.mul_add(-triangle_width, true_width),
                SUBTITLE_FULL_HEIGHT,
            ),
        ),
        EventDragAction::Center,
        event.clone(),
    ));
    view_state.subtitle_areas.push((
        iced::Rectangle::new(
            base_point + iced::Vector::new(true_width - triangle_width, 0.0),
            triangle_size,
        ),
        EventDragAction::Right,
        event.clone(),
    ));

    canvas::Path::new(|builder| {
        builder.move_to(base_point);
        builder.line_to(base_point + iced::Vector::new(pt_x, pt_y));
        builder.line_to(base_point + iced::Vector::new(true_width - pt_x, pt_y));
        builder.line_to(base_point + iced::Vector::new(true_width, 0.0));
        builder.line_to(base_point + iced::Vector::new(true_width, SUBTITLE_FULL_HEIGHT));
        builder.line_to(
            base_point + iced::Vector::new(true_width - pt_x, SUBTITLE_FULL_HEIGHT - pt_y),
        );
        builder.line_to(base_point + iced::Vector::new(pt_x, SUBTITLE_FULL_HEIGHT - pt_y));
        builder.line_to(base_point + iced::Vector::new(0.0, SUBTITLE_FULL_HEIGHT));
        builder.close();
    })
}

fn draw_subtitle_squeezed(
    event: &EventReference,
    view_state: &mut ViewState,
    true_width: f32,
    base_point: iced::Point,
) -> canvas::Path {
    // If there's not enough space, draw a “squeezed dumbbell”, like a spindle
    let squeezed_pt_x = true_width / 2.0;
    let pt_y = squeezed_pt_x * TAN_15_DEG;

    let triangle_size = iced::Size::new(squeezed_pt_x, SUBTITLE_FULL_HEIGHT);
    view_state.subtitle_areas.push((
        iced::Rectangle::new(base_point, triangle_size),
        EventDragAction::Left,
        event.clone(),
    ));
    view_state.subtitle_areas.push((
        iced::Rectangle::new(
            base_point + iced::Vector::new(squeezed_pt_x, 0.0),
            triangle_size,
        ),
        EventDragAction::Right,
        event.clone(),
    ));

    canvas::Path::new(|builder| {
        builder.move_to(base_point);
        builder.line_to(base_point + iced::Vector::new(squeezed_pt_x, pt_y));
        builder.line_to(base_point + iced::Vector::new(true_width, 0.0));
        builder.line_to(base_point + iced::Vector::new(true_width, SUBTITLE_FULL_HEIGHT));
        builder.line_to(base_point + iced::Vector::new(squeezed_pt_x, SUBTITLE_FULL_HEIGHT - pt_y));
        builder.line_to(base_point + iced::Vector::new(0.0, SUBTITLE_FULL_HEIGHT));
        builder.close();
    })
}

fn top_bar<'a>(
    _pane_state: &'a State,
    global_state: &'a crate::Samaku,
) -> iced::Element<'a, message::Message> {
    let (play_button_raw, play_text) = if global_state.playing {
        (view::Icon::Pause.button(), "Pause")
    } else {
        (view::Icon::Play.button(), "Play")
    };
    let (play_button_active, play_tooltip_text) = if global_state.shared.has_audio() {
        (
            play_button_raw.on_press(message::Message::TogglePlayback),
            play_text,
        )
    } else {
        (play_button_raw, "Play (requires loaded audio)")
    };
    let play_button = view::tooltip(play_button_active, play_tooltip_text);

    let frame_number_text_widget = iced::widget::text(pane::video::frame_number_text(global_state));

    iced::widget::container(
        iced::widget::row![play_button, frame_number_text_widget,]
            .spacing(5.0)
            .align_y(iced::Alignment::Center),
    )
    .padding(5.0)
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_float_eq::assert_float_absolute_eq;

    fn make_position(left_ms: i64, right_ms: i64) -> Position {
        Position {
            left: subtitle::StartTime(left_ms),
            right: subtitle::StartTime(right_ms),
        }
    }

    fn make_bounds(start_ms: i64, end_ms: i64) -> VideoBounds {
        VideoBounds {
            start: subtitle::StartTime(start_ms),
            end: subtitle::StartTime(end_ms),
        }
    }

    fn tick_values(ticks: Vec<subtitle::StartTime>) -> Vec<i64> {
        ticks.into_iter().map(|tick_time| tick_time.0).collect()
    }

    #[test]
    fn seconds_ticks() {
        // Video [0, 5000), viewport [0, 10000], step 1000 → ticks at 0..=4000
        let pos = make_position(0, 10000);
        let bounds = make_bounds(0, 5000);
        let mut ticks = tick_values(seconds_tick_positions(pos, bounds, 1000));
        ticks.sort_unstable();
        assert_eq!(ticks, vec![0, 1000, 2000, 3000, 4000]);

        // Video [2000, 7000), viewport [0, 10000], step 1000 → ticks at 2000..=6000
        let pos = make_position(0, 10000);
        let bounds = make_bounds(2000, 7000);
        let mut ticks = tick_values(seconds_tick_positions(pos, bounds, 1000));
        ticks.sort_unstable();
        assert_eq!(ticks, vec![2000, 3000, 4000, 5000, 6000]);

        // Video [0, 3000) — end falls exactly on a tick boundary, must NOT appear
        let pos = make_position(0, 10000);
        let bounds = make_bounds(0, 3000);
        let mut ticks = tick_values(seconds_tick_positions(pos, bounds, 1000));
        ticks.sort_unstable();
        assert_eq!(ticks, vec![0, 1000, 2000]);

        // Empty video: start == end → no ticks
        let pos = make_position(0, 10000);
        let bounds = make_bounds(0, 0);
        assert!(seconds_tick_positions(pos, bounds, 1000).is_empty());

        // Empty video at a non-zero position
        let pos = make_position(0, 10000);
        let bounds = make_bounds(5000, 5000);
        assert!(seconds_tick_positions(pos, bounds, 1000).is_empty());

        // Video entirely before the viewport → no ticks visible
        let pos = make_position(8000, 10000);
        let bounds = make_bounds(0, 5000);
        assert!(seconds_tick_positions(pos, bounds, 1000).is_empty());

        // Video entirely after the viewport → no ticks visible
        let pos = make_position(0, 3000);
        let bounds = make_bounds(5000, 10000);
        assert!(seconds_tick_positions(pos, bounds, 1000).is_empty());

        // Video [1500, 4500), step 1000 → only aligned multiples within bounds: 2000, 3000, 4000
        let pos = make_position(0, 10000);
        let bounds = make_bounds(1500, 4500);
        let mut ticks = tick_values(seconds_tick_positions(pos, bounds, 1000));
        ticks.sort_unstable();
        assert_eq!(ticks, vec![2000, 3000, 4000]);

        // Viewport [2000, 4000], video [0, 10000) → only ticks in viewport
        let pos = make_position(2000, 4000);
        let bounds = make_bounds(0, 10000);
        let mut ticks = tick_values(seconds_tick_positions(pos, bounds, 1000));
        ticks.sort_unstable();
        assert_eq!(ticks, vec![2000, 3000, 4000]);
    }
    #[test]
    fn frame_bounds() {
        // Viewport fully inside video → bounds match viewport
        let pos = make_position(3000, 5000);
        let bounds = make_bounds(2000, 7000);
        assert_eq!(
            frame_tick_bounds(pos, bounds),
            (subtitle::StartTime(3000), subtitle::StartTime(5000))
        );

        // Viewport encompasses video → bounds match video
        let pos = make_position(0, 10000);
        let bounds = make_bounds(2000, 7000);
        assert_eq!(
            frame_tick_bounds(pos, bounds),
            (subtitle::StartTime(2000), subtitle::StartTime(7000))
        );

        // Video entirely to the right → left_bound > right_bound, so no frames drawn
        let pos = make_position(0, 1000);
        let bounds = make_bounds(2000, 7000);
        let (left, right) = frame_tick_bounds(pos, bounds);
        assert!(left >= right);

        // Video entirely to the left → left_bound > right_bound, so no frames drawn
        let pos = make_position(8000, 10000);
        let bounds = make_bounds(2000, 7000);
        let (left, right) = frame_tick_bounds(pos, bounds);
        assert!(left >= right);

        // Empty video → left_bound == right_bound, no frames drawn
        let pos = make_position(0, 10000);
        let bounds = make_bounds(5000, 5000);
        let (left, right) = frame_tick_bounds(pos, bounds);
        assert_eq!(left, right);
    }

    #[test]
    fn background_bounds() {
        // Video exactly matches viewport → start_x=0, end_x=width
        let pos = make_position(0, 1000);
        let bounds = make_bounds(0, 1000);
        let (start_x, end_x) = background_x_bounds(pos, bounds, 100.0);
        assert_float_absolute_eq!(start_x, 0.0, 0.001);
        assert_float_absolute_eq!(end_x, 100.0, 0.001);

        // Video entirely right of viewport → start_x == end_x == width (all dark)
        let pos = make_position(0, 1000);
        let bounds = make_bounds(2000, 5000);
        let (start_x, end_x) = background_x_bounds(pos, bounds, 100.0);
        assert_float_absolute_eq!(start_x, 100.0, 0.001);
        assert_float_absolute_eq!(end_x, 100.0, 0.001);

        // Video entirely left of viewport → start_x == end_x == 0 (all dark)
        let pos = make_position(5000, 6000);
        let bounds = make_bounds(0, 3000);
        let (start_x, end_x) = background_x_bounds(pos, bounds, 100.0);
        assert_float_absolute_eq!(start_x, 0.0, 0.001);
        assert_float_absolute_eq!(end_x, 0.0, 0.001);

        // Viewport [0, 1000], video [250, 750] → start_x=25%, end_x=75%
        let pos = make_position(0, 1000);
        let bounds = make_bounds(250, 750);
        let (start_x, end_x) = background_x_bounds(pos, bounds, 100.0);
        assert_float_absolute_eq!(start_x, 25.0, 0.001);
        assert_float_absolute_eq!(end_x, 75.0, 0.001);

        // Empty video → start_x == end_x (zero-width lighter region)
        let pos = make_position(0, 1000);
        let bounds = make_bounds(500, 500);
        let (start_x, end_x) = background_x_bounds(pos, bounds, 100.0);
        assert_float_absolute_eq!(start_x, end_x, 0.001);
    }
}
