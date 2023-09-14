use iced::widget;

#[allow(unused_imports)]
use crate::message;

pub fn view<'a>(self_pane: iced::widget::pane_grid::Pane) -> super::PaneView<'a> {
    super::PaneView {
        title: widget::text("Unassigned pane").into(),
        content:
        widget::container(
            widget::column![
                    widget::text("Unassigned pane").size(20),
                    "Press F2 to split vertically, F3 to split horizontally, or click one of the buttons below to set the pane's type.",
                    widget::row![
                        widget::button("Video").on_press(
                            message::Message::SetPaneState(
                                self_pane,
                                Box::new(super::PaneState::Video(super::video::State::default()))
                            )
                        ),
                        widget::button("Grid").on_press(
                            message::Message::SetPaneState(
                                self_pane,
                                Box::new(super::PaneState::Grid(super::grid::State::default()))
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