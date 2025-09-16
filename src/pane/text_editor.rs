use crate::{message, subtitle};
use iced::keyboard::Key;
use iced::keyboard::key::Named;
use iced::widget::text_editor;
use std::fmt::Display;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct State {
    #[serde(skip)]
    styles: iced::widget::combo_box::State<StyleReference>,
    #[serde(skip)]
    selected_style: Option<StyleReference>,
    #[serde(skip)]
    content: text_editor::Content,
}

impl State {
    #[must_use]
    pub fn new(styles: &[subtitle::Style], selected: Option<usize>) -> Self {
        Self {
            styles: Self::create_state(styles),
            selected_style: Self::map_selected(styles, selected),
            content: text_editor::Content::new(),
        }
    }

    pub fn perform(&mut self, action: text_editor::Action) {
        self.content.perform(action);
    }

    pub fn text(&self) -> String {
        self.content.text()
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
                name: style.name().to_owned(),
                index,
            })
            .collect();
        iced::widget::combo_box::State::new(style_refs)
    }

    // TODO maybe newtype `selected`
    #[expect(
        clippy::single_option_map,
        reason = "the purpose of this method is to allow conveniently matching this kind of option"
    )]
    fn map_selected(styles: &[subtitle::Style], selected: Option<usize>) -> Option<StyleReference> {
        selected.map(|index| StyleReference {
            name: styles[index].name().to_owned(),
            index,
        })
    }
}

impl Default for State {
    fn default() -> Self {
        Self {
            styles: iced::widget::combo_box::State::new(vec![]),
            selected_style: None,
            content: text_editor::Content::new(),
        }
    }
}

#[typetag::serde(name = "text_editor")]
impl super::LocalState for State {
    fn view<'a>(
        &'a self,
        self_pane: super::Pane,
        global_state: &'a crate::Samaku,
    ) -> super::View<'a> {
        // TODO: Implement editing of multiple lines at once

        let content: iced::Element<message::Message> = match global_state
            .subtitles
            .events
            .active_event(&global_state.selected_event_indices)
        {
            Some(active_event) => {
                let first_line = active_first_line(self, active_event);
                let second_line = active_second_line(active_event);
                let main_text = active_main_text(self, self_pane);

                iced::widget::column![first_line, second_line, main_text]
                    .spacing(5.0)
                    .into()
            }
            None => match global_state.selected_event_indices.len() {
                0 => iced::widget::text("No subtitle line currently selected."),
                1 => unreachable!(),
                _ => iced::widget::text("More than one subtitle line currently selected."),
            }
            .into(),
        };

        super::View {
            title: iced::widget::text("Text editor").into(),
            content: iced::widget::container(content)
                .center_x(iced::Length::Fill)
                .center_y(iced::Length::Fill)
                .padding(5.0)
                .into(),
        }
    }

    fn visit(&mut self, visitor: &mut dyn super::Visitor) {
        visitor.visit_text_editor(self);
    }

    fn update_active_event_text(&mut self, active_event: &subtitle::Event) {
        self.content = text_editor::Content::with_text(&active_event.text);
    }

    fn update_style_lists(
        &mut self,
        styles: &[subtitle::Style],
        copy_styles: bool,
        active_event_style_index: Option<usize>,
    ) {
        if copy_styles {
            self.update_styles(styles);
        }
        self.update_selected(styles, active_event_style_index);
    }
}

fn active_first_line<'a>(
    state: &'a State,
    active_event: &'a subtitle::Event,
) -> iced::Element<'a, message::Message> {
    // Checkbox to make the event a comment or not
    let comment_checkbox = iced::widget::checkbox("Comment", active_event.is_comment())
        .on_toggle(|is_comment| {
            if is_comment {
                message::Message::SetActiveEventType(subtitle::EventType::Comment)
            } else {
                message::Message::SetActiveEventType(subtitle::EventType::Dialogue)
            }
        })
        .spacing(5.0);

    // Style selection combo box
    let style_selector = iced::widget::combo_box(
        &state.styles,
        "Style",
        state.selected_style.as_ref(),
        |style_ref| message::Message::SetActiveEventStyleIndex(style_ref.index),
    )
    .width(iced::Length::FillPortion(1));

    // Text boxes to enter the actor and the effect
    let actor_text = iced::widget::text_input("Actor", &active_event.actor)
        .on_input(message::Message::SetActiveEventActor)
        .width(iced::Length::FillPortion(1));
    let effect_text = iced::widget::text_input("Effect", &active_event.effect)
        .on_input(message::Message::SetActiveEventEffect)
        .width(iced::Length::FillPortion(1));

    iced::widget::row![
        comment_checkbox,
        iced::widget::Space::with_width(iced::Length::Fixed(10.0)),
        style_selector,
        actor_text,
        effect_text
    ]
    .spacing(5.0)
    .align_y(iced::Alignment::Center)
    .into()
}

fn active_second_line<'a>(
    active_event: &'a subtitle::Event,
) -> iced::Element<'a, message::Message> {
    // Numeric controls
    let start_time_control =
        iced_aw::number_input(&active_event.start.0, ..i64::MAX, |new_start_ms| {
            message::Message::SetActiveEventStartTime(subtitle::StartTime(new_start_ms))
        });
    let duration_control =
        iced_aw::number_input(&active_event.duration.0, ..i64::MAX, |new_duration_ms| {
            message::Message::SetActiveEventDuration(subtitle::Duration(new_duration_ms))
        });
    let layer_control = iced_aw::number_input(
        &active_event.layer_index,
        ..i32::MAX,
        message::Message::SetActiveEventLayerIndex,
    );

    iced::widget::row![
        "Start time (ms):",
        start_time_control,
        "Duration (ms):",
        duration_control,
        iced::widget::horizontal_space(),
        "Layer:",
        layer_control
    ]
    .spacing(5.0)
    .align_y(iced::Alignment::Center)
    .into()
}

fn active_main_text(
    state: &'_ State,
    self_pane: super::Pane,
) -> iced::Element<'_, message::Message> {
    text_editor(&state.content)
        .placeholder("Enter subtitle text...")
        .height(iced::Length::Fill)
        .on_action(move |action| message::Message::TextEditorActionPerformed(self_pane, action))
        .key_binding(|key_press| {
            if key_press.status == text_editor::Status::Focused {
                return match key_press.key {
                    Key::Named(Named::Enter) => key_press.modifiers.shift().then(|| {
                        text_editor::Binding::Sequence(vec![
                            text_editor::Binding::Insert('\\'),
                            text_editor::Binding::Insert('N'),
                        ])
                    }),
                    Key::Named(Named::Delete) => Some(text_editor::Binding::Delete),
                    // TODO more key bindings
                    _ => text_editor::Binding::from_key_press(key_press),
                };
            }
            None
        })
        .wrapping(iced::widget::text::Wrapping::Word)
        .into()
}

inventory::submit! {
    super::Shell::new(
        "Text editor",
        || Box::new(State::default())
    )
}

#[derive(Debug, Clone)]
struct StyleReference {
    name: String,
    index: usize,
}

impl Display for StyleReference {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.name)
    }
}
