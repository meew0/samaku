//! Utilities for creating menus.
//! Adapted from <https://github.com/iced-rs/iced_aw/blob/main/examples/menu/src/main.rs>
use iced::widget::{button, row, svg, text};
use iced::{alignment, theme, Color, Element, Length};
use iced_aw::menu::{Item, Menu};

use crate::message::Message;
use crate::resources;

struct ButtonStyle;

impl button::StyleSheet for ButtonStyle {
    type Style = iced::Theme;

    fn active(&self, style: &Self::Style) -> button::Appearance {
        button::Appearance {
            text_color: style.extended_palette().background.base.text,
            border: iced::Border::with_radius([4.0; 4]),
            background: Some(Color::TRANSPARENT.into()),
            ..Default::default()
        }
    }

    fn hovered(&self, style: &Self::Style) -> button::Appearance {
        let plt = style.extended_palette();

        button::Appearance {
            background: Some(plt.primary.weak.color.into()),
            text_color: plt.primary.weak.text,
            ..self.active(style)
        }
    }
}

pub fn base_button<'a, E: Into<Element<'a, Message, iced::Theme, iced::Renderer>>>(
    content: E,
    msg: Message,
) -> button::Button<'a, Message, iced::Theme, iced::Renderer> {
    button(content)
        .padding([4, 8])
        .style(theme::Button::Custom(Box::new(ButtonStyle {})))
        .on_press(msg)
}

#[must_use]
pub fn labeled_button<'a>(
    label: &str,
    msg: Message,
) -> button::Button<'a, Message, iced::Theme, iced::Renderer> {
    base_button(
        text(label)
            .width(Length::Fill)
            .vertical_alignment(alignment::Vertical::Center),
        msg,
    )
}

pub fn item(label: &'_ str, msg: Message) -> Item<'_, Message, iced::Theme, iced::Renderer> {
    Item::new(labeled_button(label, msg).width(Length::Fill))
}

#[allow(clippy::module_name_repetitions)]
pub fn sub_menu<'a>(
    label: &str,
    msg: Message,
    children: Vec<Item<'a, Message, iced::Theme, iced::Renderer>>,
) -> Item<'a, Message, iced::Theme, iced::Renderer> {
    let handle = svg::Handle::from_memory(resources::CARET_RIGHT_FILL);
    let arrow = svg(handle)
        .width(Length::Shrink)
        .style(theme::Svg::custom_fn(|theme| svg::Appearance {
            color: Some(theme.extended_palette().background.base.text),
        }));

    Item::with_menu(
        base_button(
            row![
                text(label)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .vertical_alignment(alignment::Vertical::Center),
                arrow
            ]
            .align_items(iced::Alignment::Center),
            msg,
        )
        .width(Length::Fill)
        .height(Length::Fill),
        Menu::new(children),
    )
}
