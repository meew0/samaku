mod controller;
mod keyboard;
mod media;
mod message;
mod model;
mod theme;
mod view;

use iced::widget::pane_grid::{self, PaneGrid};
use iced::widget::{container, responsive, scrollable, text};
use iced::{event, executor, subscription, Event};
use iced::{Application, Command, Element, Length, Settings, Subscription};
use model::pane::{PaneData, PaneState};

pub fn main() -> iced::Result {
    Samaku::run(Settings::default())
}

struct Samaku {
    global_state: model::GlobalState,
    panes: pane_grid::State<model::pane::PaneData>,
    panes_created: u64,
    focus: Option<pane_grid::Pane>,
}

impl Application for Samaku {
    type Message = message::Message;
    type Theme = iced::Theme;
    type Executor = executor::Default;
    type Flags = ();

    fn new(_flags: ()) -> (Self, Command<Self::Message>) {
        let (panes, _) = pane_grid::State::new(model::pane::PaneData::new(0));

        (
            Samaku {
                panes,
                panes_created: 1,
                focus: None,
                global_state: model::GlobalState::default(),
            },
            Command::none(),
        )
    }

    fn title(&self) -> String {
        String::from("samaku")
    }

    fn theme(&self) -> Self::Theme {
        theme::samaku_theme()
    }

    fn update(&mut self, message: Self::Message) -> Command<Self::Message> {
        match message {
            Self::Message::None => {}
            Self::Message::SplitPane(axis) => {
                if let Some(pane) = self.focus {
                    let result = self.panes.split(
                        axis,
                        &pane,
                        model::pane::PaneData::new(self.panes_created),
                    );

                    if let Some((pane, _)) = result {
                        self.focus = Some(pane);
                    }

                    self.panes_created += 1;
                }
            }
            Self::Message::ClosePane => {
                if let Some(pane) = self.focus {
                    if let Some(_) = self.panes.get(&pane) {
                        if let Some((_, sibling)) = self.panes.close(&pane) {
                            self.focus = Some(sibling);
                        }
                    }
                }
            }
            Self::Message::FocusPane(pane) => self.focus = Some(pane),
            Self::Message::DragPane(pane_grid::DragEvent::Dropped { pane, target }) => {
                self.panes.drop(&pane, target);
            }
            Self::Message::DragPane(_) => {}
            Self::Message::ResizePane(pane_grid::ResizeEvent { split, ratio }) => {
                self.panes.resize(&split, ratio)
            }
            Self::Message::CyclePaneType => {
                if let Some(pane) = self.focus {
                    if let Some(data) = self.panes.get_mut(&pane) {
                        data.state = match data.state {
                            PaneState::Unassigned => {
                                PaneState::Video(model::pane::video::State::default())
                            }
                            PaneState::Video(_) => PaneState::Unassigned,
                        }
                    }
                }
            }
            Self::Message::Global(global_message) => {
                return controller::global::global_update(&mut self.global_state, global_message);
            }
            Self::Message::Dispatch(pane_message) => {
                if let Some(pane) = self.focus {
                    if let Some(data) = self.panes.get_mut(&pane) {
                        return controller::pane::dispatch_update(&mut data.state, pane_message);
                    }
                }
            }
        }

        Command::none()
    }

    fn subscription(&self) -> Subscription<Self::Message> {
        subscription::events_with(|event, status| {
            if let event::Status::Captured = status {
                return None;
            }

            match event {
                Event::Keyboard(iced::keyboard::Event::KeyPressed {
                    modifiers,
                    key_code,
                }) => keyboard::handle_key_press(modifiers, key_code),
                _ => None,
            }
        })
    }

    fn view(&self) -> Element<Self::Message> {
        let focus = self.focus;
        let total_panes = self.panes.len();

        let pane_grid = PaneGrid::new::<PaneData>(&self.panes, |pane, data, is_maximized| {
            let is_focused = focus == Some(pane);

            let pane_view = view::pane::dispatch_view(&self.global_state, &data.state);
            let title_bar = pane_grid::TitleBar::new(pane_view.title);
            pane_grid::Content::new(pane_view.content).title_bar(title_bar)
        })
        .width(Length::Fill)
        .height(Length::Fill)
        .on_click(Self::Message::FocusPane)
        .on_drag(Self::Message::DragPane)
        .on_resize(0, Self::Message::ResizePane);

        container(pane_grid)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }
}
