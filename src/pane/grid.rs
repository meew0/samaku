use crate::{message, style, subtitle, view};

#[derive(Default, Debug, serde::Serialize, serde::Deserialize)]
pub struct State {
    #[serde(skip)]
    last_viewport: Option<iced::widget::scrollable::Viewport>,
}

const EVENT_HEIGHT: f32 = 32.0;

#[typetag::serde(name = "grid")]
impl super::LocalState for State {
    fn view<'a>(
        &'a self,
        self_pane: super::Pane,
        global_state: &'a crate::Samaku,
    ) -> super::View<'a> {
        let total_events = global_state.subtitles.events.len();
        let first_event_float: f32 = if let Some(last_viewport) = self.last_viewport {
            (last_viewport.absolute_offset().y / EVENT_HEIGHT).floor()
        } else {
            0.0
        };

        #[expect(clippy::cast_sign_loss, reason = "value should be positive")]
        #[expect(clippy::cast_possible_truncation, reason = "rounded via floor()")]
        let first_event_to_display = (first_event_float as usize).min(total_events);

        let table = iced::widget::responsive(move |size| {
            #[expect(clippy::cast_sign_loss, reason = "value should be positive")]
            #[expect(clippy::cast_possible_truncation, reason = "rounded via ceil()")]
            let num_events_to_display = (size.height / EVENT_HEIGHT).ceil() as usize;
            let last_event_to_display =
                (first_event_to_display + num_events_to_display).min(total_events);
            let range = first_event_to_display..last_event_to_display;

            let mut column: iced::widget::Column<message::Message> =
                iced::widget::Column::with_capacity(range.len() + 2);
            let top_height = first_event_float * EVENT_HEIGHT;
            #[expect(
                clippy::cast_precision_loss,
                reason = "actually a real problem in this case, with the late conversion after the subtraction it was minimized as much as possible though"
            )]
            let bottom_height =
                (total_events - range.len() - first_event_to_display) as f32 * EVENT_HEIGHT;
            column = column.push(iced::widget::vertical_space().height(top_height));

            let mut parity = false;
            for event_index in global_state.subtitles.events.iter_range_in_order(range) {
                column = column.push(row(self_pane, global_state, self, event_index, parity));
                parity = !parity;
            }

            column = column.push(iced::widget::vertical_space().height(bottom_height));

            iced::widget::scrollable(column)
                .on_scroll(move |viewport| {
                    message::Message::Pane(self_pane, message::Pane::GridScroll(viewport))
                })
                .width(iced::Length::Fill)
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
            message::Pane::GridScroll(viewport) => self.last_viewport = Some(viewport),
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

fn row<'a>(
    _self_pane: super::Pane,
    global_state: &'a crate::Samaku,
    _pane_state: &'a State,
    event_index: subtitle::EventIndex,
    parity: bool,
) -> iced::Element<'a, message::Message> {
    let event = &global_state.subtitles.events[event_index];

    // Cut off event text after a bit
    let cutoff = event.text.len().min(250);

    let background_color = if parity {
        style::SAMAKU_BACKGROUND_WEAK
    } else {
        style::SAMAKU_BACKGROUND
    };

    iced::widget::container(
        iced::widget::row![
            iced::widget::text(event.start.format_long()).width(iced::Length::Fixed(150.0)),
            iced::widget::text(event.end().format_long()).width(iced::Length::Fixed(150.0)),
            iced::widget::text(&event.text[0..cutoff])
        ]
        .height(iced::Length::Fill)
        .align_y(iced::Alignment::Center),
    )
    .style(move |_| iced::widget::container::Style {
        background: Some(background_color.into()),
        ..iced::widget::container::Style::default()
    })
    .width(iced::Length::Fill)
    .height(EVENT_HEIGHT)
    .into()
}
