use iced::mouse;
use iced::widget::canvas;

use crate::{media, message, model, style, subtitle, view};

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct State;

// Elements to display if no video is loaded
macro_rules! empty {
    () => {
        iced::widget::scrollable(iced::widget::row![iced::widget::text(
            "No video loaded. Press V to load something."
        ),])
    };
}

#[typetag::serde(name = "video")]
impl super::LocalState for State {
    fn view<'a>(
        &'a self,
        _self_pane: super::Pane,
        global_state: &'a crate::Samaku,
    ) -> super::View<'a> {
        let scroll = match &global_state.actual_frame {
            None => empty!(),
            Some((num_frame, handle)) => match &global_state.video_metadata {
                None => empty!(),
                Some(video_metadata) => {
                    let storage_size = subtitle::Resolution {
                        x: video_metadata.width,
                        y: video_metadata.height,
                    };

                    let stack = if global_state.subtitles.events.is_empty() {
                        vec![view::widget::StackedImage {
                            handle: handle.clone(),
                            x: 0,
                            y: 0,
                        }]
                    } else {
                        let instant = std::time::Instant::now();
                        let context = global_state.compile_context();
                        let current_frame_time = video_metadata.frame_rate.frame_to_ms(*num_frame);
                        let compiled = global_state.subtitles.events.compile_range(
                            &global_state.subtitles.extradata,
                            &context,
                            subtitle::StartTime(current_frame_time).stab(),
                        );
                        let elapsed_compile = instant.elapsed();

                        let instant2 = std::time::Instant::now();
                        let ass = media::subtitle::OpaqueTrack::from_compiled(
                            compiled.iter(),
                            global_state.subtitles.styles.as_slice(),
                            &global_state.subtitles.script_info,
                        );
                        let elapsed_copy = instant2.elapsed();

                        let instant3 = std::time::Instant::now();
                        let stack = {
                            let mut view_state = global_state.view.borrow_mut();
                            view_state.subtitle_renderer.render_subtitles_onto_base(
                                &ass,
                                handle.clone(),
                                *num_frame,
                                video_metadata.frame_rate,
                                storage_size, // TODO use the actual frame size here (maybe with responsive?)
                                storage_size,
                            )
                        };
                        let elapsed_render = instant3.elapsed();
                        println!(
                            "Subtitle profiling: compiling {} source events to {} compiled events took {:.2?}, copying them into libass took {:.2?}, rendering them took {:.2?}",
                            global_state.subtitles.events.len(),
                            compiled.len(),
                            elapsed_compile,
                            elapsed_copy,
                            elapsed_render
                        );

                        stack
                    };

                    let reticules: &[model::reticule::Reticule] =
                        if let Some(reticules) = &global_state.reticules {
                            reticules.list.as_slice()
                        } else {
                            &[]
                        };

                    let program = ReticuleProgram {
                        reticules,
                        storage_size,
                    };
                    iced::widget::scrollable(view::widget::ImageStack::new(stack, program))
                }
            },
        };

        super::View {
            title: iced::widget::text("Video").into(),
            content: iced::widget::container(scroll)
                .center_x(iced::Length::Fill)
                .center_y(iced::Length::Fill)
                .into(),
        }
    }
}

inventory::submit! {
    super::Shell::new(
        "Video",
        || Box::new(State)
    )
}

struct ReticuleProgram<'a> {
    reticules: &'a [model::reticule::Reticule],
    storage_size: subtitle::Resolution,
}

#[derive(Default)]
struct ReticuleState {
    dragging: Option<usize>,
    drag_offset: iced::Vector,
}

impl ReticuleProgram<'_> {
    fn find_hovered_reticule(
        &self,
        mouse_position: iced::Point,
        bounds: iced::Rectangle,
    ) -> Option<(usize, &model::reticule::Reticule, iced::Point)> {
        for (i, reticule) in self.reticules.iter().enumerate().rev() {
            let iced_pos = reticule.iced_position(bounds.size(), self.storage_size);
            if iced_pos.distance(mouse_position) < reticule.radius {
                return Some((i, reticule, iced_pos));
            }
        }

        None
    }
}

