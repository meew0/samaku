use crate::{media, message, subtitle};

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

pub fn view<'a>(global_state: &'a crate::Samaku, _video_state: &'a State) -> super::PaneView<'a> {
    let scroll = match &global_state.actual_frame {
        None => empty!(),
        Some((num_frame, handle)) => match &global_state.video_metadata {
            None => empty!(),
            Some(video_metadata) => {
                if global_state.subtitles.is_empty() {
                    iced::widget::scrollable(iced::widget::image(handle.clone()))
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
                            ass,
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

                    iced::widget::scrollable(crate::view::widget::ImageStack::new(stack))
                }
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
