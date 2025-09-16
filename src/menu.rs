//! samaku's global menus

use iced_aw::menu::{Item, Menu};

use crate::{message, view};

pub fn file<'a>() -> Item<'a, message::Message, iced::Theme, iced::Renderer> {
    Item::with_menu(
        iced::widget::button("File")
            .on_press(message::Message::None)
            .width(iced::Length::Shrink),
        Menu::new(vec![
            view::menu::item("Open", message::Message::OpenSubtitleFile),
            view::menu::item("Import", message::Message::ImportSubtitleFile),
            view::menu::item("Save", message::Message::SaveSubtitleFile),
            view::menu::item("Export", message::Message::ExportSubtitleFile),
        ])
        .width(iced::Length::Fixed(150.0)),
    )
}

pub fn media<'a>() -> Item<'a, message::Message, iced::Theme, iced::Renderer> {
    Item::with_menu(
        iced::widget::button("Media")
            .on_press(message::Message::None)
            .width(iced::Length::Shrink),
        Menu::new(vec![
            view::menu::item("Load video", message::Message::SelectVideoFile),
            view::menu::item("Load audio", message::Message::SelectAudioFile),
        ])
        .width(iced::Length::Fixed(150.0)),
    )
}
