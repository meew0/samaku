use crate::model;
use crate::{message::Message, pane};
use iced::keyboard::Event;
use iced::{
    keyboard::{Key, Location, Modifiers, key::Named},
    widget::pane_grid::Axis,
};

pub(crate) fn handle_shortcut(
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
        Key::Character("a") => modifiers.control().then_some(Message::SelectAllEvents),
        Key::Character("z") => {
            if modifiers.control() {
                if modifiers.shift() {
                    Some(Message::Redo)
                } else {
                    Some(Message::Undo)
                }
            } else {
                None
            }
        }
        Key::Character("y") => modifiers.control().then_some(Message::Redo),
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

pub(crate) fn handle_modifiers(keyboard_event: &Event) -> Option<Message> {
    match keyboard_event {
        Event::ModifiersChanged(modifiers) => Some(Message::ModifiersChanged(*modifiers)),
        _ => None,
    }
}
