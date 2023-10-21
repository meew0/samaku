#![warn(clippy::pedantic)]
#![warn(clippy::style)]
#![allow(clippy::enum_glob_use)]
#![allow(clippy::doc_markdown)] // Useful to have in general, but too many false positives — perhaps worth revisiting later?

use std::cell::RefCell;
use std::sync::{Arc, Mutex};

use iced::widget::container;
use iced::widget::pane_grid::{self, PaneGrid};
use iced::{event, executor, subscription, Alignment, Event};
use iced::{Application, Command, Element, Length, Settings, Subscription};

pub mod keyboard;
pub mod media;
pub mod menu;
pub mod message;
pub mod model;
pub mod nde;
pub mod pane;
pub mod resources;
pub mod style;
pub mod subtitle;
mod update;
pub mod version;
pub mod view;
pub mod workers;

/// Effectively samaku's main function. Creates and starts the application.
#[allow(clippy::missing_errors_doc)]
pub fn run() -> iced::Result {
    Samaku::run(Settings {
        id: Some("samaku".to_owned()),
        window: iced::window::Settings::default(),
        flags: (),
        default_font: iced::Font {
            family: iced::font::Family::Name("Barlow"),
            weight: iced::font::Weight::Normal,
            stretch: iced::font::Stretch::Normal,
            monospaced: false,
        },
        default_text_size: 16.0,
        antialiasing: false,
        exit_on_close_request: true,
    })
}

/// Global application state.
pub struct Samaku {
    /// Workers represent separate threads running certain CPU-intensive tasks, like video and audio
    /// decoding. The `Workers` interface is available to send messages to them.
    workers: workers::Workers,

    /// State that needs to be shared with the workers, like the playback position.
    shared: SharedState,

    /// State that needs to be mutable in view code, like caching of results to avoid rerunning
    /// certain calculations over and over.
    view: RefCell<ViewState>,

    /// The current state of the global pane grid.
    /// Includes all state for the individual panes themselves.
    panes: pane_grid::State<pane::State>,

    /// Currently focused pane, if one exists.
    focus: Option<pane_grid::Pane>,

    /// Toasts (notifications) to be shown over the UI.
    toasts: Vec<view::toast::Toast>,

    /// Metadata of the currently loaded video, if and only if any is loaded.
    pub video_metadata: Option<media::VideoMetadata>,

    /// Currently loaded subtitles. Will contain some useful defaults if nothing has been loaded
    /// yet.
    pub subtitles: subtitle::File,

    /// Index of currently selected event, if one exists.
    pub active_event_index: Option<usize>,

    /// The number of the frame that is actually being displayed right now,
    /// together with the image it represents.
    /// Will be slightly different from the information in
    /// `playback_state` due to decoding latency etc.
    pub actual_frame: Option<(model::FrameNumber, iced::widget::image::Handle)>,

    /// Our own representation of whether playback is currently running or not.
    /// Setting this does nothing; it is updated by playback controller workers.
    pub playing: bool,

    /// Control widgets that are shown over the video, in order to allow quick setting of positions
    /// and the like.
    pub reticules: Option<model::reticule::Reticules>,
}

/// Data that needs to be shared with workers.
pub struct SharedState {
    /// Currently loaded audio, if present.
    /// Can be shared into workers etc., but be sure not to hold the mutex for
    /// too long, otherwise the playback worker will stall.
    pub audio: Arc<Mutex<Option<media::Audio>>>,

    /// Authoritative playback position and state.
    /// Set this to seek/pause/resume etc.
    pub playback_position: Arc<model::playback::Position>,
}

/// More-or-less temporary data, that needs to be mutable within View functions.
pub struct ViewState {
    pub subtitle_renderer: media::subtitle::Renderer,
}

/// Utility methods for global state
impl Samaku {
    /// Returns the frame rate of the loaded video, or 24 fps if no video is loaded.
    pub fn frame_rate(&self) -> media::FrameRate {
        if let Some(video_metadata) = self.video_metadata {
            video_metadata.frame_rate
        } else {
            media::FrameRate {
                numerator: 24,
                denominator: 1,
            }
        }
    }

    /// Create a context for compilation.
    pub fn compile_context(&self) -> subtitle::compile::Context {
        subtitle::compile::Context {
            frame_rate: self.frame_rate(),
        }
    }

    /// Get the best guess for the number of the currently displayed frame. Returns `None` if no
    /// video is loaded.
    pub fn current_frame(&self) -> Option<model::FrameNumber> {
        match self.actual_frame {
            Some((frame, _)) => Some(frame),
            None => self.video_metadata.map(|metadata| {
                self.shared
                    .playback_position
                    .current_frame(metadata.frame_rate)
            }),
        }
    }
}

impl Application for Samaku {
    type Executor = executor::Default;
    type Message = message::Message;
    type Theme = iced::Theme;
    type Flags = ();

    fn new(_flags: ()) -> (Self, Command<Self::Message>) {
        let (panes, _) = pane_grid::State::new(pane::State::Unassigned);

        // Initial shared state...
        let shared_state = SharedState {
            audio: Arc::new(Mutex::new(None)),
            playback_position: Arc::new(model::playback::Position::default()),
        };

        // ...and initial global state
        let global_state = Samaku {
            panes,
            focus: None,
            toasts: vec![],
            workers: workers::Workers::spawn_all(&shared_state),
            actual_frame: None,
            video_metadata: None,
            subtitles: subtitle::File::default(),
            active_event_index: None,
            shared: shared_state,
            view: RefCell::new(ViewState {
                subtitle_renderer: media::subtitle::Renderer::new(),
            }),
            playing: false,
            reticules: None,
        };

        // Tell iced to load the UI font (Barlow), as well as the icon font provided by iced_aw,
        // when loading the application, so they are immediately available for rendering.
        let on_load = Command::batch(vec![
            iced::font::load(resources::BARLOW).map(|_| message::Message::None),
            iced::font::load(iced_aw::graphics::icons::ICON_FONT_BYTES)
                .map(|_| message::Message::None),
        ]);

        (global_state, on_load)
    }

