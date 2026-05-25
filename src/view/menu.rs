//! Utilities for creating menus.
//! Adapted from <https://github.com/iced-rs/iced_aw/blob/main/examples/menu/src/main.rs>.
use iced::widget::{button, row, text};
use iced::{Color, Element, Length, alignment};
use iced_aw::menu::{Item, Menu};

use crate::message::Message;

fn button_style(class: &iced::Theme, status: button::Status) -> button::Style {
    let palette = class.extended_palette();

    let active_style = button::Style {
        text_color: palette.background.base.text,
        border: iced::border::rounded(iced::border::radius(4.0)),
        background: Some(Color::TRANSPARENT.into()),
        ..Default::default()
    };

    let hovered_style = button::Style {
        background: Some(palette.primary.weak.color.into()),
        text_color: palette.primary.weak.text,
        ..active_style
    };

    let pressed_style = button::Style {
        background: Some(palette.background.weak.color.into()),
        ..active_style
    };

    let disabled_style = button::Style {
        text_color: palette.background.base.text.scale_alpha(0.5),
        ..active_style
    };

    match status {
        button::Status::Active => active_style,
        button::Status::Hovered => hovered_style,
        button::Status::Pressed => pressed_style,
        button::Status::Disabled => disabled_style,
    }
}

pub fn base_button<'a, E: Into<Element<'a, Message, iced::Theme, iced::Renderer>>>(
    content: E,
    msg: Option<Message>,
) -> button::Button<'a, Message, iced::Theme, iced::Renderer> {
    button(content)
        .padding([4, 8])
        .style(button_style)
        .on_press_maybe(msg)
}

#[must_use]
pub fn labeled_button(
    label: &str,
    msg: Option<Message>,
) -> button::Button<'_, Message, iced::Theme, iced::Renderer> {
    base_button(
        text(label)
            .width(Length::Fill)
            .align_y(alignment::Vertical::Center),
        msg,
    )
}

pub fn item(label: &'_ str, msg: Message) -> Item<'_, Message, iced::Theme, iced::Renderer> {
    Item::new(labeled_button(label, Some(msg)).width(Length::Fill))
}

// An item that can contain a non-static label, and can be disabled.
pub fn intricate_item<'a, S: Into<String>>(
    label: S,
    msg: Option<Message>,
) -> Item<'a, Message, iced::Theme, iced::Renderer> {
    let string: String = label.into();
    let button = base_button(
        text(string)
            .width(Length::Fill)
            .align_y(alignment::Vertical::Center),
        msg,
    );
    Item::new(button.width(Length::Fill))
}

#[expect(
    clippy::module_name_repetitions,
    reason = "this is clearly the best name in this case"
)]
pub fn sub_menu<'a>(
    label: &'a str,
    msg: Message,
    children: Vec<Item<'a, Message, iced::Theme, iced::Renderer>>,
) -> Item<'a, Message, iced::Theme, iced::Renderer> {
    let arrow = super::icon(super::icons::CARET_RIGHT_FILL).size(10.0);

    Item::with_menu(
        base_button(
            row![text(label).width(Length::Fill), arrow.width(Length::Shrink)]
                .align_y(iced::Alignment::Center),
            Some(msg),
        )
        .width(Length::Fill),
        Menu::new(children).width(150.0),
    )
}
