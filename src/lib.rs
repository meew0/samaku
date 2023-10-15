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

use crate::pane::State;

pub mod keyboard;
pub mod media;
pub mod message;
pub mod model;
pub mod nde;
pub mod pane;
pub mod resources;
pub mod style;
pub mod subtitle;
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

    /// Currently loaded subtitles, if present.
    pub subtitles: subtitle::SlineTrack,

    /// Index of currently active sline, if one exists.
    pub active_sline_index: Option<usize>,

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
    /// Notifies all entities (like node editor panes) that keep some internal copy of the
    /// NDE filter list to update their internal representations
    pub fn update_filter_lists(&mut self) {
        for pane in self.panes.panes.values_mut() {
            if let State::NodeEditor(node_editor_state) = pane {
                node_editor_state.update_filter_names(&self.subtitles);
            }
        }
    }

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

        (
            // ...and initial global state
            Samaku {
                panes,
                focus: None,
                toasts: vec![],
                workers: workers::Workers::spawn_all(&shared_state),
                actual_frame: None,
                video_metadata: None,
                subtitles: subtitle::SlineTrack::default(),
                active_sline_index: None,
                shared: shared_state,
                view: RefCell::new(ViewState {
                    subtitle_renderer: media::subtitle::Renderer::new(),
                }),
                playing: false,
                reticules: None,
            },
            // Tell iced to load the UI font (Barlow) when loading the application, so it is
            // immediately available for rendering.
            iced::font::load(resources::BARLOW).map(|_| message::Message::None),
        )
    }

    fn title(&self) -> String {
        String::from("samaku")
    }

    /// The global update method. Takes a [`Message`] emitted by a UI widget somewhere, runs
    /// whatever processing is required, and updates the global state based on it. This will cause
    /// iced to rerender the application afterwards.
    #[allow(clippy::too_many_lines)]
    fn update(&mut self, message: Self::Message) -> Command<Self::Message> {
        #[allow(clippy::match_same_arms)]
        match message {
            Self::Message::None => {}
            Self::Message::SplitPane(axis) => {
                if let Some(pane) = self.focus {
                    let result = self.panes.split(axis, &pane, pane::State::Unassigned);

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
                self.panes.resize(&split, ratio);
            }
            Self::Message::SetPaneState(pane, new_state) => {
                if let Some(pane_state) = self.panes.get_mut(&pane) {
                    *pane_state = *new_state;
                }
            }
            Self::Message::Pane(pane, pane_message) => {
                if let Some(pane_state) = self.panes.get_mut(&pane) {
                    return pane::dispatch_update(pane_state, pane_message);
                }
            }
            Self::Message::FocusedPane(pane_message) => {
                if let Some(pane) = self.focus {
                    if let Some(pane_state) = self.panes.get_mut(&pane) {
                        return pane::dispatch_update(pane_state, pane_message);
                    }
                }
            }
            Self::Message::Toast(toast) => {
                self.toasts.push(toast);
            }
            Self::Message::CloseToast(index) => {
                self.toasts.remove(index);
            }
            Self::Message::SelectVideoFile => {
                return Command::perform(
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
                return Command::perform(
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
                let future = async {
                    match rfd::AsyncFileDialog::new().pick_file().await {
                        Some(handle) => {
                            Some(smol::fs::read_to_string(handle.path()).await.unwrap())
                        }
                        None => None,
                    }
                };
                return Command::perform(
                    future,
                    Self::Message::map_option(Self::Message::SubtitleFileRead),
                );
            }
            Self::Message::SubtitleFileRead(content) => {
                let ass = media::subtitle::OpaqueTrack::parse(&content);
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
                        .playback_position
                        .add_frames(delta_frames, video_metadata.frame_rate);
                }
                self.workers.emit_playback_step();
            }
            Self::Message::PlaybackAdvanceSeconds(delta_seconds) => {
                self.shared.playback_position.add_seconds(delta_seconds);
                self.workers.emit_playback_step();
            }
            Self::Message::TogglePlayback => {
                // Notify workers to play or pause. The respective playback controller will assume
                // responsibility of updating us.
                if self.playing {
                    self.workers.emit_pause();
                } else {
                    self.workers.emit_play();
                }
            }
            Self::Message::Playing(playing) => {
                self.playing = playing;
            }
            Self::Message::AddSline => {
                let new_sline = subtitle::Sline {
                    start: subtitle::StartTime(0),
                    duration: subtitle::Duration(5000),
                    layer_index: 0,
                    style_index: 0,
                    margins: subtitle::Margins {
                        left: 50,
                        right: 50,
                        vertical: 50,
                    },
                    text: "Sphinx of black quartz, judge my vow".to_string(),
                    nde_filter_index: None,
                };
                self.subtitles.slines.push(new_sline);
            }
            Self::Message::SelectSline(index) => self.active_sline_index = Some(index),
            Self::Message::SetActiveSlineText(new_text) => {
                if let Some(sline) = self.subtitles.active_sline_mut(self.active_sline_index) {
                    sline.text = new_text;
                }
            }
            Self::Message::CreateEmptyFilter => {
                self.subtitles.filters.push(nde::Filter {
                    name: String::new(),
                    graph: nde::graph::Graph::identity(),
                });
                self.update_filter_lists();
            }
            Self::Message::AssignFilterToActiveSline(filter_index) => {
                if let Some(active_sline) = self.subtitles.active_sline_mut(self.active_sline_index)
                {
                    active_sline.nde_filter_index = Some(filter_index);
                }
            }
            Self::Message::UnassignFilterFromActiveSline => {
                if let Some(active_sline) = self.subtitles.active_sline_mut(self.active_sline_index)
                {
                    active_sline.nde_filter_index = None;
                }
            }
            Self::Message::SetActiveFilterName(new_name) => {
                if let Some(filter) = self
                    .subtitles
                    .active_nde_filter_mut(self.active_sline_index)
                {
                    filter.name = new_name;
                    self.update_filter_lists();
                }
            }
            Self::Message::DeleteFilter(_filter_index) => {
                todo!()
            }
            Self::Message::AddNode(node_shell) => {
                if let Some(filter) = self
                    .subtitles
                    .active_nde_filter_mut(self.active_sline_index)
                {
                    let visual_node = nde::graph::VisualNode {
                        node: node_shell.instantiate(),
                        position: iced::Point::new(0.0, 0.0),
                    };
                    filter.graph.nodes.push(visual_node);
                }
            }
            Self::Message::MoveNode(node_index, x, y) => {
                if let Some(filter) = self
                    .subtitles
                    .active_nde_filter_mut(self.active_sline_index)
                {
                    let node = &mut filter.graph.nodes[node_index];
                    node.position = iced::Point::new(node.position.x + x, node.position.y + y);
                }
            }
            Self::Message::ConnectNodes(link) => {
                if let Some(filter) = self
                    .subtitles
                    .active_nde_filter_mut(self.active_sline_index)
                {
                    let (start, end) = link.unwrap_sockets();
                    filter.graph.connect(
                        nde::graph::NextEndpoint {
                            node_index: end.node_index,
                            socket_index: end.socket_index,
                        },
                        nde::graph::PreviousEndpoint {
                            node_index: start.node_index,
                            socket_index: start.socket_index,
                        },
                    );
                }
            }
            Self::Message::DisconnectNodes(endpoint, new_dangling_end_position, source_pane) => {
                if let Some(filter) = self
                    .subtitles
                    .active_nde_filter_mut(self.active_sline_index)
                {
                    let maybe_previous = filter.graph.disconnect(nde::graph::NextEndpoint {
                        node_index: endpoint.node_index,
                        socket_index: endpoint.socket_index,
                    });

                    if let Some(previous) = maybe_previous {
                        if let Some(pane::State::NodeEditor(node_editor_state)) =
                            self.panes.get_mut(&source_pane)
                        {
                            let new_dangling_source = iced_node_editor::LogicalEndpoint {
                                node_index: previous.node_index,
                                role: iced_node_editor::SocketRole::Out,
                                socket_index: previous.socket_index,
                            };
                            node_editor_state.dangling_source = Some(new_dangling_source);
                            node_editor_state.dangling_connection =
                                Some(iced_node_editor::Link::from_unordered(
                                    iced_node_editor::Endpoint::Socket(new_dangling_source),
                                    iced_node_editor::Endpoint::Absolute(new_dangling_end_position),
                                ));
                        }
                    }
                }
            }
            Self::Message::SetReticules(reticules) => {
                self.reticules = Some(reticules);
            }
            Self::Message::UpdateReticulePosition(index, position) => {
                if let Some(reticules) = &mut self.reticules {
                    if let Some(filter) = self
                        .subtitles
                        .active_nde_filter_mut(self.active_sline_index)
                    {
                        if let Some(node) = filter.graph.nodes.get_mut(reticules.source_node_index)
                        {
                            node.node.reticule_update(reticules, index, position);
                        }
                    }
                }
            }
            Self::Message::TrackMotionForNode(node_index, initial_region) => {
                if let Some(video_metadata) = self.video_metadata {
                    let current_frame = self.current_frame().unwrap(); // video is loaded

                    // Update the node's cached track to put the marker it requested at the
                    // position of the current frame.
                    // The node can't do this itself, because it does not know the number of
                    // the current frame.
                    self.subtitles.update_node(
                        self.active_sline_index,
                        node_index,
                        message::Node::MotionTrackUpdate(current_frame, initial_region),
                    );

                    if let Some(sline) = self.subtitles.active_sline(self.active_sline_index) {
                        self.workers.emit_track_motion_for_node(
                            node_index,
                            initial_region,
                            current_frame,
                            video_metadata.frame_rate.ms_to_frame(sline.end().0),
                        );
                    }
                }
            }
            Self::Message::Node(node_index, node_message) => {
                self.subtitles
                    .update_node(self.active_sline_index, node_index, node_message);
            }
        }

        Command::none()
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

        // The title row — currently only contains the logo and the application name.
        // TODO: add buttons/menus for loading/saving/etc
        let title_row = iced::widget::row![
            iced::widget::svg(iced::widget::svg::Handle::from_memory(resources::LOGO))
                .width(30)
                .height(30),
            iced::widget::text("samaku")
                .size(25)
                .style(iced::theme::Text::Color(style::SAMAKU_PRIMARY))
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
