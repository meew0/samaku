//! Global update logic: update the global state ([`Samaku`] object) based on an incoming message.

use smol::io::AsyncBufReadExt as _;
use std::borrow::Cow;
use std::fmt::Write as _;

use crate::message::Message;
use crate::{media, message, model, nde, pane, subtitle, view};

macro_rules! active_event {
    ($global_state:ident) => {
        $global_state
            .subtitles
            .events
            .active_event(&$global_state.selected_event_indices)
    };
}
macro_rules! active_event_mut {
    ($global_state:ident) => {
        $global_state
            .subtitles
            .events
            .active_event_mut(&$global_state.selected_event_indices)
    };
}

macro_rules! iter_panes {
    ($global_state:expr, $p:pat, $b:expr) => {
        for pane in $global_state.panes.panes.values_mut() {
            if let $p = pane {
                $b;
            }
        }
    };
}

pub(crate) fn update(global_state: &mut super::Samaku, message: Message) -> iced::Command<Message> {
    // Run the internal update method, which does the actual updating of global state.
    let command = update_internal(global_state, message);

    // Check whether certain properties have been modified. If they have, we need to notify
    // our panes about this, since some of them contain copies of the data in an iced-specific
    // format, which needs to be kept in sync.
    let styles_modified = global_state.subtitles.styles.check();
    update_style_lists(global_state, styles_modified);

    command
}