impl canvas::Program<message::Message> for ReticuleProgram<'_> {
    type State = ReticuleState;

    fn update(
        &self,
        state: &mut Self::State,
        event: canvas::Event,
        bounds: iced::Rectangle,
        cursor: mouse::Cursor,
    ) -> (iced::event::Status, Option<message::Message>) {
        if let Some(position) = cursor.position_in(bounds)
            && let canvas::Event::Mouse(mouse_event) = event
        {
            match mouse_event {
                mouse::Event::ButtonPressed(mouse::Button::Left) => {
                    if let Some((i, _reticule, iced_pos)) =
                        self.find_hovered_reticule(position, bounds)
                    {
                        state.dragging = Some(i);
                        state.drag_offset = position - iced_pos;
                        return (iced::event::Status::Captured, None);
                    }
                }
                mouse::Event::CursorMoved { .. } => {
                    if let Some(dragging_reticule_index) = state.dragging {
                        return (
                            iced::event::Status::Captured,
                            Some(message::Message::UpdateReticulePosition(
                                dragging_reticule_index,
                                model::reticule::Reticule::position_from_iced(
                                    position,
                                    state.drag_offset,
                                    bounds.size(),
                                    self.storage_size,
                                ),
                            )),
                        );
                    }
                }
                mouse::Event::ButtonReleased(mouse::Button::Left) => {
                    if state.dragging.is_some() {
                        state.dragging = None;
                        return (iced::event::Status::Captured, None);
                    }
                }
                _ => {}
            }
        }

        (iced::event::Status::Ignored, None)
    }

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &iced::Renderer,
        _theme: &iced::Theme,
        bounds: iced::Rectangle,
        cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());

        let hovered_reticule_index = cursor
            .position_in(bounds)
            .and_then(|mouse_position| self.find_hovered_reticule(mouse_position, bounds))
            .map(|(i, _, _)| i);

        for (i, reticule) in self.reticules.iter().enumerate() {
            let hovered = hovered_reticule_index.is_some_and(|hovered_index| i == hovered_index);
            let alpha_factor: f32 = if hovered { 0.4 } else { 0.2 };
            let center_point = reticule.iced_position(bounds.size(), self.storage_size);

            match reticule.shape {
                model::reticule::Shape::Cross => {
                    let rect_top_left = center_point
                        - iced::Vector::new(reticule.radius * 0.5, reticule.radius * 0.5);
                    let rect_size = iced::Size::new(reticule.radius, reticule.radius);
                    frame.fill_rectangle(
                        rect_top_left,
                        rect_size,
                        style::SAMAKU_TEXT.scale_alpha(alpha_factor),
                    );

                    frame.stroke_rectangle(
                        rect_top_left,
                        rect_size,
                        canvas::Stroke::default()
                            .with_color(style::SAMAKU_BACKGROUND)
                            .with_width(1.0),
                    );

                    let thin_path = canvas::Path::new(|path| {
                        path.move_to(center_point + iced::Vector::new(-reticule.radius, 0.0));
                        path.line_to(center_point + iced::Vector::new(reticule.radius, 0.0));
                        path.move_to(center_point + iced::Vector::new(0.0, -reticule.radius));
                        path.line_to(center_point + iced::Vector::new(0.0, reticule.radius));
                    });

                    frame.stroke(
                        &thin_path,
                        canvas::Stroke::default()
                            .with_color(style::SAMAKU_BACKGROUND)
                            .with_width(2.0),
                    );

                    frame.stroke(
                        &thin_path,
                        canvas::Stroke::default()
                            .with_color(style::SAMAKU_PRIMARY)
                            .with_width(1.0),
                    );
                }
                model::reticule::Shape::CornerTopLeft => {
                    corner(
                        &mut frame,
                        center_point,
                        reticule.radius,
                        1.0,
                        1.0,
                        alpha_factor,
                    );
                }
                model::reticule::Shape::CornerTopRight => {
                    corner(
                        &mut frame,
                        center_point,
                        reticule.radius,
                        -1.0,
                        1.0,
                        alpha_factor,
                    );
                }
                model::reticule::Shape::CornerBottomLeft => {
                    corner(
                        &mut frame,
                        center_point,
                        reticule.radius,
                        1.0,
                        -1.0,
                        alpha_factor,
                    );
                }
                model::reticule::Shape::CornerBottomRight => {
                    corner(
                        &mut frame,
                        center_point,
                        reticule.radius,
                        -1.0,
                        -1.0,
                        alpha_factor,
                    );
                }
            }
        }

        vec![frame.into_geometry()]
    }

    fn mouse_interaction(
        &self,
        state: &Self::State,
        bounds: iced::Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if state.dragging.is_some() {
            return mouse::Interaction::Grabbing;
        }

        if let Some(mouse_position) = cursor.position_in(bounds)
            && let Some(_) = self.find_hovered_reticule(mouse_position, bounds)
        {
            return mouse::Interaction::Grab;
        }

        mouse::Interaction::None
    }
}

fn corner(
    frame: &mut canvas::Frame,
    center_point: iced::Point,
    radius: f32,
    x_sign: f32,
    y_sign: f32,
    alpha_factor: f32,
) {
    let path = canvas::Path::new(|path| {
        path.move_to(center_point + iced::Vector::new(x_sign * radius, 0.0));
        path.line_to(center_point);
        path.line_to(center_point + iced::Vector::new(0.0, y_sign * radius));
    });

    frame.fill(
        &canvas::Path::circle(center_point, radius * 0.5),
        style::SAMAKU_TEXT.scale_alpha(alpha_factor),
    );

    frame.stroke(
        &canvas::Path::circle(center_point, radius * 0.5),
        canvas::Stroke::default()
            .with_color(style::SAMAKU_BACKGROUND)
            .with_width(1.0),
    );

    frame.stroke(
        &path,
        canvas::Stroke::default()
            .with_color(style::SAMAKU_BACKGROUND)
            .with_width(2.5),
    );

    frame.stroke(
        &path,
        canvas::Stroke::default()
            .with_color(style::SAMAKU_PRIMARY)
            .with_width(1.0),
    );
}
