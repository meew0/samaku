use crate::message;

pub mod grid;
pub mod unassigned;
pub mod video;

/// The state information contained by a pane: what type of pane it is, as well as any
/// extra data that is specific to the pane itself, like the state of control elements.
#[derive(Debug, Clone)]
pub enum PaneState {
    Unassigned,
    Video(video::State),
    Grid(grid::State),
}

pub struct PaneView<'a> {
    pub title: iced::Element<'a, message::Message>,
    pub content: iced::Element<'a, message::Message>,
}

pub fn dispatch_view<'a>(
    self_pane: iced::widget::pane_grid::Pane,
    global_state: &'a crate::Samaku,
    state: &'a PaneState,
) -> PaneView<'a> {
    match state {
        PaneState::Unassigned => unassigned::view(self_pane),
        PaneState::Video(video_state) => video::view(global_state, video_state),
        PaneState::Grid(grid_state) => grid::view(global_state, grid_state),
    }
}

pub fn dispatch_update(
    state: &mut PaneState,
    pane_message: message::PaneMessage,
) -> iced::Command<message::Message> {
    match state {
        PaneState::Unassigned => iced::Command::none(),
        PaneState::Video(video_state) => video::update(video_state, pane_message),
        PaneState::Grid(grid_state) => grid::update(grid_state, pane_message),
    }
}