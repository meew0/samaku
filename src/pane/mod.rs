pub use iced::widget::pane_grid::Pane;

use crate::message;

pub mod grid;
pub mod node_editor;
pub mod text_editor;
pub mod unassigned;
pub mod video;

/// The state information contained by a pane: what type of pane it is, as well as any
/// extra data that is specific to the pane itself, like the state of control elements.
#[derive(Debug, Clone)]
pub enum State {
    Unassigned,
    Video(video::State),
    Grid(grid::State),
    TextEditor(text_editor::State),
    NodeEditor(node_editor::State),
}

pub struct View<'a> {
    pub title: iced::Element<'a, message::Message>,
    pub content: iced::Element<'a, message::Message>,
}

pub(crate) fn dispatch_view<'a>(
    self_pane: Pane,
    global_state: &'a crate::Samaku,
    state: &'a State,
) -> View<'a> {
    match state {
        State::Unassigned => unassigned::view(self_pane),
        State::Video(local_state) => video::view(self_pane, global_state, local_state),
        State::Grid(local_state) => grid::view(self_pane, global_state, local_state),
        State::TextEditor(local_state) => text_editor::view(self_pane, global_state, local_state),
        State::NodeEditor(local_state) => node_editor::view(self_pane, global_state, local_state),
    }
}

pub fn dispatch_update(
    state: &mut State,
    pane_message: message::Pane,
) -> iced::Command<message::Message> {
    match state {
        State::Unassigned => iced::Command::none(),
        State::Video(local_state) => video::update(local_state, pane_message),
        State::Grid(local_state) => grid::update(local_state, pane_message),
        State::TextEditor(local_state) => text_editor::update(local_state, pane_message),
        State::NodeEditor(local_state) => node_editor::update(local_state, pane_message),
    }
}
