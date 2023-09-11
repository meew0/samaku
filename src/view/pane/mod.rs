use crate::{message::Message, model};

mod video;

pub struct PaneView<'a> {
    pub title: iced::Element<'a, Message>,
    pub content: iced::Element<'a, Message>,
}

pub fn dispatch_view<'a>(
    global_state: &'a model::GlobalState,
    state: &'a model::pane::PaneState,
) -> PaneView<'a> {
    match state {
        model::pane::PaneState::Unassigned => PaneView {
            title: iced::widget::text("Unassigned pane").into(),
            content: iced::widget::container(iced::widget::scrollable(iced::widget::text(
                format!("Unassigned pane"),
            )))
            .width(iced::Length::Fill)
            .height(iced::Length::Fill)
            .center_y()
            .into(),
        },
        model::pane::PaneState::Video(video_state) => video::view(global_state, video_state),
    }
}
