//! samaku's global menus.

use iced_aw::menu::{Item, Menu};

use crate::{history, message, project, view};

pub fn file<'a>() -> Item<'a, message::Message, iced::Theme, iced::Renderer> {
    Item::with_menu(
        iced::widget::button("File")
            .on_press(message::Message::None)
            .width(iced::Length::Shrink),
        Menu::new(vec![
            view::menu::item("New", message::Message::NewSubtitleFile),
            view::menu::item("Open", message::Message::OpenProject),
            view::menu::item("Import", message::Message::ImportSubtitleFile),
            view::menu::item(
                "Save",
                message::Message::SaveProject(project::SaveMode::Over),
            ),
            view::menu::item(
                "Save as",
                message::Message::SaveProject(project::SaveMode::SaveAs),
            ),
            view::menu::item("Export", message::Message::ExportSubtitleFile),
        ])
        .width(iced::Length::Fixed(150.0)),
    )
}

pub fn edit<'a>(
    history: &history::History,
) -> Item<'a, message::Message, iced::Theme, iced::Renderer> {
    let undo = undo_redo_item("Undo", history.peek_undo(), message::Message::Undo);
    let redo = undo_redo_item("Redo", history.peek_redo(), message::Message::Redo);

    Item::with_menu(
        iced::widget::button("Edit")
            .on_press(message::Message::None)
            .width(iced::Length::Shrink),
        Menu::new(vec![undo, redo]).width(iced::Length::Fixed(150.0)),
    )
}

fn undo_redo_item<'a>(
    name: &'static str,
    content_option: Option<&'static str>,
    message: message::Message,
) -> Item<'a, message::Message, iced::Theme, iced::Renderer> {
    match content_option {
        Some(content) => {
            if content.is_empty() {
                view::menu::intricate_item(name, Some(message))
            } else {
                view::menu::intricate_item(format!("{name}: {content}"), Some(message))
            }
        }
        None => view::menu::intricate_item(name, None),
    }
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
