use crate::{
    controller::global,
    message::{Message, PaneMessage},
    model,
};

macro_rules! empty {
    () => {
        iced::widget::scrollable(iced::widget::row![iced::widget::text(
            "No video loaded. Press V to load something."
        ),])
    };
}

pub fn view<'a>(
    global_state: &'a model::GlobalState,
    video_state: &'a model::pane::video::State,
) -> super::PaneView<'a> {
    let scroll = match &video_state.image_handle {
        None => empty!(),
        Some(handle) => match &global_state.video_metadata {
            None => empty!(),
            Some(video_metadata) => match &global_state.subtitles {
                Some(subtitles) => {
                    let stack = subtitles.render_onto(
                        handle.clone(),
                        video_state.actual_frame,
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
            .center_y()
            .into(),
    }
}
