use iced::{
    keyboard::{KeyCode, Modifiers},
    widget::pane_grid::Axis,
};

use crate::model;
use crate::{message::Message, pane};

pub(crate) fn handle_key_press(modifiers: Modifiers, key_code: KeyCode) -> Option<Message> {
    match key_code {
        KeyCode::F2 => Some(Message::SplitPane(Axis::Vertical)),
        KeyCode::F3 => Some(Message::SplitPane(Axis::Horizontal)),
        KeyCode::F4 => {
            if modifiers.shift() {
                Some(Message::SetFocusedPaneState(Box::new(
                    pane::State::Unassigned,
                )))
            } else {
                Some(Message::ClosePane)
            }
        }
        KeyCode::V => Some(Message::SelectVideoFile),
        KeyCode::B => Some(Message::ImportSubtitleFile),
        KeyCode::N => Some(Message::SelectAudioFile),
        KeyCode::O => Some(Message::OpenSubtitleFile),
        KeyCode::S => Some(Message::SaveSubtitleFile),
        KeyCode::Comma => Some(Message::PlaybackAdvanceFrames(model::FrameDelta(-1))),
        KeyCode::Period => Some(Message::PlaybackAdvanceFrames(model::FrameDelta(1))),
        KeyCode::Left => Some(Message::PlaybackAdvanceSeconds(-1.0)),
        KeyCode::Right => Some(Message::PlaybackAdvanceSeconds(1.0)),
        KeyCode::Space => Some(Message::TogglePlayback),
        KeyCode::Plus => Some(Message::AddEvent),
        _ => None,
    }
}