    fn title(&self) -> String {
        format!("samaku {}", version::Long)
    }

    /// The global update method. Takes a [`Message`] emitted by a UI widget somewhere, runs
    /// whatever processing is required, and updates the global state based on it. This will cause
    /// iced to rerender the application afterwards.
    fn update(&mut self, message: Self::Message) -> Command<Self::Message> {
        // The update logic is implemented in `update.rs`, to keep this file somewhat clean and to
        // make it easier to add utility functions to the update logic.
        update::update(self, message)
    }

    /// Construct the user interface. Called whenever iced needs to rerender the application.
    fn view(&self) -> Element<Self::Message> {
        let focus = self.focus;

        // The pane grid makes up the main part of the application. All the fundamental
        // functionality, like moving panes around, is provided by iced here; we just take care
        // of filling the panes with content.
        let pane_grid =
            PaneGrid::new::<pane::State>(&self.panes, |pane, pane_state, _is_maximized| {
                // This closure is called for every pane.

                let is_focused = focus == Some(pane);

                // Construct the user interface within the pane itself, based on whatever the pane
                // struct wants to do.
                let pane_view = pane::dispatch_view(pane, self, pane_state);
                let title_bar =
                    pane_grid::TitleBar::new(pane_view.title)
                        .padding(5)
                        .style(if is_focused {
                            style::title_bar_focused
                        } else {
                            style::title_bar_active
                        });
                pane_grid::Content::new(pane_view.content)
                    .title_bar(title_bar)
                    .style(if is_focused {
                        style::pane_focused
                    } else {
                        style::pane_active
                    })
            })
            .width(Length::Fill)
            .height(Length::Fill)
            .spacing(5)
            .on_click(Self::Message::FocusPane)
            .on_drag(Self::Message::DragPane)
            .on_resize(0, Self::Message::ResizePane);

        // We implement our own non-native menu using iced_aw. The entry definitions are located
        // in `menu.rs`.
        // Once iced supports native menus again, we may switch to that.
        let menu_bar = iced_aw::menu_bar!(menu::file(), menu::media())
            .spacing(5.0)
            .item_width(iced_aw::menu::ItemWidth::Uniform(180))
            .item_height(iced_aw::menu::ItemHeight::Uniform(32));

        // The title row — currently only contains the logo and the application name.
        // TODO: add buttons/menus for loading/saving/etc
        let title_row = iced::widget::row![
            iced::widget::svg(iced::widget::svg::Handle::from_memory(resources::LOGO))
                .width(30)
                .height(30),
            iced::widget::text("samaku")
                .size(25)
                .style(iced::theme::Text::Color(style::SAMAKU_PRIMARY)),
            iced::widget::horizontal_space(Length::Fixed(10.0)),
            menu_bar
        ]
        .spacing(5)
        .align_items(Alignment::Center);

        let content: Element<Self::Message> =
            container(iced::widget::column![title_row, pane_grid].spacing(10))
                .width(Length::Fill)
                .height(Length::Fill)
                .padding(5)
                .into();

        view::toast::Manager::new(content, &self.toasts, message::Message::CloseToast)
            .timeout(view::toast::DEFAULT_TIMEOUT)
            .into()
    }

    fn theme(&self) -> Self::Theme {
        style::samaku_theme()
    }

    fn subscription(&self) -> Subscription<Self::Message> {
        use iced::futures::StreamExt;

        // Handle incoming global events, like key presses
        let events = subscription::events_with(|event, status| {
            if let event::Status::Captured = status {
                return None;
            }

            // Call the function in the `keyboard` module for every key press.
            match event {
                Event::Keyboard(iced::keyboard::Event::KeyPressed {
                    modifiers,
                    key_code,
                }) => keyboard::handle_key_press(modifiers, key_code),
                _ => None,
            }
        });

        // This is the magic code that allows us to listen to messages emitted by the workers.
        // While `subscription` is called frequently, we specify the same ID (`TypeID` of `Workers`)
        // every time, so only the result of the first `unfold` call is actually used, which is the
        // only one where `self.workers.receiver.take()` produces a `Some` value. For all subsequent
        // times `subscription` is called, the second argument will be `None` and would lead to a
        // panic if it were unwrapped within the closure, but the closure is never called because
        // the initially created subscription is never overwritten.
        let worker_messages = subscription::unfold(
            std::any::TypeId::of::<workers::Workers>(),
            self.workers.receiver.take(),
            move |mut receiver| async move {
                let message = receiver.as_mut().unwrap().next().await.unwrap();
                (message, receiver)
            },
        );

        Subscription::batch(vec![events, worker_messages])
    }
}

#[cfg(test)]
pub mod test_utils {
    use std::env;
    use std::path::{Path, PathBuf};

    /// Creates a `PathBuf` pointing to the given file relative to the root directory, and ensures
    /// the file exists.
    ///
    /// # Panics
    /// Panics if the file could not be found.
    pub fn test_file<P>(join_path: P) -> PathBuf
    where
        P: AsRef<Path>,
    {
        let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
        let path = Path::new(&manifest_dir).join(&join_path);
        assert!(
            path.exists(),
            "Could not find test data ({})! Perhaps some relative-path problem?",
            join_path.as_ref().display()
        );
        path
    }
}
