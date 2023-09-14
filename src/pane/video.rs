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
                    let compiled =
                        global_state
                            .subtitles
                            .compile(0, 1000000, video_metadata.frame_rate); // TODO give actual frame range values here
                    let ass = media::subtitle::OpaqueTrack::from_events_and_styles(
                        &compiled,
                        &global_state.subtitles.styles,
                    );
                    println!(
                        "# events: compiled {}, ass {}; # styles: ass {}",
                        compiled.len(),
                        ass.num_events(),
                        ass.num_styles(),
                    );
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
                    iced::widget::scrollable(crate::view::widget::ImageStack::new(stack))
                }
            }
        },
    };

    super::PaneView {
        title: iced::widget::text("Video pane").into(),
        content: iced::widget::container(scroll)
            .width(iced::Length::Fill)
            .height(iced::Length::Fill)
            .center_x()
            .center_y()
            .into(),
    }
}

pub fn update(_video_state: &mut State, pane_message: message::PaneMessage) {
    match pane_message {}
}
