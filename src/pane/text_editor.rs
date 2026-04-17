use crate::{action, message, style, subtitle};
use iced::keyboard::Key;
use iced::keyboard::key::Named;
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::fmt::Display;
use std::hash::Hash;
use std::sync::Arc;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct State {
    #[serde(skip)]
    styles: iced::widget::combo_box::State<StyleReference>,
    #[serde(skip)]
    editor_content: iced::widget::text_editor::Content,
    #[serde(skip)]
    editor_text_cache: Cow<'static, str>,
    #[serde(skip)]
    multi_event: Option<MultiEvent>,
}

impl State {
    pub fn perform(&mut self, action: iced::widget::text_editor::Action) -> Option<String> {
        let is_edit = action.is_edit();
        self.editor_content.perform(action);
        is_edit.then(|| {
            let new_text = self.editor_content.text();
            self.editor_text_cache = Cow::Owned(new_text.clone());
            new_text
        })
    }

    pub fn text(&self) -> String {
        self.editor_content.text()
    }

    pub fn update_styles(&mut self, styles: &[subtitle::Style]) {
        self.styles = Self::create_state(styles);
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
}

impl Default for State {
    fn default() -> Self {
        Self {
            styles: iced::widget::combo_box::State::new(vec![]),
            editor_content: iced::widget::text_editor::Content::new(),
            editor_text_cache: String::new().into(),
            multi_event: None,
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
        let content: iced::Element<message::Message> = if let Some(multi_event) = &self.multi_event
        {
            let first_line = active_first_line(global_state, self, multi_event);
            let second_line = active_second_line(multi_event);
            let main_text = active_main_text(self, multi_event, self_pane);

            iced::widget::column![first_line, second_line, main_text]
                .spacing(5.0)
                .into()
        } else {
            iced::widget::text("No subtitle line currently selected.").into()
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

    fn update_selected_events(
        &mut self,
        selected_event_indices: &HashSet<subtitle::EventIndex>,
        event_track: &subtitle::EventTrack,
    ) {
        if selected_event_indices.is_empty() {
            self.multi_event = None;
            // Don't bother updating the editor stuff here since the editor will not be displayed anyway.
            // If we're lucky we could reuse it later
        } else {
            // We need to ensure consistent ordering from now on, so,
            // clone the list of selected events into a slice.
            // We use Arc since we need to put these in a `Message` later,
            // so it needs to be `Send`.
            let event_indices: Arc<[subtitle::EventIndex]> =
                selected_event_indices.iter().copied().collect();
            let events = event_track.all(&event_indices);
            let multi_event = if events.len() < 50 {
                MultiEvent::from_events::<true>(event_indices, &events)
            } else {
                MultiEvent::from_events::<false>(event_indices, &events)
            };
            let editor_text = multi_event.text.primary.clone();

            if self.editor_text_cache != editor_text {
                // Only update the editor state if the text actually changed
                self.editor_content = iced::widget::text_editor::Content::with_text(&editor_text);
                self.editor_text_cache = editor_text;
            }

            self.multi_event = Some(multi_event);
        }
    }

    fn update_style_lists(&mut self, styles: &[subtitle::Style], copy_styles: bool) {
        if copy_styles {
            self.update_styles(styles);
        }
    }
}

#[derive(Debug)]
struct MultiEvent {
    indices: Arc<[subtitle::EventIndex]>,
    start: Count<subtitle::StartTime>,
    duration: Count<subtitle::Duration>,
    layer_index: Count<i32>,
    style_index: Count<usize>,
    _margins: Count<subtitle::Margins>, // TODO
    text: Count<Cow<'static, str>>,
    actor: Count<Cow<'static, str>>,
    effect: Count<Cow<'static, str>>,
    event_type: Count<subtitle::EventType>,
}

impl MultiEvent {
    fn from_events<const FULL_COUNT: bool>(
        event_indices: Arc<[subtitle::EventIndex]>,
        events: &[&subtitle::Event<'static>],
    ) -> Self {
        Self {
            indices: event_indices,
            start: Count::count_event_values::<FULL_COUNT>(events, |event| event.start),
            duration: Count::count_event_values::<FULL_COUNT>(events, |event| event.duration),
            layer_index: Count::count_event_values::<FULL_COUNT>(events, |event| event.layer_index),
            style_index: Count::count_event_values::<FULL_COUNT>(events, |event| event.style_index),
            _margins: Count::count_event_values::<FULL_COUNT>(events, |event| event.margins),
            text: Count::count_event_values::<FULL_COUNT>(events, |event| &event.text)
                .into_cloned(),
            actor: Count::count_event_values::<FULL_COUNT>(events, |event| &event.actor)
                .into_cloned(),
            effect: Count::count_event_values::<FULL_COUNT>(events, |event| &event.effect)
                .into_cloned(),
            event_type: Count::count_event_values::<FULL_COUNT>(events, |event| event.event_type),
        }
    }
}

#[derive(Debug)]
struct Count<T> {
    num_values: Option<usize>,
    primary: T,
}

impl<T> Count<T>
where
    T: Hash + Eq,
{
    fn count_event_values<'a, const FULL_COUNT: bool>(
        events: &[&'a subtitle::Event<'static>],
        accessor: fn(&'a subtitle::Event<'static>) -> T,
    ) -> Self
    where
        T: 'a,
    {
        if FULL_COUNT {
            // Count up how many times each value occurs
            let mut counts: HashMap<T, u64> = HashMap::new();
            for event in events {
                let value = accessor(event);
                if let Some(count) = counts.get_mut(&value) {
                    *count += 1;
                } else {
                    counts.insert(value, 1);
                }
            }

            // Find the most common value
            // TODO make this stable, such that ties are broken deterministically
            let num_values = counts.len();
            let (primary, _) = counts.into_iter().max_by_key(|&(_, count)| count).unwrap();

            Self {
                num_values: Some(num_values),
                primary,
            }
        } else {
            // If `FULL_COUNT` is false, we have decided there are too many events to reasonably count the values up.
            // So we just choose an arbitrary value and decide we don't know exactly how many values there are.
            let primary = accessor(events[0]);
            Self {
                num_values: None,
                primary,
            }
        }
    }
}

impl<T> Count<&T>
where
    T: Clone,
{
    fn into_cloned(self) -> Count<T> {
        let Count {
            num_values,
            primary,
        } = self;

        Count {
            num_values,
            primary: primary.clone(),
        }
    }
}

impl<T> Count<T> {
    fn tooltip<'a, E: Into<iced::Element<'a, message::Message>>>(
        &self,
        content: E,
    ) -> iced::Element<'a, message::Message> {
        let text = match self.num_values {
            Some(num) => {
                if num == 1 {
                    // Don't add a tooltip if there's exactly one value
                    return content.into();
                }

                iced::widget::text(format!("{num} values"))
            }
            None => iced::widget::text("Multiple values"),
        };

        iced::widget::tooltip(content.into(), text, iced::widget::tooltip::Position::Top).into()
    }

    // Returns the color associated with this count (white where everything is equal,
    // samaku_destructive where there are multiple values)
    fn color(&self) -> iced::Color {
        match self.num_values {
            Some(1) => style::SAMAKU_TEXT,
            Some(_) => style::SAMAKU_DESTRUCTIVE,
            None => style::SAMAKU_TEXT_WEAK,
        }
    }
}

fn message_fn<V: Clone, F: Fn(action::MultiEdit<V>) -> message::Message + Copy + 'static>(
    multi_event: &MultiEvent,
    inner: F,
) -> impl Fn(V) -> message::Message + Clone + 'static {
    let indices = Arc::clone(&multi_event.indices);
    move |value| {
        let edit = if indices.len() == 1 {
            action::MultiEdit::Single(indices[0], value)
        } else {
            action::MultiEdit::All(Arc::clone(&indices), value)
        };
        inner(edit)
    }
}

fn active_first_line<'a>(
    global_state: &'a crate::Samaku,
    pane_state: &'a State,
    multi_event: &'a MultiEvent,
) -> iced::Element<'a, message::Message> {
    // Checkbox to make the event a comment or not
    let comment_checkbox = multi_event.event_type.tooltip(
        iced::widget::checkbox(multi_event.event_type.primary.is_comment())
            .label("Comment")
            .style(|theme, status| iced::widget::checkbox::Style {
                icon_color: multi_event.event_type.color(),
                text_color: Some(multi_event.event_type.color()),
                ..iced::widget::checkbox::primary(theme, status)
            })
            .on_toggle(message_fn(multi_event, |edit| {
                message::Message::MultiEditEventType(edit.map(|is_comment| {
                    if is_comment {
                        subtitle::EventType::Comment
                    } else {
                        subtitle::EventType::Dialogue
                    }
                }))
            }))
            .spacing(5.0),
    );

    // Style selection combo box
    let style_index = multi_event.style_index.primary;
    let selected_style = StyleReference {
        name: global_state.subtitles.styles[style_index].name().to_owned(),
        index: style_index,
    };
    let style_selector = multi_event.style_index.tooltip(
        iced::widget::combo_box(
            &pane_state.styles,
            "Style",
            Some(&selected_style),
            message_fn(multi_event, |edit| {
                message::Message::MultiEditEventStyleIndex(
                    edit.map(|style_ref: StyleReference| style_ref.index),
                )
            }),
        )
        .input_style(|theme, status| iced::widget::text_input::Style {
            value: multi_event.style_index.color(),
            ..iced::widget::text_input::default(theme, status)
        })
        .width(iced::Length::FillPortion(1)),
    );

    // Text boxes to enter the actor and the effect
    let actor_text = multi_event.actor.tooltip(
        iced::widget::text_input("Actor", &multi_event.actor.primary)
            .on_input(message_fn(multi_event, |edit| {
                message::Message::MultiEditEventActor(edit.map(Cow::Owned))
            }))
            .style(|theme, status| iced::widget::text_input::Style {
                value: multi_event.actor.color(),
                ..iced::widget::text_input::default(theme, status)
            })
            .width(iced::Length::FillPortion(1)),
    );
    let effect_text = multi_event.effect.tooltip(
        iced::widget::text_input("Effect", &multi_event.effect.primary)
            .on_input(message_fn(multi_event, |edit| {
                message::Message::MultiEditEventEffect(edit.map(Cow::Owned))
            }))
            .style(|theme, status| iced::widget::text_input::Style {
                value: multi_event.effect.color(),
                ..iced::widget::text_input::default(theme, status)
            })
            .width(iced::Length::FillPortion(1)),
    );

    iced::widget::row![
        comment_checkbox,
        iced::widget::Space::new().width(iced::Length::Fixed(10.0)),
        style_selector,
        actor_text,
        effect_text
    ]
    .spacing(5.0)
    .align_y(iced::Alignment::Center)
    .into()
}

