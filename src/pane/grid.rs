use std::fmt::{Display, Formatter};

use crate::{message, subtitle};

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
                ColumnField::Start => "Start",
                ColumnField::Duration => "Duration",
                ColumnField::Text => "Text",
            }
        )
    }
}

impl<'a, 'b> iced_table::table::Column<'a, 'b, message::Message, iced::Renderer> for Column {
    type Row = subtitle::Sline;

    fn header(
        &'b self,
        col_index: usize,
    ) -> iced::advanced::graphics::core::Element<'a, message::Message, iced::Renderer> {
        iced::widget::container(iced::widget::text(format!("{}", self.field)))
            .height(24)
            .center_y()
            .into()
    }

    fn cell(
        &'b self,
        col_index: usize,
        row_index: usize,
        row: &'b Self::Row,
    ) -> iced::advanced::graphics::core::Element<'a, message::Message, iced::Renderer> {
        iced::widget::container(iced::widget::text(match self.field {
            ColumnField::Start => format!("{}", row.start.0),
            ColumnField::Duration => format!("{}", row.duration.0),
            ColumnField::Text => format!("{}", row.text),
        }))
        .height(24)
        .center_y()
        .into()
    }

    fn width(&self) -> f32 {
        self.width
    }

    fn resize_offset(&self) -> Option<f32> {
        self.resize_offset
    }
}

pub fn view<'a>(global_state: &'a crate::Samaku, grid_state: &'a State) -> super::PaneView<'a> {
    let table = iced::widget::responsive(|size| {
        iced_table::table(
            grid_state.header_scrollable_id.clone(),
            grid_state.body_scrollable_id.clone(),
            &grid_state.columns,
            &global_state.subtitles.slines,
            |offset| message::Message::Pane(message::PaneMessage::GridSyncHeader(offset)),
        )
        .on_column_resize(
            |index, offset| {
                message::Message::Pane(message::PaneMessage::GridColumnResizing(index, offset))
            },
            message::Message::Pane(message::PaneMessage::GridColumnResized),
        )
        .min_width(size.width)
        .into()
    });

    super::PaneView {
        title: iced::widget::text("Subtitle grid").into(),
        content: iced::widget::container(table)
            .width(iced::Length::Fill)
            .height(iced::Length::Fill)
            .center_x()
            .center_y()
            .into(),
    }
}

pub fn update(
    grid_state: &mut State,
    pane_message: message::PaneMessage,
) -> iced::Command<message::Message> {
    match pane_message {
        message::PaneMessage::GridSyncHeader(offset) => {
            return iced::widget::scrollable::scroll_to(
                grid_state.header_scrollable_id.clone(),
                offset,
            );
        }
        message::PaneMessage::GridColumnResizing(index, offset) => {
            if let Some(column) = grid_state.columns.get_mut(index) {
                column.resize_offset = Some(offset);
            }
        }
        message::PaneMessage::GridColumnResized => {
            grid_state.columns.iter_mut().for_each(|column| {
                if let Some(offset) = column.resize_offset.take() {
                    column.width += offset;
                }
            })
        }
    }

    return iced::Command::none();
}
