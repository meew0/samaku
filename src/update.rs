//! Global update logic: update the global state ([`Samaku`] object) based on an incoming message.

use smol::io::AsyncBufReadExt;
use std::borrow::Cow;

use crate::message::Message;
use crate::{media, message, nde, pane, subtitle, view};

#[allow(clippy::too_many_lines)]
pub fn update(global_state: &mut super::Samaku, message: Message) -> iced::Command<Message> {
    #[allow(clippy::match_same_arms)]
    match message {
        Message::None => {}
        Message::SplitPane(axis) => {
            if let Some(pane) = global_state.focus {
                let result = global_state
                    .panes
                    .split(axis, &pane, pane::State::Unassigned);

                if let Some((pane, _)) = result {
                    global_state.focus = Some(pane);
                }
            }
        }
        Message::ClosePane => {
            if let Some(pane) = global_state.focus {
                if global_state.panes.get(&pane).is_some() {
                    if let Some((_, sibling)) = global_state.panes.close(&pane) {
                        global_state.focus = Some(sibling);
                    }
                }
            }
        }
        Message::FocusPane(pane) => global_state.focus = Some(pane),
        Message::DragPane(iced::widget::pane_grid::DragEvent::Dropped { pane, target }) => {
            global_state.panes.drop(&pane, target);
        }
        Message::DragPane(_) => {}
        Message::ResizePane(iced::widget::pane_grid::ResizeEvent { split, ratio }) => {
            global_state.panes.resize(&split, ratio);
        }
        Message::SetPaneState(pane, new_state) => {
            if let Some(pane_state) = global_state.panes.get_mut(&pane) {
                *pane_state = *new_state;
            }
        }
        Message::Pane(pane, pane_message) => {
            if let Some(pane_state) = global_state.panes.get_mut(&pane) {
                return pane::dispatch_update(pane_state, pane_message);
            }
        }
        Message::FocusedPane(pane_message) => {
            if let Some(pane) = global_state.focus {
                if let Some(pane_state) = global_state.panes.get_mut(&pane) {
                    return pane::dispatch_update(pane_state, pane_message);
                }
            }
        }
        Message::Toast(toast) => {
            global_state.toasts.push(toast);
        }
        Message::CloseToast(index) => {
            global_state.toasts.remove(index);
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
            global_state.subtitles = subtitle::File {
                events: opaque.to_event_track(),
                styles: opaque.styles(),
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
                        Err(io_err) => Err(subtitle::parse::Error::IoError(io_err)),
                    },
                    None => Err(subtitle::parse::Error::NoFileSelected),
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
                Ok(ass_file) => global_state.subtitles = *ass_file,
                Err(err) => {
                    global_state.toasts.push(view::toast::Toast {
                        title: "Error while loading subtitle file".to_string(),
                        body: err.to_string(),
                        status: view::toast::Status::Danger,
                    });
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
                global_state.toasts.push(view::toast::Toast {
                    title: "Warning".to_string(),
                    body: format!("Exporting subtitles requires a loaded video for exact results. (Assuming {} fps)", f64::from(global_state.frame_rate())),
                    status: view::toast::Status::Primary,
                });
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
                text: Cow::Owned("Sphinx of black quartz, judge my vow".to_string()),
                actor: Cow::Owned(String::new()),
                effect: Cow::Owned(String::new()),
                event_type: subtitle::EventType::Dialogue,
                extradata_ids: vec![],
            };
            global_state.subtitles.events.push(new_event);
        }
        Message::SelectEvent(index) => global_state.active_event_index = Some(index),
        Message::SetActiveEventText(new_text) => {
            if let Some(event) = global_state
                .subtitles
                .events
                .active_event_mut(global_state.active_event_index)
            {
                event.text = Cow::Owned(new_text);
            }
        }
        Message::CreateEmptyFilter => {
            global_state.subtitles.extradata.push_filter(nde::Filter {
                name: String::new(),
                graph: nde::graph::Graph::identity(),
            });
            global_state.update_filter_lists();
        }
        Message::AssignFilterToActiveEvent(filter_index) => {
            if let Some(active_event) = global_state
                .active_event_index
                .map(|index| &mut global_state.subtitles.events[index])
            {
                active_event.assign_nde_filter(filter_index, &global_state.subtitles.extradata);
            }
        }
        Message::UnassignFilterFromActiveEvent => {
            if let Some(active_event) = global_state
                .active_event_index
                .map(|index| &mut global_state.subtitles.events[index])
            {
                active_event.unassign_nde_filter(&global_state.subtitles.extradata);
            }
        }
        Message::SetActiveFilterName(new_name) => {
            if let Some(filter) = global_state.subtitles.events.active_nde_filter_mut(
                global_state.active_event_index,
                &mut global_state.subtitles.extradata,
            ) {
                filter.name = new_name;
                global_state.update_filter_lists();
            }
        }
        Message::DeleteFilter(_filter_index) => {
            todo!()
        }
        Message::AddNode(node_constructor) => {
            if let Some(filter) = global_state.subtitles.events.active_nde_filter_mut(
                global_state.active_event_index,
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
                global_state.active_event_index,
                &mut global_state.subtitles.extradata,
            ) {
                let node = &mut filter.graph.nodes[node_index];
                node.position = iced::Point::new(node.position.x + x, node.position.y + y);
            }
        }
        Message::ConnectNodes(link) => {
            if let Some(filter) = global_state.subtitles.events.active_nde_filter_mut(
                global_state.active_event_index,
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
                global_state.active_event_index,
                &mut global_state.subtitles.extradata,
            ) {
                let maybe_previous = filter.graph.disconnect(nde::graph::NextEndpoint {
                    node_index: endpoint.node_index,
                    socket_index: endpoint.socket_index,
                });

                if let Some(previous) = maybe_previous {
                    if let Some(pane::State::NodeEditor(node_editor_state)) =
                        global_state.panes.get_mut(&source_pane)
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
        Message::SetReticules(reticules) => {
            global_state.reticules = Some(reticules);
        }
        Message::UpdateReticulePosition(index, position) => {
            if let Some(reticules) = &mut global_state.reticules {
                if let Some(filter) = global_state.subtitles.events.active_nde_filter_mut(
                    global_state.active_event_index,
                    &mut global_state.subtitles.extradata,
                ) {
                    if let Some(node) = filter.graph.nodes.get_mut(reticules.source_node_index) {
                        node.node.reticule_update(reticules, index, position);
                    }
                }
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
                    global_state.active_event_index,
                    &mut global_state.subtitles.extradata,
                    node_index,
                    message::Node::MotionTrackUpdate(current_frame, initial_region),
                );

                if let Some(event) = global_state
                    .subtitles
                    .events
                    .active_event(global_state.active_event_index)
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
                global_state.active_event_index,
                &mut global_state.subtitles.extradata,
                node_index,
                node_message,
            );
        }
    }

    iced::Command::none()
}
