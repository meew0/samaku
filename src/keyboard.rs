use iced::{
    keyboard::{Key, Location, Modifiers, key::Named},
    widget::pane_grid::Axis,
};

use crate::model;
use crate::{message::Message, pane};

pub(crate) fn handle_key_press(
    key: &Key,
    modifiers: Modifiers,
    _location: Location,
) -> Option<Message> {
    match key.as_ref() {
        Key::Named(Named::F2) => Some(Message::SplitPane(Axis::Vertical)),
        Key::Named(Named::F3) => Some(Message::SplitPane(Axis::Horizontal)),
        Key::Named(Named::F4) => {
            if modifiers.shift() {
                Some(Message::SetFocusedPaneType(|| {
                    Box::new(pane::unassigned::State {})
                }))
            } else {
                Some(Message::ClosePane)
            }
        }
        Key::Character("v") => Some(Message::SelectVideoFile),
        Key::Character("b") => Some(Message::ImportSubtitleFile),
        Key::Character("n") => Some(Message::SelectAudioFile),
        Key::Character("o") => Some(Message::OpenSubtitleFile),
        Key::Character("s") => Some(Message::SaveSubtitleFile),
        Key::Character(",") => Some(Message::PlaybackAdvanceFrames(model::FrameDelta(-1))),
        Key::Character(".") => Some(Message::PlaybackAdvanceFrames(model::FrameDelta(1))),
        Key::Named(Named::ArrowLeft) => Some(Message::PlaybackAdvanceSeconds(-1.0)),
        Key::Named(Named::ArrowRight) => Some(Message::PlaybackAdvanceSeconds(1.0)),
        Key::Named(Named::Space) => Some(Message::TogglePlayback),
        Key::Character("+") => Some(Message::AddEvent),
        _ => None,
    }
}
