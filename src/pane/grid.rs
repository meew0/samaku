use std::fmt::{Display, Formatter};

use crate::{message, style, subtitle, view};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct State {
    #[serde(skip, default = "unique_scrollable_id")]
    header_scrollable_id: iced::widget::scrollable::Id,
    #[serde(skip, default = "unique_scrollable_id")]
    body_scrollable_id: iced::widget::scrollable::Id,
    columns: Vec<Column>,
}

#[typetag::serde(name = "grid")]
impl super::LocalState for State {
    fn view<'a>(
        &'a self,
        self_pane: super::Pane,
        global_state: &'a crate::Samaku,
    ) -> super::View<'a> {
        let table = iced::widget::responsive(move |size| {
            iced_table::table(
                self.header_scrollable_id.clone(),
                self.body_scrollable_id.clone(),
                global_state,
                self.columns.as_slice(),
                &[],
                move |offset| {
                    message::Message::Pane(self_pane, message::Pane::GridSyncHeader(offset))
                },
            )
            .on_column_resize(
                move |index, offset| {
                    message::Message::Pane(
                        self_pane,
                        message::Pane::GridColumnResizing(index, offset),
                    )
                },
                message::Message::Pane(self_pane, message::Pane::GridColumnResized),
            )
            .min_width(size.width)
            .into()
        });

        let add_button = iced::widget::button(view::icon(iced_fonts::Bootstrap::Plus))
            .on_press(message::Message::AddEvent);

        let delete_button = iced::widget::button(view::icon(iced_fonts::Bootstrap::Dash))
            .on_press(message::Message::DeleteSelectedEvents);

        let top_bar = iced::widget::container(
            iced::widget::row![add_button, delete_button]
                .spacing(5.0)
                .align_y(iced::Alignment::Center),
        )
        .padding(5.0);

        let content: iced::Element<message::Message> =
            iced::widget::column![top_bar, view::separator(), table].into();

        super::View {
            title: iced::widget::text("Subtitle grid").into(),
            content: iced::widget::container(content)
                .center_x(iced::Length::Fill)
                .center_y(iced::Length::Fill)
                .into(),
        }
    }

    fn update(&mut self, pane_message: message::Pane) -> iced::Task<message::Message> {
        match pane_message {
            message::Pane::GridSyncHeader(offset) => {
                return iced::widget::scrollable::scroll_to(
                    self.header_scrollable_id.clone(),
                    offset,
                );
            }
            message::Pane::GridColumnResizing(index, offset) => {
                if let Some(column) = self.columns.get_mut(index) {
                    column.resize_offset = Some(offset);
                }
            }
            message::Pane::GridColumnResized => {
                self.columns.iter_mut().for_each(|column| {
                    if let Some(offset) = column.resize_offset.take() {
                        column.width += offset;
                    }
                });
            }
            _ => (),
        }

        iced::Task::none()
    }
}

inventory::submit! {
    super::Shell::new(
        "Subtitle grid",
        || Box::new(State::default())
    )
}

fn unique_scrollable_id() -> iced::widget::scrollable::Id {
    iced::widget::scrollable::Id::unique()
}

impl Default for State {
    fn default() -> Self {
        Self {
            header_scrollable_id: unique_scrollable_id(),
            body_scrollable_id: unique_scrollable_id(),
            columns: vec![
                Column {
                    field: ColumnField::SelectButton,
                    width: 100.0,
                    resize_offset: None,
                },
                Column {
                    field: ColumnField::FilterName,
                    width: 200.0,
                    resize_offset: None,
                },
                Column {
                    field: ColumnField::Start,
                    width: 100.0,
                    resize_offset: None,
                },
                Column {
                    field: ColumnField::Duration,
                    width: 100.0,
                    resize_offset: None,
                },
                Column {
                    field: ColumnField::Text,
                    width: 400.0,
                    resize_offset: None,
                },
            ],
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Column {
    field: ColumnField,
    width: f32,
    resize_offset: Option<f32>,
}

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub enum ColumnField {
    SelectButton,
    FilterName,
    Start,
    Duration,
    Text,
}

impl Display for ColumnField {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "{}",
            match self {
                ColumnField::SelectButton => "Select",
                ColumnField::FilterName => "Filter name",
                ColumnField::Start => "Start",
                ColumnField::Duration => "Duration",
                ColumnField::Text => "Text",
            }
        )
    }
}

fn highlighted_style(theme: &iced::Theme) -> iced::widget::container::Style {
    let pair = theme.extended_palette().primary.weak;

    iced::widget::container::Style {
        background: Some(pair.color.into()),
        text_color: pair.text.into(),
        ..iced::widget::container::rounded_box(theme)
    }
}

fn comment_style(theme: &iced::Theme) -> iced::widget::container::Style {
    iced::widget::container::Style {
        text_color: style::SAMAKU_TEXT_WEAK.into(),
        ..iced::widget::container::rounded_box(theme)
    }
}

impl<'a> iced_table::table::Column<'a, message::Message, iced::Theme, iced::Renderer> for Column {
    type Row = (subtitle::EventIndex, subtitle::Event<'static>);
    type State = crate::Samaku;

    fn header(&'a self, _col_index: usize) -> iced::Element<'a, message::Message> {
        iced::widget::container(iced::widget::text(format!("{}", self.field)))
            .center_y(24)
            .into()
    }

    fn cell(
        &'a self,
        _col_index: usize,
        _row_index: usize,
        state: &'a Self::State,
        (event_index, event): &'a Self::Row,
    ) -> iced::Element<'a, message::Message> {
        let selected = state.selected_event_indices.contains(event_index);

        let cell_content: iced::Element<message::Message> = match self.field {
            ColumnField::SelectButton => {
                let icon = if selected {
                    iced_fonts::Bootstrap::Dot
                } else {
                    iced_fonts::Bootstrap::CircleFill
                };

                iced::widget::button(view::icon(icon).size(12.0))
                    .on_press(message::Message::ToggleEventSelection(*event_index))
                    .into()
            }
            ColumnField::FilterName => iced::widget::text(
                match state.subtitles.extradata.nde_filter_for_event(event) {
                    Some(filter) => {
                        let stored_name = &filter.name;
                        if stored_name.is_empty() {
                            "(unnamed filter)"
                        } else {
                            stored_name
                        }
                    }
                    None => "",
                },
            )
            .into(),
            ColumnField::Start => iced::widget::text(format!("{}", event.start.0)).into(),
            ColumnField::Duration => iced::widget::text(format!("{}", event.duration.0)).into(),
            ColumnField::Text => iced::widget::text(event.text.to_string()).into(),
        };

        // Highlight the selected event
        let container = iced::widget::container(cell_content);

        let styled_container = if selected {
            container.style(highlighted_style)
        } else if event.is_comment() {
            container.style(comment_style)
        } else {
            container
        };

        styled_container.center_y(24).into()
    }

    fn width(&self) -> f32 {
        self.width
    }

    fn resize_offset(&self) -> Option<f32> {
        self.resize_offset
    }
}