/// The internal update method, which actually processes the message and updates global state.
#[expect(
    clippy::too_many_lines,
    reason = "the elm architecture more or less requires a complex global update method"
)]
#[expect(
    clippy::cognitive_complexity,
    reason = "the elm architecture more or less requires a complex global update method"
)]
fn update_internal(global_state: &mut super::Samaku, message: Message) -> iced::Command<Message> {
    #[expect(
        clippy::match_same_arms,
        reason = "needed in this case to coherently group messages together"
    )]
    match message {
        Message::None => {}
        Message::SplitPane(axis) => {
            if let Some(pane) = global_state.focus {
                let result = global_state
                    .panes
                    .split(axis, pane, pane::State::Unassigned);

                if let Some((pane, _)) = result {
                    global_state.focus = Some(pane);
                }
            }
        }
        Message::ClosePane => {
            if let Some(pane) = global_state.focus
                && global_state.panes.get(pane).is_some()
                && let Some((_, sibling)) = global_state.panes.close(pane)
            {
                global_state.focus = Some(sibling);
            }
        }
        Message::FocusPane(pane) => global_state.focus = Some(pane),
        Message::DragPane(iced::widget::pane_grid::DragEvent::Dropped { pane, target }) => {
            global_state.panes.drop(pane, target);
        }
        Message::DragPane(_) => {}
        Message::ResizePane(iced::widget::pane_grid::ResizeEvent { split, ratio }) => {
            global_state.panes.resize(split, ratio);
        }
        Message::SetPaneState(pane, new_state) => {
            if let Some(pane_state) = global_state.panes.get_mut(pane) {
                *pane_state = *new_state;

                update_filter_lists(global_state);
                update_style_lists(global_state, true);
            }
        }
        Message::SetFocusedPaneState(new_state) => {
            if let Some(focused_pane) = global_state.focus
                && let Some(focused_pane_state) = global_state.panes.get_mut(focused_pane)
            {
                *focused_pane_state = *new_state;

                update_filter_lists(global_state);
                update_style_lists(global_state, true);
            }
        }
        Message::Pane(pane, pane_message) => {
            if let Some(pane_state) = global_state.panes.get_mut(pane) {
                return pane::dispatch_update(pane_state, pane_message);
            }
        }
        Message::FocusedPane(pane_message) => {
            if let Some(pane) = global_state.focus
                && let Some(pane_state) = global_state.panes.get_mut(pane)
            {
                return pane::dispatch_update(pane_state, pane_message);
            }
        }
        Message::Toast(toast) => {
            global_state.toast(toast);
        }
        Message::CloseToast(index) => {
            // Sometimes, when two toasts are closed in very quick succession, we receive two
            // consecutive `CloseToast` messages with the same ID, making the second one invalid.
            // I suspect this is due to a race condition somewhere, but for now, try to handle the
            // situation somewhat gracefully.
            // TODO: figure out what causes this
            if index < global_state.toasts.len() {
                global_state.toasts.remove(index);
            } else {
                global_state.toasts.pop();
            }
        }
        Message::SelectVideoFile => {
            return iced::Command::perform(
                rfd::AsyncFileDialog::new().pick_file(),
                Message::map_option(|handle: rfd::FileHandle| {
                    Message::VideoFileSelected(handle.path().to_path_buf())
                }),
            );
        }
        Message::VideoFileSelected(path_buf) => {
            global_state.workers.emit_load_video(path_buf);
        }
        Message::VideoLoaded(metadata) => {
            global_state.video_metadata = Some(*metadata);
            global_state.workers.emit_playback_step();
        }
        Message::SelectAudioFile => {
            return iced::Command::perform(
                rfd::AsyncFileDialog::new().pick_file(),
                Message::map_option(|handle: rfd::FileHandle| {
                    Message::AudioFileSelected(handle.path().to_path_buf())
                }),
            );
        }
        Message::AudioFileSelected(path_buf) => {
            let mut audio_lock = global_state.shared.audio.lock().unwrap();
            *audio_lock = Some(media::Audio::load(path_buf));
            drop(audio_lock);
            global_state.workers.emit_restart_audio();
        }
        Message::ImportSubtitleFile => {
            let future = async {
                match rfd::AsyncFileDialog::new().pick_file().await {
                    Some(handle) => Some(smol::fs::read_to_string(handle.path()).await.unwrap()),
                    None => None,
                }
            };
            return iced::Command::perform(
                future,
                Message::map_option(Message::SubtitleFileReadForImport),
            );
        }
        Message::SubtitleFileReadForImport(content) => {
            let opaque = media::subtitle::OpaqueTrack::parse(&content);

            let (style_list, leftover) = subtitle::StyleList::from_vec(opaque.styles());

            // Show warning toasts for duplicate styles
            if !leftover.is_empty() {
                let duplicate_names = leftover
                    .iter()
                    .map(subtitle::Style::name)
                    .collect::<Vec<&str>>()
                    .join(", ");
                global_state.toast(view::toast::Toast::new(
                    view::toast::Status::Primary,
                    "Duplicate styles".to_owned(),
                    format!(
                        "Skipped the following duplicate styles when loading: {duplicate_names}"
                    ),
                ));
            }

            global_state.subtitles = subtitle::File {
                events: opaque.to_event_track(),
                styles: model::Trace::new(style_list),
                script_info: opaque.script_info(),
                ..Default::default()
            }
        }
        Message::OpenSubtitleFile => {
            let future = async {
                match rfd::AsyncFileDialog::new().pick_file().await {
                    Some(handle) => match smol::fs::File::open(handle.path()).await {
                        Ok(file) => {
                            let lines = smol::io::BufReader::new(file).lines();
                            subtitle::File::parse(lines).await.map(Box::new)
                        }
                        Err(io_err) => Err(subtitle::parse::SubtitleParseError::IoError(io_err)),
                    },
                    None => Err(subtitle::parse::SubtitleParseError::NoFileSelected),
                }
            };

            // The reason we need to block here, instead of asynchronously executing the future,
            // is that otherwise we would have to pass the resulting `AssFile` via a `Message`.
            // This requires it to be cloneable, because messages need to be cloneable in the
            // general case (for example when retained within widgets, like buttons), even
            // though it would not actually need to be cloned in this specific case. It is
            // possible to make `AssFile`s cloneable using type-erased cloning of trait objects,
            // and in fact we likely want to do this someday to implement duplication of NDE
            // filters, but I think `AssFile`s should not actually be cloneable entirely.
            let result = smol::block_on(future);

            match result {
                Ok(file_box) => {
                    let (ass_file, warnings) = *file_box;
                    global_state.subtitles = ass_file;

                    for warning in &warnings {
                        global_state.toast(view::toast::Toast::new(
                            view::toast::Status::Primary,
                            "Warning while loading subtitle file".to_owned(),
                            format!("{warning}"),
                        ));
                    }
                }
                Err(err) => {
                    global_state.toast(view::toast::Toast::new(
                        view::toast::Status::Danger,
                        "Error while loading subtitle file".to_owned(),
                        err.to_string(),
                    ));
                }
            }
        }
        Message::SaveSubtitleFile => {
            let mut data = String::new();
            subtitle::emit(&mut data, &global_state.subtitles, None).unwrap();

            let future = async {
                if let Some(handle) = rfd::AsyncFileDialog::new().save_file().await {
                    smol::fs::write(handle.path(), data).await.unwrap();
                }
            };

            return iced::Command::perform(future, |()| Message::None);
        }
        Message::ExportSubtitleFile => {
            let mut data = String::new();
            subtitle::emit(
                &mut data,
                &global_state.subtitles,
                Some(global_state.compile_context()),
            )
            .unwrap();

            if global_state.video_metadata.is_none() {
                global_state.toast(view::toast::Toast::new(
                    view::toast::Status::Primary,
                    "Warning".to_owned(),
                    format!("Exporting subtitles requires a loaded video for exact results. (Assuming {} fps)", f64::from(global_state.frame_rate())),
                ));
            }

            let future = async {
                if let Some(handle) = rfd::AsyncFileDialog::new().save_file().await {
                    smol::fs::write(handle.path(), data).await.unwrap();
                }
            };

            return iced::Command::perform(future, |()| Message::None);
        }
        Message::VideoFrameAvailable(new_frame, handle) => {
            global_state.actual_frame = Some((new_frame, handle));
        }
        Message::PlaybackStep => {
            global_state.workers.emit_playback_step();
        }
        Message::PlaybackAdvanceFrames(delta_frames) => {
            if let Some(video_metadata) = &global_state.video_metadata {
                global_state
                    .shared
                    .playback_position
                    .add_frames(delta_frames, video_metadata.frame_rate);
            }
            global_state.workers.emit_playback_step();
        }
        Message::PlaybackAdvanceSeconds(delta_seconds) => {
            global_state
                .shared
                .playback_position
                .add_seconds(delta_seconds);
            global_state.workers.emit_playback_step();
        }
        Message::TogglePlayback => {
            // Notify workers to play or pause. The respective playback controller will assume
            // responsibility of updating us.
            if global_state.playing {
                global_state.workers.emit_pause();
            } else {
                global_state.workers.emit_play();
            }
        }
        Message::Playing(playing) => {
            global_state.playing = playing;
        }
        Message::CreateStyle => {
            let mut counter = 1;
            let mut name = format!("New style {counter}");

            while global_state.subtitles.styles.find_by_name(&name).is_some() {
                counter += 1;
                name.truncate("New style ".len());
                write!(name, "{counter}").unwrap();
            }

            let new_style = subtitle::Style {
                name,
                ..Default::default()
            };
            global_state.subtitles.styles.insert(new_style);
        }
        Message::DeleteStyle(index) => {
            global_state.subtitles.styles.remove(index);

            // Update style references in events: assign the default style to all events that had
            // the removed style assigned, and decrement style indices of applicable events (with
            // existing style indices > the removed index)
            for event in &mut global_state.subtitles.events {
                match event.style_index.cmp(&index) {
                    std::cmp::Ordering::Less => {}
                    std::cmp::Ordering::Equal => event.style_index = 0,
                    std::cmp::Ordering::Greater => event.style_index -= 1,
                }
            }
        }
        Message::SetStyleBold(index, value) => {
            global_state.subtitles.styles[index].bold = value;
        }
        Message::SetStyleItalic(index, value) => {
            global_state.subtitles.styles[index].italic = value;
        }
        Message::SetStyleUnderline(index, value) => {
            global_state.subtitles.styles[index].underline = value;
        }
        Message::SetStyleStrikeOut(index, value) => {
            global_state.subtitles.styles[index].strike_out = value;
        }
        Message::AddEvent => {
            let new_event = subtitle::Event {
                start: subtitle::StartTime(0),
                duration: subtitle::Duration(5000),
                layer_index: 0,
                style_index: 0,
                margins: subtitle::Margins {
                    left: 50,
                    right: 50,
                    vertical: 50,
                },
                text: Cow::Owned("Sphinx of black quartz, judge my vow".to_owned()),
                actor: Cow::Owned(String::new()),
                effect: Cow::Owned(String::new()),
                event_type: subtitle::EventType::Dialogue,
                extradata_ids: vec![],
            };
            global_state.subtitles.events.push(new_event);
        }
        Message::DeleteSelectedEvents => {
            global_state
                .subtitles
                .events
                .remove_from_set(&mut global_state.selected_event_indices);
        }
        Message::ToggleEventSelection(index) => {
            if global_state.selected_event_indices.contains(&index) {
                global_state.selected_event_indices.remove(&index);
            } else {
                global_state.selected_event_indices.insert(index);
            }
        }
        Message::SetActiveEventText(new_text) => {
            if let Some(event) = active_event_mut!(global_state) {
                event.text = Cow::Owned(new_text);
            }
        }
        Message::SetActiveEventActor(new_actor) => {
            if let Some(event) = active_event_mut!(global_state) {
                event.actor = Cow::Owned(new_actor);
            }
        }
        Message::SetActiveEventEffect(new_effect) => {
            if let Some(event) = active_event_mut!(global_state) {
                event.effect = Cow::Owned(new_effect);
            }
        }
        Message::SetActiveEventStartTime(new_start_time) => {
            if let Some(event) = active_event_mut!(global_state) {
                event.start = new_start_time;
            }
        }
        Message::SetActiveEventDuration(new_duration) => {
            if let Some(event) = active_event_mut!(global_state) {
                event.duration = new_duration;
            }
        }
        Message::SetActiveEventStyleIndex(new_style_index) => {
            if let Some(event) = active_event_mut!(global_state) {
                event.style_index = new_style_index;
            }
        }
        Message::SetActiveEventLayerIndex(new_layer_index) => {
            if let Some(event) = active_event_mut!(global_state) {
                event.layer_index = new_layer_index;
            }
        }
        Message::SetActiveEventType(new_type) => {
            if let Some(event) = active_event_mut!(global_state) {
                event.event_type = new_type;
            }
        }
        Message::CreateEmptyFilter => {
            global_state.subtitles.extradata.push_filter(nde::Filter {
                name: String::new(),
                graph: nde::graph::Graph::identity(),
            });
            update_filter_lists(global_state);
        }
        Message::AssignFilterToSelectedEvents(filter_index) => {
            for selected_event_index in &global_state.selected_event_indices {
                global_state.subtitles.events[*selected_event_index]
                    .assign_nde_filter(filter_index, &global_state.subtitles.extradata);
            }
        }
        Message::UnassignFilterFromSelectedEvents => {
            for selected_event_index in &global_state.selected_event_indices {
                global_state.subtitles.events[*selected_event_index]
                    .unassign_nde_filter(&global_state.subtitles.extradata);
            }
        }
        Message::SetActiveFilterName(new_name) => {
            if let Some(filter) = global_state.subtitles.events.active_nde_filter_mut(
                &global_state.selected_event_indices,
                &mut global_state.subtitles.extradata,
            ) {
                filter.name = new_name;
                update_filter_lists(global_state);
            }
        }
        Message::DeleteFilter(filter_index) => {
            // Unassign filters from events that might have it assigned
            for event in &mut global_state.subtitles.events {
                event.extradata_ids.retain(|id| *id != filter_index);
            }

            // Remove the filter itself
            global_state.subtitles.extradata.remove(filter_index);

            update_filter_lists(global_state);
        }
        Message::AddNode(node_constructor) => {
            if let Some(filter) = global_state.subtitles.events.active_nde_filter_mut(
                &global_state.selected_event_indices,
                &mut global_state.subtitles.extradata,
            ) {
                let visual_node = nde::graph::VisualNode {
                    node: node_constructor(),
                    position: iced::Point::new(0.0, 0.0),
                };
                filter.graph.nodes.push(visual_node);
            }
        }
        Message::MoveNode(node_index, x, y) => {
            if let Some(filter) = global_state.subtitles.events.active_nde_filter_mut(
                &global_state.selected_event_indices,
                &mut global_state.subtitles.extradata,
            ) {
                let node = &mut filter.graph.nodes[node_index];
                node.position = iced::Point::new(node.position.x + x, node.position.y + y);
            }
        }
        Message::ConnectNodes(link) => {
            if let Some(filter) = global_state.subtitles.events.active_nde_filter_mut(
                &global_state.selected_event_indices,
                &mut global_state.subtitles.extradata,
            ) {
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
        Message::DisconnectNodes(endpoint, new_dangling_end_position, source_pane) => {
            if let Some(filter) = global_state.subtitles.events.active_nde_filter_mut(
                &global_state.selected_event_indices,
                &mut global_state.subtitles.extradata,
            ) {
                let maybe_previous = filter.graph.disconnect(nde::graph::NextEndpoint {
                    node_index: endpoint.node_index,
                    socket_index: endpoint.socket_index,
                });

                if let Some(previous) = maybe_previous
                    && let Some(pane::State::NodeEditor(node_editor_state)) =
                        global_state.panes.get_mut(source_pane)
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
        Message::SetReticules(reticules) => {
            global_state.reticules = Some(reticules);
        }
        Message::UpdateReticulePosition(index, position) => {
            if let Some(reticules) = &mut global_state.reticules
                && let Some(filter) = global_state.subtitles.events.active_nde_filter_mut(
                    &global_state.selected_event_indices,
                    &mut global_state.subtitles.extradata,
                )
                && let Some(node) = filter.graph.nodes.get_mut(reticules.source_node_index)
            {
                node.node.reticule_update(reticules, index, position);
            }
        }
        Message::TrackMotionForNode(node_index, initial_region) => {
            if let Some(video_metadata) = global_state.video_metadata {
                let current_frame = global_state.current_frame().unwrap(); // video is loaded

                // Update the node's cached track to put the marker it requested at the
                // position of the current frame.
                // The node can't do this itglobal_state, because it does not know the number of
                // the current frame.
                global_state.subtitles.events.update_node(
                    &global_state.selected_event_indices,
                    &mut global_state.subtitles.extradata,
                    node_index,
                    message::Node::MotionTrackUpdate(current_frame, initial_region),
                );

                if let Some(event) = global_state
                    .subtitles
                    .events
                    .active_event(&global_state.selected_event_indices)
                {
                    global_state.workers.emit_track_motion_for_node(
                        node_index,
                        initial_region,
                        current_frame,
                        video_metadata.frame_rate.ms_to_frame(event.end().0),
                    );
                }
            }
        }
        Message::Node(node_index, node_message) => {
            global_state.subtitles.events.update_node(
                &global_state.selected_event_indices,
                &mut global_state.subtitles.extradata,
                node_index,
                node_message,
            );
        }
    }

    iced::Command::none()
}

/// Notifies all entities (like node editor panes) that keep some internal copy of the
/// NDE filter list to update their internal representations
fn update_filter_lists(global_state: &mut super::Samaku) {
    iter_panes!(
        global_state,
        pane::State::NodeEditor(node_editor_state),
        node_editor_state.update_filter_names(&global_state.subtitles.extradata)
    );
}

/// Notifies all entities (like text editor panes) that keep some internal copy of the
/// styles list to update their internal representations. If `copy_styles` is false, only the
/// selected style will be updated.
fn update_style_lists(global_state: &mut super::Samaku, copy_styles: bool) {
    let active_event_style_index = active_event!(global_state).map(|event| event.style_index);

    for pane in global_state.panes.panes.values_mut() {
        match pane {
            pane::State::TextEditor(text_editor_state) => {
                if copy_styles {
                    text_editor_state.update_styles(global_state.subtitles.styles.as_slice());
                }
                text_editor_state.update_selected(
                    global_state.subtitles.styles.as_slice(),
                    active_event_style_index,
                );
            }
            pane::State::StyleEditor(style_editor_state) => {
                // A style might have been deleted, which might cause the style selected in a
                // style editor pane to no longer exist. In that case, set it to 0 which will
                // always exist.
                if style_editor_state.selected_style_index >= global_state.subtitles.styles.len() {
                    style_editor_state.selected_style_index = 0;
                }
            }
            _ => {}
        }
    }
}
