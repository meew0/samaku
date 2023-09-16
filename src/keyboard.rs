use iced::{
    keyboard::{KeyCode, Modifiers},
    widget::pane_grid::Axis,
};

use crate::message::Message;

pub(crate) fn handle_key_press(_modifiers: Modifiers, key_code: KeyCode) -> Option<Message> {
    match key_code {
        KeyCode::F2 => Some(Message::SplitPane(Axis::Vertical)),
        KeyCode::F3 => Some(Message::SplitPane(Axis::Horizontal)),
        KeyCode::F4 => Some(Message::ClosePane),
        KeyCode::V => Some(Message::SelectVideoFile),
        KeyCode::B => Some(Message::SelectSubtitleFile),
        KeyCode::N => Some(Message::SelectAudioFile),
        KeyCode::Comma => Some(Message::PlaybackAdvanceFrames(-1)),
        KeyCode::Period => Some(Message::PlaybackAdvanceFrames(1)),
        KeyCode::Left => Some(Message::PlaybackAdvanceSeconds(-1.0)),
        KeyCode::Right => Some(Message::PlaybackAdvanceSeconds(1.0)),
        KeyCode::Space => Some(Message::TogglePlayback),
        KeyCode::E => Some(Message::NdeExample),
        _ => None,
    }
}
