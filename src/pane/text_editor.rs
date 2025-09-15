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
        }
    }
}

impl super::LocalState for State {
    fn view<'a>(
        &'a self,
        _self_pane: super::Pane,
        global_state: &'a crate::Samaku,
    ) -> super::View<'a> {
        // TODO: Implement editing of multiple lines at once

        let content: iced::Element<message::Message> = match global_state
            .subtitles
            .events
            .active_event(&global_state.selected_event_indices)
        {
            Some(active_event) => {
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
                    &self.styles,
                    "Style",
                    self.selected_style.as_ref(),
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

                let first_line = iced::widget::row![
                    comment_checkbox,
                    iced::widget::Space::with_width(iced::Length::Fixed(10.0)),
                    style_selector,
                    actor_text,
                    effect_text
                ]
                .spacing(5.0)
                .align_y(iced::Alignment::Center);

                // Numeric controls
                let start_time_control =
                    iced_aw::number_input(&active_event.start.0, ..i64::MAX, |new_start_ms| {
                        message::Message::SetActiveEventStartTime(subtitle::StartTime(new_start_ms))
                    });
                let duration_control = iced_aw::number_input(
                    &active_event.duration.0,
                    ..i64::MAX,
                    |new_duration_ms| {
                        message::Message::SetActiveEventDuration(subtitle::Duration(
                            new_duration_ms,
                        ))
                    },
                );
                let layer_control = iced_aw::number_input(
                    &active_event.layer_index,
                    ..i32::MAX,
                    message::Message::SetActiveEventLayerIndex,
                );

                let second_line = iced::widget::row![
                    "Start time (ms):",
                    start_time_control,
                    "Duration (ms):",
                    duration_control,
                    iced::widget::horizontal_space(),
                    "Layer:",
                    layer_control
                ]
                .spacing(5.0)
                .align_y(iced::Alignment::Center);

                let main_text = iced::widget::responsive(|size| {
                    iced::widget::text_input("Enter subtitle text...", &active_event.text)
                        .on_input(message::Message::SetActiveEventText)
                        .width(size.width)
                        .line_height(iced::widget::text::LineHeight::Absolute(size.height.into()))
                        .into()
                });

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

    fn update(&mut self, _pane_message: message::Pane) -> iced::Task<message::Message> {
        iced::Task::none()
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
