use iced::widget::pane_grid::Axis;

use crate::message::{GlobalMessage, Message};

pub(crate) fn handle_key_press(
    _modifiers: iced::keyboard::Modifiers,
    key_code: iced::keyboard::KeyCode,
) -> Option<Message> {
    match key_code {
        iced::keyboard::KeyCode::F2 => Some(Message::SplitPane(Axis::Vertical)),
        iced::keyboard::KeyCode::F3 => Some(Message::SplitPane(Axis::Horizontal)),
        iced::keyboard::KeyCode::F4 => Some(Message::ClosePane),
        iced::keyboard::KeyCode::F6 => Some(Message::CyclePaneType),
        iced::keyboard::KeyCode::V => Some(Message::Global(GlobalMessage::LoadVideo)),
        iced::keyboard::KeyCode::B => Some(Message::Global(GlobalMessage::LoadSubtitles)),
        iced::keyboard::KeyCode::Period => Some(Message::Global(GlobalMessage::NextFrame)),
        iced::keyboard::KeyCode::Comma => Some(Message::Global(GlobalMessage::PreviousFrame)),
        _ => None,
    }
}
