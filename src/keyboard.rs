use iced::{
    keyboard::{KeyCode, Modifiers},
    widget::pane_grid::Axis,
};

use crate::message::{GlobalMessage, Message};

pub(crate) fn handle_key_press(_modifiers: Modifiers, key_code: KeyCode) -> Option<Message> {
    match key_code {
        KeyCode::F2 => Some(Message::SplitPane(Axis::Vertical)),
        KeyCode::F3 => Some(Message::SplitPane(Axis::Horizontal)),
        KeyCode::F4 => Some(Message::ClosePane),
        KeyCode::V => Some(Message::Global(GlobalMessage::SelectVideoFile)),
        KeyCode::B => Some(Message::Global(GlobalMessage::SelectSubtitleFile)),
        KeyCode::N => Some(Message::Global(GlobalMessage::SelectAudioFile)),
        KeyCode::Comma => Some(Message::Global(GlobalMessage::PlaybackAdvanceFrames(-1))),
        KeyCode::Period => Some(Message::Global(GlobalMessage::PlaybackAdvanceFrames(1))),
        KeyCode::Left => Some(Message::Global(GlobalMessage::PlaybackAdvanceSeconds(-1.0))),
        KeyCode::Right => Some(Message::Global(GlobalMessage::PlaybackAdvanceSeconds(1.0))),
        KeyCode::Space => Some(Message::Global(GlobalMessage::TogglePlayback)),
        _ => None,
    }
}
