use std::fmt::Display;

use crate::{message, subtitle};

#[derive(Debug, Clone)]
pub struct State {
    styles: iced::widget::combo_box::State<StyleReference>,
    selected_style: Option<StyleReference>,
}

impl State {
    #[must_use]
    pub fn new(styles: &[subtitle::Style], selected: Option<usize>) -> Self {
        Self {
            styles: Self::create_state(styles),
            selected_style: Self::map_selected(styles, selected),
        }
    }

    pub fn update_styles(&mut self, styles: &[subtitle::Style]) {
        self.styles = Self::create_state(styles);
    }

    pub fn update_selected(&mut self, styles: &[subtitle::Style], selected: Option<usize>) {
        self.selected_style = Self::map_selected(styles, selected);
    }

    fn create_state(styles: &[subtitle::Style]) -> iced::widget::combo_box::State<StyleReference> {
        let style_refs = styles
            .iter()
            .enumerate()
            .map(|(index, style)| StyleReference {
                name: style.name.clone(),
                index,
            })
            .collect();
        iced::widget::combo_box::State::new(style_refs)
    }

    fn map_selected(styles: &[subtitle::Style], selected: Option<usize>) -> Option<StyleReference> {
        selected.map(|index| StyleReference {
            name: styles[index].name.clone(),
            index,
        })
    }
}

impl Default for State {
    fn default() -> Self {
        Self {
            styles: iced::widget::combo_box::State::new(vec![]),
            selected_style: None,
        }
    }
}

#[derive(Debug, Clone)]
struct StyleReference {
    name: String,
    index: usize,
}

impl Display for StyleReference {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.name)
    }
}

pub fn view<'a>(
    _self_pane: super::Pane,
    global_state: &'a crate::Samaku,
    editor_state: &'a State,
) -> super::View<'a> {
    let content: iced::Element<message::Message> = match global_state
        .subtitles
        .events
        .active_event(global_state.active_event_index)
    {
        Some(active_event) => {
            // Checkbox to make the event a comment or not
            let comment_checkbox =
                iced::widget::checkbox("Comment", active_event.is_comment(), |is_comment| {
                    if is_comment {
                        message::Message::SetActiveEventType(subtitle::EventType::Comment)
                    } else {
                        message::Message::SetActiveEventType(subtitle::EventType::Dialogue)
                    }
                });

            // Style selection combo box
            let style_selector = iced::widget::combo_box(
                &editor_state.styles,
                "Style",
                editor_state.selected_style.as_ref(),
                |style_ref| message::Message::SetActiveEventStyleIndex(style_ref.index),
            );

            let top_line = iced::widget::row![comment_checkbox, style_selector,]
                .spacing(5.0)
                .align_items(iced::Alignment::Center);

            let main_text = iced::widget::responsive(|size| {
                iced::widget::text_input("Enter subtitle text...", &active_event.text)
                    .on_input(message::Message::SetActiveEventText)
                    .width(size.width)
                    .line_height(iced::widget::text::LineHeight::Absolute(size.height.into()))
                    .into()
            });

            iced::widget::column![top_line, main_text]
                .spacing(5.0)
                .into()
        }
        None => iced::widget::text("No subtitle line currently selected.").into(),
    };

    super::View {
        title: iced::widget::text("Text editor").into(),
        content: iced::widget::container(content)
            .width(iced::Length::Fill)
            .height(iced::Length::Fill)
            .center_x()
            .center_y()
            .padding(5.0)
            .into(),
    }
}

#[allow(clippy::needless_pass_by_value)]
pub fn update(
    _editor_state: &mut State,
    _pane_message: message::Pane,
) -> iced::Command<message::Message> {
    iced::Command::none()
}
