use std::fmt::{Display, Formatter};

use crate::{message, subtitle, view};

#[derive(Debug, Clone)]
pub struct State {
    header_scrollable_id: iced::widget::scrollable::Id,
    body_scrollable_id: iced::widget::scrollable::Id,
    columns: Vec<Column>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            header_scrollable_id: iced::widget::scrollable::Id::unique(),
            body_scrollable_id: iced::widget::scrollable::Id::unique(),
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

#[derive(Debug, Clone)]
pub struct Column {
    field: ColumnField,
    width: f32,
    resize_offset: Option<f32>,
}

#[derive(Debug, Clone, Copy)]
pub enum ColumnField {
    SelectButton,
    FilterName,
    Start,
    Duration,
    Text,
}

impl Display for ColumnField {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
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

struct Highlighted;

impl iced::widget::container::StyleSheet for Highlighted {
    type Style = iced::Theme;

    fn appearance(&self, theme: &iced::Theme) -> iced::widget::container::Appearance {
        let pair = theme.extended_palette().primary.weak;

        iced::widget::container::Appearance {
            background: Some(pair.color.into()),
            text_color: pair.text.into(),
            ..Default::default()
        }
    }
}

impl<'a, 'b> iced_table::table::Column<'a, 'b, message::Message, iced::Renderer>
    for (&'a crate::Samaku, &'a Column)
{
    type Row = subtitle::Event<'static>;

    fn header(
        &'b self,
        _col_index: usize,
    ) -> iced::advanced::graphics::core::Element<'a, message::Message, iced::Renderer> {
        iced::widget::container(iced::widget::text(format!("{}", self.1.field)))
            .height(24)
            .center_y()
            .into()
    }

    fn cell(
        &'b self,
        _col_index: usize,
        row_index: usize,
        row: &'b Self::Row,
    ) -> iced::advanced::graphics::core::Element<'a, message::Message, iced::Renderer> {
        let cell_content: iced::Element<message::Message> = match self.1.field {
            ColumnField::SelectButton => {
                if self.0.active_event_index == Some(row_index) {
                    // Don't show a select button for the already selected row
                    iced::widget::text("").into()
                } else {
                    iced::widget::button(view::icon(iced_aw::Icon::CircleFill).size(12.0))
                        .on_press(message::Message::SelectEvent(row_index))
                        .into()
                }
            }
            ColumnField::FilterName => {
                iced::widget::text(match self.0.subtitles.extradata.nde_filter_for_event(row) {
                    Some(filter) => {
                        let stored_name = &filter.name;
                        if stored_name.is_empty() {
                            "(unnamed filter)"
                        } else {
                            stored_name
                        }
                    }
                    None => "",
                })
                .into()
            }
            ColumnField::Start => iced::widget::text(format!("{}", row.start.0)).into(),
            ColumnField::Duration => iced::widget::text(format!("{}", row.duration.0)).into(),
            ColumnField::Text => iced::widget::text(row.text.to_string()).into(),
        };

        // Highlight the selected event
        let container = if self.0.active_event_index == Some(row_index) {
            let style = iced::theme::Container::Custom(Box::new(Highlighted));
            iced::widget::container(cell_content).style(style)
        } else {
            iced::widget::container(cell_content)
        };

        container.height(24).center_y().into()
    }

    fn width(&self) -> f32 {
        self.1.width
    }

    fn resize_offset(&self) -> Option<f32> {
        self.1.resize_offset
    }
}

pub fn view<'a>(
    self_pane: super::Pane,
    global_state: &'a crate::Samaku,
    grid_state: &'a State,
) -> super::View<'a> {
    let columns_with_state: Vec<(&'a crate::Samaku, &Column)> = std::iter::repeat(global_state)
        .zip(&grid_state.columns)
        .collect();

    let table = iced::widget::responsive(move |size| {
        iced_table::table(
            grid_state.header_scrollable_id.clone(),
            grid_state.body_scrollable_id.clone(),
            columns_with_state.as_slice(),
            global_state.subtitles.events.as_slice(),
            // We have to use `FocusedPane` here (and in `on_column_resize`) because `iced_table`
            // does not support passing a closure here.
            // TODO: Make a PR to support this?
            |offset| message::Message::FocusedPane(message::Pane::GridSyncHeader(offset)),
        )
        .on_column_resize(
            |index, offset| {
                message::Message::FocusedPane(message::Pane::GridColumnResizing(index, offset))
            },
            message::Message::Pane(self_pane, message::Pane::GridColumnResized),
        )
        .min_width(size.width)
        .into()
    });

    let add_button =
        iced::widget::button(view::icon(iced_aw::Icon::Plus)).on_press(message::Message::AddEvent);

    let top_bar = iced::widget::container(
        iced::widget::row![add_button]
            .spacing(5.0)
            .align_items(iced::Alignment::Center),
    )
    .padding(5.0);

    let content: iced::Element<message::Message> =
        iced::widget::column![top_bar, view::separator(), table].into();

    super::View {
        title: iced::widget::text("Subtitle grid").into(),
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
    grid_state: &mut State,
    pane_message: message::Pane,
) -> iced::Command<message::Message> {
    match pane_message {
        message::Pane::GridSyncHeader(offset) => {
            return iced::widget::scrollable::scroll_to(
                grid_state.header_scrollable_id.clone(),
                offset,
            );
        }
        message::Pane::GridColumnResizing(index, offset) => {
            if let Some(column) = grid_state.columns.get_mut(index) {
                column.resize_offset = Some(offset);
            }
        }
        message::Pane::GridColumnResized => {
            grid_state.columns.iter_mut().for_each(|column| {
                if let Some(offset) = column.resize_offset.take() {
                    column.width += offset;
                }
            });
        }
        _ => (),
    }

    iced::Command::none()
}
