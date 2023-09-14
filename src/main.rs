#![feature(int_roundings)]

use std::cell::RefCell;
use std::sync::{Arc, Mutex};

use iced::widget::container;
use iced::widget::pane_grid::{self, PaneGrid};
use iced::{event, executor, subscription, Event};
use iced::{Application, Command, Element, Length, Settings, Subscription};

mod keyboard;
mod media;
mod message;
mod model;
mod pane;
mod style;
mod subtitle;
mod view;
mod workers;

pub fn main() -> iced::Result {
    Samaku::run(Settings::default())
}

/// Global application state.
struct Samaku {
    workers: workers::Workers,

    shared: SharedState,
    view: RefCell<ViewState>,

    /// The current state of the global pane grid.
    /// Includes all state for the individual panes themselves.
    panes: pane_grid::State<pane::PaneState>,

    /// Currently focused pane, if one exists.
    focus: Option<pane_grid::Pane>,

    /// Metadata of the currently loaded video, if and only if any is loaded.
    pub video_metadata: Option<media::VideoMetadata>,

    /// Currently loaded subtitles, if present.
    pub subtitles: subtitle::SlineTrack,

    /// The number of the frame that is actually being displayed right now,
    /// together with the image it represents.
    /// Will be slightly different from the information in
    /// `playback_state` due to decoding latency etc.
    pub actual_frame: Option<(i32, iced::widget::image::Handle)>,
}

/// Data that needs to be shared with workers.
struct SharedState {
    /// Currently loaded audio, if present.
    /// Can be shared into workers etc., but be sure not to hold the mutex for
    /// too long, otherwise the playback worker will stall.
    pub audio: Arc<Mutex<Option<media::Audio>>>,

    /// Authoritative playback position and state.
    /// Set this to seek/pause/resume etc.
    pub playback_state: Arc<model::playback::PlaybackState>,
}

/// More-or-less temporary data, that needs to be mutable within View functions.
struct ViewState {
    pub subtitle_renderer: media::subtitle::Renderer,
}

impl Application for Samaku {
    type Message = message::Message;
    type Theme = iced::Theme;
    type Executor = executor::Default;
    type Flags = ();

    fn new(_flags: ()) -> (Self, Command<Self::Message>) {
        let (panes, _) = pane_grid::State::new(pane::PaneState::Unassigned);

        let shared_state = SharedState {
            audio: Arc::new(Mutex::new(None)),
            playback_state: Arc::new(model::playback::PlaybackState::default()),
        };

        (
            Samaku {
                panes,
                focus: None,
                workers: workers::Workers::spawn_all(&shared_state),
                actual_frame: None,
                video_metadata: None,
                subtitles: subtitle::SlineTrack::default(),
                shared: shared_state,
                view: RefCell::new(ViewState {
                    subtitle_renderer: media::subtitle::Renderer::new(),
                }),
            },
            Command::none(),
        )
    }

    fn title(&self) -> String {
        String::from("samaku")
    }

    fn theme(&self) -> Self::Theme {
        style::samaku_theme()
    }

    fn update(&mut self, message: Self::Message) -> Command<Self::Message> {
        match message {
            Self::Message::None => {}
            Self::Message::SplitPane(axis) => {
                if let Some(pane) = self.focus {
                    let result = self.panes.split(axis, &pane, pane::PaneState::Unassigned);

                    if let Some((pane, _)) = result {
                        self.focus = Some(pane);
                    }
                }
            }
            Self::Message::ClosePane => {
                if let Some(pane) = self.focus {
                    if self.panes.get(&pane).is_some() {
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
            Self::Message::SetPaneState(pane, new_state) => {
                if let Some(pane_state) = self.panes.get_mut(&pane) {
                    *pane_state = *new_state;
                }
            }
            Self::Message::Pane(pane_message) => {
                if let Some(pane) = self.focus {
                    if let Some(pane_state) = self.panes.get_mut(&pane) {
                        return pane::dispatch_update(pane_state, pane_message);
                    }
                }
            }
            Self::Message::SelectVideoFile => {
                return iced::Command::perform(
                    rfd::AsyncFileDialog::new().pick_file(),
                    Self::Message::map_option(|handle: rfd::FileHandle| {
                        Self::Message::VideoFileSelected(handle.path().to_path_buf())
                    }),
                );
            }
            Self::Message::VideoFileSelected(path_buf) => {
                self.workers.emit_load_video(path_buf);
            }
            Self::Message::VideoLoaded(metadata) => {
                self.video_metadata = Some(*metadata);
                self.workers.emit_playback_step();
            }
            Self::Message::SelectAudioFile => {
                return iced::Command::perform(
                    rfd::AsyncFileDialog::new().pick_file(),
                    Self::Message::map_option(|handle: rfd::FileHandle| {
                        Self::Message::AudioFileSelected(handle.path().to_path_buf())
                    }),
                );
            }
            Self::Message::AudioFileSelected(path_buf) => {
                let mut audio_lock = self.shared.audio.lock().unwrap();
                *audio_lock = Some(media::Audio::load(path_buf));
                self.workers.emit_restart_audio();
            }
            Self::Message::SelectSubtitleFile => {
                if self.video_metadata.is_some() {
                    let future = async {
                        match rfd::AsyncFileDialog::new().pick_file().await {
                            Some(handle) => {
                                Some(smol::fs::read_to_string(handle.path()).await.unwrap())
                            }
                            None => None,
                        }
                    };
                    return iced::Command::perform(
                        future,
                        Self::Message::map_option(|content| {
                            Self::Message::SubtitleFileRead(content)
                        }),
                    );
                }
            }
            Self::Message::SubtitleFileRead(content) => {
                let ass = media::subtitle::OpaqueTrack::parse(content);
                self.subtitles = ass.to_sline_track();
            }
            Self::Message::VideoFrameAvailable(new_frame, handle) => {
                self.actual_frame = Some((new_frame, handle));
            }
            Self::Message::PlaybackStep => {
                self.workers.emit_playback_step();
            }
            Self::Message::PlaybackAdvanceFrames(delta_frames) => {
                if let Some(video_metadata) = &self.video_metadata {
                    self.shared
                        .playback_state
                        .add_frames(delta_frames, video_metadata.frame_rate);
                }
                self.workers.emit_playback_step();
            }
            Self::Message::PlaybackAdvanceSeconds(delta_seconds) => {
                self.shared.playback_state.add_seconds(delta_seconds);
                self.workers.emit_playback_step();
            }
            Self::Message::TogglePlayback => {
                // For some reason `fetch_not`, which would perform a toggle in place,
                // is unstable. `fetch_xor` with true should be equivalent.
                self.shared
                    .playback_state
                    .playing
                    .fetch_xor(true, std::sync::atomic::Ordering::Relaxed);
            }
        }

        Command::none()
    }

    fn subscription(&self) -> Subscription<Self::Message> {
        let events = subscription::events_with(|event, status| {
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
        });

        use iced::futures::StreamExt;
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
    fn view(&self) -> Element<Self::Message> {
        let focus = self.focus;
        // let total_panes = self.panes.len();

        let pane_grid =
            PaneGrid::new::<pane::PaneState>(&self.panes, |pane, pane_state, _is_maximized| {
                let is_focused = focus == Some(pane);

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

        container(pane_grid)
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(5)
            .into()
    }
}