fn active_second_line(multi_event: &'_ MultiEvent) -> iced::Element<'_, message::Message> {
    // Numeric controls
    let start_time_control = multi_event.start.tooltip(
        iced_aw::NumberInput::new(
            &multi_event.start.primary.0,
            ..i64::MAX,
            message_fn(multi_event, |edit| {
                message::Message::MultiEditEventStartTime(edit.map(subtitle::StartTime))
            }),
        )
        .input_style(|theme, status| iced::widget::text_input::Style {
            value: multi_event.start.color(),
            ..iced::widget::text_input::default(theme, status)
        }),
    );
    let duration_control = multi_event.duration.tooltip(
        iced_aw::NumberInput::new(
            &multi_event.duration.primary.0,
            ..i64::MAX,
            message_fn(multi_event, |edit| {
                message::Message::MultiEditEventDuration(edit.map(subtitle::Duration))
            }),
        )
        .input_style(|theme, status| iced::widget::text_input::Style {
            value: multi_event.duration.color(),
            ..iced::widget::text_input::default(theme, status)
        }),
    );
    let layer_control = multi_event.layer_index.tooltip(
        iced_aw::NumberInput::new(
            &multi_event.layer_index.primary,
            ..i32::MAX,
            message_fn(multi_event, |edit| {
                message::Message::MultiEditEventLayerIndex(edit)
            }),
        )
        .input_style(|theme, status| iced::widget::text_input::Style {
            value: multi_event.layer_index.color(),
            ..iced::widget::text_input::default(theme, status)
        }),
    );

    iced::widget::row![
        "Start time (ms):",
        start_time_control,
        "Duration (ms):",
        duration_control,
        iced::widget::space::horizontal(),
        "Layer:",
        layer_control
    ]
    .spacing(5.0)
    .align_y(iced::Alignment::Center)
    .into()
}

fn active_main_text<'a>(
    state: &'a State,
    multi_event: &'a MultiEvent,
    self_pane: super::Pane,
) -> iced::Element<'a, message::Message> {
    multi_event.text.tooltip(
        iced::widget::text_editor(&state.editor_content)
            .placeholder("Enter subtitle text...")
            .height(iced::Length::Fill)
            .on_action(message_fn(multi_event, move |action| {
                message::Message::TextEditorActionPerformed(self_pane, action)
            }))
            .key_binding(|key_press| {
                if let iced::widget::text_editor::Status::Focused {
                    is_hovered: _is_hovered,
                } = key_press.status
                {
                    return match key_press.key {
                        Key::Named(Named::Enter) => key_press.modifiers.shift().then(|| {
                            iced::widget::text_editor::Binding::Sequence(vec![
                                iced::widget::text_editor::Binding::Insert('\\'),
                                iced::widget::text_editor::Binding::Insert('N'),
                            ])
                        }),
                        Key::Named(Named::Delete) => {
                            Some(iced::widget::text_editor::Binding::Delete)
                        }
                        // TODO more key bindings
                        _ => iced::widget::text_editor::Binding::from_key_press(key_press),
                    };
                }
                None
            })
            .wrapping(iced::widget::text::Wrapping::Word)
            .style(|theme, status| iced::widget::text_editor::Style {
                value: multi_event.text.color(),
                ..iced::widget::text_editor::default(theme, status)
            }),
    )
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
