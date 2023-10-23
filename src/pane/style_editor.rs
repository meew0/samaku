use std::fmt::Display;

use crate::{message, subtitle};

#[derive(Debug, Clone, Default)]
pub struct State {
    selected_style_index: usize,
}

/// A custom wrapper around `Style` that implements `Display`, `Eq`, and `Hash`,
/// by only referencing the names. We can do this because `StyleList` guarantees that styles have
/// unique names.
#[derive(Clone)]
struct StyleWrapper(subtitle::Style);

static_assertions::assert_eq_size!(StyleWrapper, subtitle::Style);
static_assertions::assert_eq_align!(StyleWrapper, subtitle::Style);

impl Display for StyleWrapper {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}", self.0.name())
    }
}

impl PartialEq for StyleWrapper {
    fn eq(&self, other: &Self) -> bool {
        self.0.name() == other.0.name()
    }
}

impl Eq for StyleWrapper {}

impl std::hash::Hash for StyleWrapper {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.name().hash(state);
    }
}

pub fn view<'a>(
    self_pane: super::Pane,
    global_state: &'a crate::Samaku,
    pane_state: &'a State,
) -> super::View<'a> {
    let styles = global_state.subtitles.styles.as_slice();

    // We know that this conversion is sound, since we statically assert that `StyleWrapper` and
    // `Style` have the same size and alignment. However, Rust does not allow us to do this safely,
    // so we have to use unsafe.
    let wrapped_styles: &[StyleWrapper] =
        unsafe { std::slice::from_raw_parts(styles.as_ptr().cast(), styles.len()) };

    let selection_list = iced_aw::selection_list(wrapped_styles, move |selection_index, _| {
        message::Message::Pane(
            self_pane,
            message::Pane::StyleEditorStyleSelected(selection_index),
        )
    })
    .width(iced::Length::Fixed(200.0))
    .height(iced::Length::Fixed(200.0));

    let create_button = iced::widget::button(iced::widget::text("Create new"))
        .on_press(message::Message::CreateStyle);
    let delete_button = iced::widget::button(iced::widget::text("Delete")).on_press_maybe(
        // Do not allow deleting the first style
        (pane_state.selected_style_index == 0).then_some(message::Message::DeleteStyle(
            pane_state.selected_style_index,
        )),
    );

    let left_column = iced::widget::column![
        iced::widget::text("Styles").size(20),
        selection_list,
        iced::widget::row![create_button, delete_button].spacing(5)
    ]
    .spacing(5);

    // Right column starts here

    let i = pane_state.selected_style_index;
    let selected_style = &global_state.subtitles.styles[i];

    let bold_checkbox = iced::widget::checkbox("Bold", selected_style.bold, move |val| {
        message::Message::SetStyleBold(i, val)
    });

    let right_column = iced::widget::column![bold_checkbox].spacing(5);

    let content = iced::widget::row![left_column, right_column];

    super::View {
        title: iced::widget::text("Pane title").into(),
        content: iced::widget::container(content)
            .width(iced::Length::Fill)
            .height(iced::Length::Fill)
            .center_x()
            .center_y()
            .into(),
    }
}

#[allow(clippy::needless_pass_by_value)]
pub fn update(
    editor_state: &mut State,
    pane_message: message::Pane,
) -> iced::Command<message::Message> {
    #[allow(clippy::single_match)]
    match pane_message {
        message::Pane::StyleEditorStyleSelected(style_index) => {
            editor_state.selected_style_index = style_index;
        }
        _ => (),
    }

    iced::Command::none()
}
