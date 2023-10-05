use iced::widget::canvas;

use crate::{media, message, subtitle, view};

#[derive(Debug, Clone, Default)]
pub struct State {}

// Elements to display if no video is loaded
macro_rules! empty {
    () => {
        iced::widget::scrollable(iced::widget::row![iced::widget::text(
            "No video loaded. Press V to load something."
        ),])
    };
}

pub fn view<'a>(
    _self_pane: super::Pane,
    global_state: &'a crate::Samaku,
    _video_state: &'a State,
) -> super::PaneView<'a> {
    let scroll = match &global_state.actual_frame {
        None => empty!(),
        Some((num_frame, handle)) => match &global_state.video_metadata {
            None => empty!(),
            Some(video_metadata) => {
                let stack = if global_state.subtitles.is_empty() {
                    vec![view::widget::StackedImage {
                        handle: handle.clone(),
                        x: 0,
                        y: 0,
                    }]
                } else {
                    let instant = std::time::Instant::now();
                    let compiled =
                        global_state
                            .subtitles
                            .compile(0, 1000000, video_metadata.frame_rate); // TODO give actual frame range values here
                    let elapsed_compile = instant.elapsed();

                    let instant2 = std::time::Instant::now();
                    let ass = media::subtitle::OpaqueTrack::from_compiled(
                        &compiled,
                        &global_state.subtitles,
                    );
                    let elapsed_copy = instant2.elapsed();

                    let instant3 = std::time::Instant::now();
                    let storage_size = subtitle::Resolution {
                        x: video_metadata.width,
                        y: video_metadata.height,
                    };
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
                        "Subtitle profiling: compiling {} slines to {} events took {:.2?}, copying them into libass took {:.2?}, rendering them took {:.2?}",
                        global_state.subtitles.slines.len(), compiled.len(), elapsed_compile, elapsed_copy, elapsed_render
                    );

                    stack
                };

                let program = ReticuleProgram {};
                iced::widget::scrollable(view::widget::ImageStack::new(stack, program))
            }
        },
    };

    super::PaneView {
        title: iced::widget::text("Video").into(),
        content: iced::widget::container(scroll)
            .width(iced::Length::Fill)
            .height(iced::Length::Fill)
            .center_x()
            .center_y()
            .into(),
    }
}

pub fn update(
    _video_state: &mut State,
    _pane_message: message::PaneMessage,
) -> iced::Command<message::Message> {
    iced::Command::none()
}

struct ReticuleProgram {}

#[derive(Default)]
struct ReticuleState {
    position: iced::Point,
}

impl canvas::Program<message::Message> for ReticuleProgram {
    type State = ReticuleState;

    fn update(
        &self,
        state: &mut Self::State,
        event: canvas::Event,
        bounds: iced::Rectangle,
        cursor: iced::mouse::Cursor,
    ) -> (iced::event::Status, Option<message::Message>) {
        if let canvas::Event::Mouse(iced::mouse::Event::ButtonPressed(iced::mouse::Button::Left)) =
            event
        {
            if let Some(position) = cursor.position_in(bounds) {
                state.position = position;
                return (iced::event::Status::Captured, None);
            }
        }

        (iced::event::Status::Ignored, None)
    }

    fn draw(
        &self,
        state: &Self::State,
        renderer: &iced::Renderer,
        _theme: &iced::Theme,
        bounds: iced::Rectangle,
        _cursor: iced::mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let circle = canvas::Path::circle(state.position, 20.0);
        frame.fill(&circle, iced::Color::BLACK);
        vec![frame.into_geometry()]
    }
}
