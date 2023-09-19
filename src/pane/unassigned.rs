#[allow(unused_imports)]
use crate::message;

pub fn view<'a>(self_pane: iced::widget::pane_grid::Pane) -> super::PaneView<'a> {
    super::PaneView {
        title: iced::widget::text("Unassigned pane").into(),
        content:
        iced::widget::container(
            iced::widget::column![
                    iced::widget::text("Unassigned pane").size(20),
                    "Press F2 to split vertically, F3 to split horizontally, or click one of the buttons below to set the pane's type.",
                    iced::widget::row![
                        iced::widget::button("Video").on_press(
                            message::Message::SetPaneState(
                                self_pane,
                                Box::new(super::PaneState::Video(super::video::State::default()))
                            )
                        ),
                        iced::widget::button("Grid").on_press(
                            message::Message::SetPaneState(
                                self_pane,
                                Box::new(super::PaneState::Grid(super::grid::State::default()))
                            )
                        ),
                        iced::widget::button("Text editor").on_press(
                            message::Message::SetPaneState(
                                self_pane,
                                Box::new(super::PaneState::TextEditor(super::text_editor::State::default()))
                            )
                        ),
                        iced::widget::button("Node editor").on_press(
                            message::Message::SetPaneState(
                                self_pane,
                                Box::new(super::PaneState::NodeEditor(super::node_editor::State::default()))
                            )
                        ),
                    ].spacing(10),
                ]
                .spacing(20)
        )
            .width(iced::Length::Fill)
            .height(iced::Length::Fill)
            .center_x()
            .center_y()
            .into(),
    }
}
