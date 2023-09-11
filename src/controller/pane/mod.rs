use crate::{
    message::{Message, PaneMessage},
    model,
};

mod video;

pub fn dispatch_update(
    state: &mut model::pane::PaneState,
    pane_message: PaneMessage,
) -> iced::Command<Message> {
    match state {
        model::pane::PaneState::Unassigned => (),
        model::pane::PaneState::Video(video_state) => video::update(video_state, pane_message),
    }

    iced::Command::none()
}
