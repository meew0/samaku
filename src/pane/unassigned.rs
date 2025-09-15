use crate::message;

pub struct State;

impl super::LocalState for State {
    fn view(&self, self_pane: super::Pane, _global_state: &crate::Samaku) -> super::View<'_> {
        // Collect registered panes across the entire codebase
        let pane_type_row = iced::widget::Row::with_children(
            inventory::iter::<super::Shell>.into_iter().map(|shell| {
                iced::widget::button(shell.name)
                    .on_press(message::Message::SetPaneType(self_pane, shell.constructor))
                    .into()
            }),
        );

        super::View {
        title: iced::widget::text("Unassigned pane").into(),
        content:
        iced::widget::container(
            iced::widget::column![
                    iced::widget::text("Unassigned pane").size(20),
                    "Press F2 to split vertically, F3 to split horizontally, or click one of the buttons below to set the pane's type.",
                    pane_type_row.spacing(10),
                ]
                .spacing(20)
        )
            .center_x(iced::Length::Fill)
            .center_y(iced::Length::Fill)
            .into(),
    }
    }
}
