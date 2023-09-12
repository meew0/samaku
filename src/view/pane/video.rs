use crate::{
    message::{Message, PaneMessage},
    model,
};

pub fn view<'a>(
    global_state: &'a model::GlobalState,
    video_state: &'a model::pane::video::State,
) -> super::PaneView<'a> {
    let scroll = match &global_state.video {
        None => iced::widget::scrollable(iced::widget::row![
            iced::widget::button("Increment counter")
                .on_press(Message::Dispatch(PaneMessage::VideoIncrementCounter)),
            iced::widget::text(format!("Count: {}", video_state.counter)),
        ]),
        Some(video) => match &global_state.subtitles {
            Some(subtitles) => {
                let current_frame = global_state.playback_state.current_frame(video.frame_rate);
                let base = video.get_frame(current_frame);
                let stack = subtitles.render_onto(base, current_frame, video.frame_rate);
                iced::widget::scrollable(crate::view::widget::ImageStack::new(stack))
            }
            None => iced::widget::scrollable(iced::widget::image(
                video.get_frame(global_state.playback_state.current_frame(video.frame_rate)),
            )),
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
