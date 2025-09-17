//! Utilities for creating menus.
//! Adapted from <https://github.com/iced-rs/iced_aw/blob/main/examples/menu/src/main.rs>
use iced::widget::{button, row, svg, text};
use iced::{Color, Element, Length, alignment};
use iced_aw::menu::{Item, Menu};

use crate::message::Message;
use crate::resources;

fn button_style(class: &iced::Theme, status: button::Status) -> button::Style {
    let active_style = button::Style {
        text_color: class.extended_palette().background.base.text,
        border: iced::border::rounded(iced::border::radius(4.0)),
        background: Some(Color::TRANSPARENT.into()),
        ..Default::default()
    };

    // TODO add pressed/disabled styles
    #[expect(clippy::match_same_arms, reason = "extra styles to be added later")]
    match status {
        button::Status::Active => active_style,
        button::Status::Hovered => {
            let plt = class.extended_palette();
            button::Style {
                background: Some(plt.primary.weak.color.into()),
                text_color: plt.primary.weak.text,
                ..active_style
            }
        }
        button::Status::Pressed => active_style,
        button::Status::Disabled => active_style,
    }
}

pub fn base_button<'a, E: Into<Element<'a, Message, iced::Theme, iced::Renderer>>>(
    content: E,
    msg: Message,
) -> button::Button<'a, Message, iced::Theme, iced::Renderer> {
    button(content)
        .padding([4, 8])
        .style(button_style)
        .on_press(msg)
}

#[must_use]
pub fn labeled_button(
    label: &str,
    msg: Message,
) -> button::Button<'_, Message, iced::Theme, iced::Renderer> {
    base_button(
        text(label)
            .width(Length::Fill)
            .align_y(alignment::Vertical::Center),
        msg,
    )
}

pub fn item(label: &'_ str, msg: Message) -> Item<'_, Message, iced::Theme, iced::Renderer> {
    Item::new(labeled_button(label, msg).width(Length::Fill))
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
    let handle = svg::Handle::from_memory(resources::CARET_RIGHT_FILL);
    let arrow = svg(handle)
        .width(Length::Shrink)
        .style(|theme: &iced::Theme, _status| svg::Style {
            color: Some(theme.extended_palette().background.base.text),
        });

    Item::with_menu(
        base_button(
            row![
                text(label)
                    .width(Length::Fill)
                    .align_y(alignment::Vertical::Center),
                arrow
            ]
            .align_y(iced::Alignment::Center),
            msg,
        )
        .width(Length::Fill),
        Menu::new(children).width(150.0),
    )
}
