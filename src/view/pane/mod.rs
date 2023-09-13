use crate::{message::Message, model};

mod unassigned;
mod video;

pub struct PaneView<'a> {
    pub title: iced::Element<'a, Message>,
    pub content: iced::Element<'a, Message>,
}

pub fn dispatch_view<'a>(
    self_pane: iced::widget::pane_grid::Pane,
    global_state: &'a model::GlobalState,
    state: &'a model::pane::PaneState,
) -> PaneView<'a> {
    match state {
        model::pane::PaneState::Unassigned => unassigned::view(self_pane),
        model::pane::PaneState::Video(video_state) => video::view(global_state, video_state),
    }
}
