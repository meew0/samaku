use crate::message;

#[derive(Debug, Clone)]
pub struct State {}

impl Default for State {
    fn default() -> Self {
        Self {}
    }
}

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
            Some(video_metadata) => match &global_state.subtitles {
                Some(subtitles) => {
                    let stack = subtitles.render_onto(
                        handle.clone(),
                        *num_frame,
                        video_metadata.frame_rate,
                    );
                    iced::widget::scrollable(crate::view::widget::ImageStack::new(stack))
                }
                None => iced::widget::scrollable(iced::widget::image(handle.clone())),
            },
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
