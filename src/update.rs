//! Global update logic: update the global state ([`Samaku`] object) based on an incoming message.

use crate::message::Message;
use crate::{action, media, message, model, nde, pane, project, subtitle, view};
use anyhow::Context as _;
use smol::io::AsyncBufReadExt as _;
use std::borrow::Cow;
use std::fmt::Write as _;

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

/// The global update method. Takes a [`Message`] emitted by a UI widget somewhere, runs
/// whatever processing is required, and updates the global state based on it. This will cause
/// iced to rerender the application afterwards.
pub(crate) fn update(global_state: &mut super::Samaku, message: Message) -> iced::Task<Message> {
    // Run the internal update method, which does the actual updating of global state.
    let task = update_internal(global_state, message);

    // Check whether certain properties have been modified. If they have, we need to notify
    // our panes about this, since some of them contain copies of the data in an iced-specific
    // format, which needs to be kept in sync.
    let styles_modified = global_state.subtitles.styles.check();
    notify_style_lists(global_state, styles_modified);

    task
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
fn update_internal(global_state: &mut super::Samaku, message: Message) -> iced::Task<Message> {
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
                    .split(axis, pane, pane::State::unassigned());

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
        Message::SetPaneType(pane, constructor) => {
            if let Some(pane_state) = global_state.panes.get_mut(pane) {
                *pane_state = pane::State::new(constructor());

                notify_selected_events(global_state);
                notify_filter_lists(global_state);
                notify_style_lists(global_state, true);
            }
        }
        Message::SetFocusedPaneType(constructor) => {
            if let Some(focused_pane) = global_state.focus
                && let Some(focused_pane_state) = global_state.panes.get_mut(focused_pane)
            {
                *focused_pane_state = pane::State::new(constructor());

                notify_selected_events(global_state);
                notify_filter_lists(global_state);
                notify_style_lists(global_state, true);
            }
        }
        Message::Pane(pane, pane_message) => {
            if let Some(pane_state) = global_state.panes.get_mut(pane) {
                return pane_state.local.update(pane_message);
            }
        }
        Message::FocusedPane(pane_message) => {
            if let Some(pane) = global_state.focus
                && let Some(pane_state) = global_state.panes.get_mut(pane)
            {
                return pane_state.local.update(pane_message);
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
            return iced::Task::perform(
                rfd::AsyncFileDialog::new().pick_file(),
                Message::map_option(|handle: rfd::FileHandle| {
                    Message::VideoFileSelected(handle.path().to_path_buf())
                }),
            );
        }
        Message::VideoFileSelected(path_buf) => {
            global_state.project_properties.video_path = Some(path_buf.clone());
            action::load_video(global_state, path_buf);
        }
        Message::VideoLoaded(metadata) => {
            global_state.video_metadata = Some(*metadata);
            global_state.workers.emit_playback_step();
        }
        Message::SelectAudioFile => {
            return iced::Task::perform(
                rfd::AsyncFileDialog::new().pick_file(),
                Message::map_option(|handle: rfd::FileHandle| {
                    Message::AudioFileSelected(handle.path().to_path_buf())
                }),
            );
        }
        Message::AudioFileSelected(path_buf) => {
            global_state.project_properties.audio_path = Some(path_buf.clone());
            action::load_audio(global_state, path_buf);
        }
        Message::ImportSubtitleFile => {
            let future = async {
                match rfd::AsyncFileDialog::new().pick_file().await {
                    Some(handle) => subtitle::import(handle.path()).await.map(Some),
                    None => Ok(None),
                }
            };
            return iced::Task::perform(
                future,
                Message::map_anyhow_option(Message::SubtitleFileReadForImport),
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

            let new_file = subtitle::File {
                events: opaque.to_event_track(),
                styles: model::Trace::new(style_list),
                script_info: opaque.script_info(),
                ..Default::default()
            };

            action::replace_subtitle_file(global_state, new_file);
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

            return iced::Task::perform(future, |result| match result {
                Ok(file_box) => Message::SubtitleFileReadForOpen(model::NeverClone(file_box)),
                Err(err) => Message::SubtitleParseError(model::NeverClone(err)),
            });
        }
        Message::SubtitleFileReadForOpen(file_box) => {
            // Load ASS subtitles themselves
            let (ass_file, warnings) = *(file_box.0);
            action::replace_subtitle_file(global_state, ass_file);

            for warning in &warnings {
                global_state.toast(view::toast::Toast::new(
                    view::toast::Status::Primary,
                    "Warning while loading subtitle file".to_owned(),
                    format!("{warning}"),
                ));
            }

            let project_load_result = project::load(global_state);
            if global_state.anyhow_toast(project_load_result) == Some(true) {
                // Some project metadata was loaded, we might have to perform after-load tasks such as opening linked video/audio files
                return project::after_load(global_state);
            }
        }
        Message::SubtitleParseError(err) => {
            global_state.toast(view::toast::Toast::new(
                view::toast::Status::Danger,
                "Error while loading subtitle file".to_owned(),
                err.to_string(),
            ));
        }
        Message::SaveSubtitleFile => {
            let result = (|| {
                project::store(global_state).context("Failed to serialize project data")?;

                let mut data = String::new();
                subtitle::emit(&mut data, &global_state.subtitles, None)
                    .context("subtitle::emit() failed")?; // should never happen

                Ok(data)
            })();

            if let Some(data) = global_state.anyhow_toast(result) {
                let future = async {
                    select_file_and_save(data)
                        .await
                        .context("Failed to write to file")?;
                    Ok(())
                };
                return iced::Task::perform(future, Message::map_anyhow(|()| Message::None));
            }
        }
        Message::ExportSubtitleFile => {
            let mut data = String::new();
            subtitle::emit(
                &mut data,
                &global_state.subtitles,
                Some(global_state.compile_context()),
            )
            .expect("subtitle::emit() failed"); // should never happen

            if global_state.video_metadata.is_none() {
                global_state.toast(view::toast::Toast::new(
                    view::toast::Status::Primary,
                    "Warning".to_owned(),
                    format!("Exporting subtitles requires a loaded video for exact results. (Assuming {} fps)", f64::from(global_state.frame_rate())),
                ));
            }

            let future = select_file_and_save(data);
            return iced::Task::perform(future, Message::map_anyhow(|()| Message::None));
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
        Message::PlaybackSetPosition(position) => {
            global_state.shared.playback_position.set_to_event(position);
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
            global_state.selected_event_indices.clear();
        }
        Message::ToggleEventSelection(index) => {
            if global_state.selected_event_indices.contains(&index) {
                global_state.selected_event_indices.remove(&index);
            } else {
                global_state.selected_event_indices.insert(index);
            }
            notify_selected_events(global_state);
        }
        Message::SelectOnlyEvent(index) => {
            global_state.selected_event_indices.clear();
            global_state.selected_event_indices.insert(index);
            notify_selected_events(global_state);
        }
        Message::SetActiveEventText(new_text) => {
            if let Some(event) = active_event_mut!(global_state) {
                event.text = Cow::Owned(new_text);
                notify_active_event_text(&mut global_state.panes, event, None);
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
        Message::SetEventStartTimeAndDuration(event_index, start, duration) => {
            if let Some(event) = global_state.subtitles.events.get_mut(event_index) {
                event.start = start;
                event.duration = duration;
            }
        }
        Message::TextEditorActionPerformed(pane, action) => {
            if let Some(pane_state) = global_state.panes.get_mut(pane) {
                // Create a visitor that will perform the given action on the text editor pane, if the given pane is a text editor pane
                struct Visitor {
                    action: Option<iced::widget::text_editor::Action>,
                    new_text: Option<String>,
                }

                impl pane::Visitor for Visitor {
                    fn visit_text_editor(
                        &mut self,
                        text_editor_state: &mut pane::text_editor::State,
                    ) {
                        let is_edit = self.action.as_ref().unwrap().is_edit();

                        text_editor_state.perform(self.action.take().unwrap());

                        if is_edit {
                            self.new_text = Some(text_editor_state.text());
                        }
                    }
                }

                let mut visitor = Visitor {
                    action: Some(action),
                    new_text: None,
                };

                pane_state.local.visit(&mut visitor);

                // Check if the text has changed, and if it has, update the active event
                if let Some(new_text) = visitor.new_text
                    && let Some(event) = active_event_mut!(global_state)
                {
                    event.text = Cow::Owned(new_text);
                    // Notify all other text editors except for the one we just performed the action on
                    notify_active_event_text(&mut global_state.panes, event, Some(pane));
                }
            }
        }
        Message::CreateEmptyFilter => {
            global_state.subtitles.extradata.push_filter(nde::Filter {
                name: String::new(),
                graph: nde::graph::Graph::identity(),
            });
            notify_filter_lists(global_state);
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
                notify_filter_lists(global_state);
            }
        }
        Message::DeleteFilter(filter_index) => {
            // Unassign filters from events that might have it assigned
            for event in &mut global_state.subtitles.events {
                event.extradata_ids.retain(|id| *id != filter_index);
            }

            // Remove the filter itself
            global_state.subtitles.extradata.remove(filter_index);

            notify_filter_lists(global_state);
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
                    && let Some(pane_state) = global_state.panes.get_mut(source_pane)
                {
                    struct Visitor {
                        previous: nde::graph::PreviousEndpoint,
                        new_dangling_end_position: iced::Point,
                    }
                    impl pane::Visitor for Visitor {
                        fn visit_node_editor(
                            &mut self,
                            node_editor_state: &mut pane::node_editor::State,
                        ) {
                            let new_dangling_source = iced_node_editor::LogicalEndpoint {
                                node_index: self.previous.node_index,
                                role: iced_node_editor::SocketRole::Out,
                                socket_index: self.previous.socket_index,
                            };
                            node_editor_state.dangling_source = Some(new_dangling_source);
                            node_editor_state.dangling_connection =
                                Some(iced_node_editor::Link::from_unordered(
                                    iced_node_editor::Endpoint::Socket(new_dangling_source),
                                    iced_node_editor::Endpoint::Absolute(
                                        self.new_dangling_end_position,
                                    ),
                                ));
                        }
                    }

                    let mut visitor = Visitor {
                        previous,
                        new_dangling_end_position,
                    };
                    pane_state.local.visit(&mut visitor);
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

    iced::Task::none()
}

/// Notifies all entities (like node editor panes) that keep some internal copy of the
/// NDE filter list to update their internal representations
pub(crate) fn notify_selected_events(global_state: &mut super::Samaku) {
    if let Some(active_event) = active_event!(global_state) {
        notify_active_event_text(&mut global_state.panes, active_event, None);
    }
}

pub(crate) fn notify_active_event_text(
    panes: &mut iced::widget::pane_grid::State<pane::State>,
    active_event: &subtitle::Event,
    except_pane: Option<iced::widget::pane_grid::Pane>,
) {
    for (pane, pane_state) in &mut panes.panes {
        if let Some(except_pane) = except_pane
            && *pane == except_pane
        {
            continue;
        }
        pane_state.local.update_active_event_text(active_event);
    }
}

/// Notifies all entities (like node editor panes) that keep some internal copy of the
/// NDE filter list to update their internal representations
pub(crate) fn notify_filter_lists(global_state: &mut super::Samaku) {
    for pane in global_state.panes.panes.values_mut() {
        pane.local
            .update_filter_names(&global_state.subtitles.extradata);
    }
}

/// Notifies all entities (like text editor panes) that keep some internal copy of the
/// styles list to update their internal representations. If `copy_styles` is false, only the
/// selected style will be updated.
pub(crate) fn notify_style_lists(global_state: &mut super::Samaku, copy_styles: bool) {
    let active_event_style_index = active_event!(global_state).map(|event| event.style_index);

    for pane in global_state.panes.panes.values_mut() {
        pane.local.update_style_lists(
            global_state.subtitles.styles.as_slice(),
            copy_styles,
            active_event_style_index,
        );
    }
}

async fn select_file_and_save(data: String) -> anyhow::Result<()> {
    if let Some(handle) = rfd::AsyncFileDialog::new().save_file().await {
        smol::fs::write(handle.path(), data).await?;
    }

    // No file selected
    Ok(())
}
