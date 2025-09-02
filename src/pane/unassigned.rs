use crate::message;

#[must_use]
pub fn view<'a>(self_pane: super::Pane) -> super::View<'a> {
    super::View {
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
                                Box::new(super::State::Video(super::video::State))
                            )
                        ),
                        iced::widget::button("Grid").on_press(
                            message::Message::SetPaneState(
                                self_pane,
                                Box::new(super::State::Grid(super::grid::State::default()))
                            )
                        ),
                        iced::widget::button("Text editor").on_press(
                            message::Message::SetPaneState(
                                self_pane,
                                Box::new(super::State::TextEditor(super::text_editor::State::default()))
                            )
                        ),
                        iced::widget::button("Node editor").on_press(
                            message::Message::SetPaneState(
                                self_pane,
                                Box::new(super::State::NodeEditor(super::node_editor::State::default()))
                            )
                        ),
                        iced::widget::button("Style editor").on_press(
                            message::Message::SetPaneState(
                                self_pane,
                                Box::new(super::State::StyleEditor(super::style_editor::State::default()))
                            )
                        ),
                    ].spacing(10),
                ]
                .spacing(20)
        )
            .center_x(iced::Length::Fill)
            .center_y(iced::Length::Fill)
            .into(),
    }
}
