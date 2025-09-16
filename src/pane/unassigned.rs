use crate::message;

#[derive(serde::Serialize, serde::Deserialize)]
pub struct State;

#[typetag::serde(name = "unassigned")]
impl super::LocalState for State {
    fn view(&self, self_pane: super::Pane, _global_state: &crate::Samaku) -> super::View<'_> {
        // Collect registered panes across the entire codebase
        let mut shells: Vec<&'static super::Shell> =
            inventory::iter::<super::Shell>.into_iter().collect();
        shells.sort_by_key(|shell| shell.name);
        let pane_type_row = iced::widget::Row::with_children(shells.into_iter().map(|shell| {
            iced::widget::button(shell.name)
                .on_press(message::Message::SetPaneType(self_pane, shell.constructor))
                .into()
        }))
        .spacing(10)
        .wrap();

        super::View {
            title: iced::widget::text("Unassigned pane").into(),
            content: iced::widget::container(
                iced::widget::column![
                            iced::widget::text("Unassigned pane").size(20),
                            "Press F2 to split vertically, F3 to split horizontally, or click one of the buttons below to set the pane's type.",
                            pane_type_row,
                        ]
                    .spacing(20)
            )
            .center_x(iced::Length::Fill)
            .center_y(iced::Length::Fill)
            .into(),
        }
    }
}
