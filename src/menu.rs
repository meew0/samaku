//! samaku's global menus

use iced_aw::menu::MenuTree;

use crate::{message, view};

#[must_use]
pub fn file<'a>() -> MenuTree<'a, message::Message, iced::Renderer> {
    iced_aw::helpers::menu_tree(
        iced::widget::button("File").on_press(message::Message::None),
        vec![
            view::menu::item("Open", message::Message::OpenSubtitleFile),
            view::menu::item("Import", message::Message::ImportSubtitleFile),
            view::menu::item("Save", message::Message::SaveSubtitleFile),
            view::menu::item("Export", message::Message::ExportSubtitleFile),
        ],
    )
}

#[must_use]
pub fn media<'a>() -> MenuTree<'a, message::Message, iced::Renderer> {
    iced_aw::helpers::menu_tree(
        iced::widget::button("Media").on_press(message::Message::None),
        vec![
            view::menu::item("Load video", message::Message::SelectVideoFile),
            view::menu::item("Load audio", message::Message::SelectAudioFile),
        ],
    )
}
