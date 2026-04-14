//! Global update logic: update the global state ([`Samaku`] object) based on an incoming message.

use crate::message::Message;
use crate::{action, history, media, message, model, nde, pane, project, subtitle};
use anyhow::Context as _;
use smol::io::AsyncBufReadExt as _;
use std::borrow::Cow;
use std::collections::HashSet;
use std::fmt::Write as _;
use std::mem::replace;

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
///
/// This specific method is primarily concerned with updating the history (undo/redo),
/// the message processing is handed by internal methods.
pub(crate) fn update(global_state: &mut super::Samaku, message: Message) -> iced::Task<Message> {
    // Create a history key, if the message is one that could potentially be undone.
    let mut key = global_state.history.make_key(&message);

    let task = update_direct(global_state, message, &mut key);

    global_state.history.record(key);

    task
}

/// Handle a message without recording it in the history.
pub(crate) fn update_direct(
    global_state: &mut super::Samaku,
    message: Message,
    undo: &mut history::Key,
) -> iced::Task<Message> {
    // Run the internal update method, which does the actual updating of global state.
    let task = update_internal(global_state, message, undo);

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
fn update_internal(
    global_state: &mut super::Samaku,
    message: Message,
    undo: &mut history::Key,
) -> iced::Task<Message> {
    #[expect(
        clippy::match_same_arms,
        reason = "needed in this case to coherently group messages together"
    )]
    match message {
        Message::None => {}
        Message::ModifiersChanged(modifiers) => {
            global_state.modifiers = modifiers;
        }
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
            global_state.toasts.push(toast);
        }
        Message::CloseToast(index) => {
            global_state.toasts.remove(index);
        }
        Message::UpdateToastProgress(id, progress) => {
            global_state.toasts.update_progress(id, progress);
        }
        Message::Undo => {
            let messages = global_state.history.undo();
            // Undo messages need to be processed in reverse order, since batching
            // appends the newest messages at the end.
            let tasks: Vec<iced::Task<Message>> = messages
                .into_iter()
                .rev()
                .map(|message| update_direct(global_state, message, &mut history::Key::Dummy))
                .collect();
            return iced::Task::batch(tasks);
        }
        Message::Redo => {
            let messages = global_state.history.redo();
            // Redo messages need to be processed in forward order
            let tasks: Vec<iced::Task<Message>> = messages
                .into_iter()
                .map(|message| update_direct(global_state, message, &mut history::Key::Dummy))
                .collect();
            return iced::Task::batch(tasks);
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
            action::index_video_and_load(global_state, path_buf);
        }
        Message::VideoIndexed(path_buf, index) => {
            let model::NeverClone(index) = index;
            action::load_video(global_state, path_buf, index);
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
        Message::NewSubtitleFile => {
            action::replace_subtitle_file(global_state, subtitle::File::default());
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
                global_state.toasts.push(model::toast::Toast::message(
                    model::toast::Status::Primary,
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
                global_state.toasts.push(model::toast::Toast::message(
                    model::toast::Status::Primary,
                    "Warning while loading subtitle file".to_owned(),
                    format!("{warning}"),
                ));
            }

            let project_load_result = project::load(global_state);
            if global_state.toasts.anyhow(project_load_result) == Some(true) {
                // Some project metadata was loaded, we might have to perform after-load tasks such as opening linked video/audio files
                return project::after_load(global_state);
            }
        }
        Message::SubtitleParseError(err) => {
            global_state.toasts.push(model::toast::Toast::message(
                model::toast::Status::Danger,
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

            if let Some(data) = global_state.toasts.anyhow(result) {
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
                global_state.toasts.push(model::toast::Toast::message(
                    model::toast::Status::Primary,
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
            for event in global_state.subtitles.events.iter_events_mut() {
                match event.style_index.cmp(&index) {
                    std::cmp::Ordering::Less => {}
                    std::cmp::Ordering::Equal => event.style_index = 0,
                    std::cmp::Ordering::Greater => event.style_index -= 1,
                }
            }
        }
        Message::SetStyleName(index, name) => {
            let current_name = global_state.subtitles.styles[index].name.clone();
            global_state.subtitles.styles.rename(index, name);

            undo.put_instant("Set style name", Message::SetStyleName(index, current_name));
        }
        Message::SetStyleFontName(index, value) => {
            let old = replace(&mut global_state.subtitles.styles[index].font_name, value);

            undo.put_instant("Set style font name", Message::SetStyleFontName(index, old));
        }
        Message::SetStyleFontSize(index, value) => {
            let old = replace(&mut global_state.subtitles.styles[index].font_size, value);

            undo.put_instant("Set style font size", Message::SetStyleFontSize(index, old));
        }
        Message::SetStylePrimaryColour(index, value) => {
            let old = replace(
                &mut global_state.subtitles.styles[index].primary_colour,
                value,
            );

            undo.put_instant(
                "Set style primary color",
                Message::SetStylePrimaryColour(index, old),
            );
        }
        Message::SetStylePrimaryTransparency(index, value) => {
            let old = replace(
                &mut global_state.subtitles.styles[index].primary_transparency,
                value,
            );

            undo.put_instant(
                "Set style primary transparency",
                Message::SetStylePrimaryTransparency(index, old),
            );
        }
        Message::SetStyleSecondaryColour(index, value) => {
            let old = replace(
                &mut global_state.subtitles.styles[index].secondary_colour,
                value,
            );

            undo.put_instant(
                "Set style secondary color",
                Message::SetStyleSecondaryColour(index, old),
            );
        }
        Message::SetStyleSecondaryTransparency(index, value) => {
            let old = replace(
                &mut global_state.subtitles.styles[index].secondary_transparency,
                value,
            );

            undo.put_instant(
                "Set style secondary transparency",
                Message::SetStyleSecondaryTransparency(index, old),
            );
        }
        Message::SetStyleBorderColour(index, value) => {
            let old = replace(
                &mut global_state.subtitles.styles[index].border_colour,
                value,
            );

            undo.put_instant(
                "Set style border color",
                Message::SetStyleBorderColour(index, old),
            );
        }
        Message::SetStyleBorderTransparency(index, value) => {
            let old = replace(
                &mut global_state.subtitles.styles[index].border_transparency,
                value,
            );

            undo.put_instant(
                "Set style border transparency",
                Message::SetStyleBorderTransparency(index, old),
            );
        }
        Message::SetStyleShadowColour(index, value) => {
            let old = replace(
                &mut global_state.subtitles.styles[index].shadow_colour,
                value,
            );

            undo.put_instant(
                "Set style shadow color",
                Message::SetStyleShadowColour(index, old),
            );
        }
        Message::SetStyleShadowTransparency(index, value) => {
            let old = replace(
                &mut global_state.subtitles.styles[index].shadow_transparency,
                value,
            );

            undo.put_instant(
                "Set style shadow transparency",
                Message::SetStyleShadowTransparency(index, old),
            );
        }
        Message::SetStyleBold(index, value) => {
            let old = replace(&mut global_state.subtitles.styles[index].bold, value);

            undo.put_no_batch("Set style bold", Message::SetStyleBold(index, old));
        }
        Message::SetStyleItalic(index, value) => {
            let old = replace(&mut global_state.subtitles.styles[index].italic, value);

            undo.put_no_batch("Set style italic", Message::SetStyleItalic(index, old));
        }
        Message::SetStyleUnderline(index, value) => {
            let old = replace(&mut global_state.subtitles.styles[index].underline, value);

            undo.put_no_batch(
                "Set style underline",
                Message::SetStyleUnderline(index, old),
            );
        }
        Message::SetStyleStrikeOut(index, value) => {
            let old = replace(&mut global_state.subtitles.styles[index].strike_out, value);

            undo.put_no_batch(
                "Set style strike-out",
                Message::SetStyleStrikeOut(index, old),
            );
        }
        Message::SetStyleScaleX(index, value) => {
            let old = replace(&mut global_state.subtitles.styles[index].scale.x, value);

            undo.put_instant("Set style scale X", Message::SetStyleScaleX(index, old));
        }
        Message::SetStyleScaleY(index, value) => {
            let old = replace(&mut global_state.subtitles.styles[index].scale.y, value);

            undo.put_instant("Set style scale Y", Message::SetStyleScaleY(index, old));
        }
        Message::SetStyleSpacing(index, value) => {
            let old = replace(&mut global_state.subtitles.styles[index].spacing, value);

            undo.put_instant("Set style spacing", Message::SetStyleSpacing(index, old));
        }
        Message::SetStyleAngle(index, value) => {
            let old = replace(
                &mut global_state.subtitles.styles[index].angle,
                subtitle::Angle(value),
            );

            undo.put_instant("Set style angle", Message::SetStyleAngle(index, old.0));
        }
        Message::SetStyleBlur(index, value) => {
            let old = replace(&mut global_state.subtitles.styles[index].blur, value);

            undo.put_instant("Set style blur", Message::SetStyleBlur(index, old));
        }
        Message::SetStyleBorderStyle(index, value) => {
            let old = replace(
                &mut global_state.subtitles.styles[index].border_style,
                value,
            );

            undo.put_no_batch(
                "Set style border style",
                Message::SetStyleBorderStyle(index, old),
            );
        }
        Message::SetStyleBorderWidth(index, value) => {
            let old = replace(
                &mut global_state.subtitles.styles[index].border_width,
                value,
            );

            undo.put_instant(
                "Set style border width",
                Message::SetStyleBorderWidth(index, old),
            );
        }
        Message::SetStyleShadowDistance(index, value) => {
            let old = replace(
                &mut global_state.subtitles.styles[index].shadow_distance,
                value,
            );

            undo.put_instant(
                "Set style shadow distance",
                Message::SetStyleShadowDistance(index, old),
            );
        }
        Message::SetStyleAlignment(index, value) => {
            let old = replace(&mut global_state.subtitles.styles[index].alignment, value);

            undo.put_no_batch(
                "Set style alignment",
                Message::SetStyleAlignment(index, old),
            );
        }
        Message::SetStyleMarginLeft(index, value) => {
            let old = replace(
                &mut global_state.subtitles.styles[index].margins.left,
                value,
            );

            undo.put_instant(
                "Set style left margin",
                Message::SetStyleMarginLeft(index, old),
            );
        }
        Message::SetStyleMarginRight(index, value) => {
            let old = replace(
                &mut global_state.subtitles.styles[index].margins.right,
                value,
            );

            undo.put_instant(
                "Set style right margin",
                Message::SetStyleMarginRight(index, old),
            );
        }
        Message::SetStyleMarginVertical(index, value) => {
            let old = replace(
                &mut global_state.subtitles.styles[index].margins.vertical,
                value,
            );

            undo.put_instant(
                "Set style vertical margin",
                Message::SetStyleMarginVertical(index, old),
            );
        }
        Message::SetStyleJustify(index, value) => {
            let old = replace(&mut global_state.subtitles.styles[index].justify, value);

            undo.put_no_batch(
                "Set style justify mode",
                Message::SetStyleJustify(index, old),
            );
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
            let new_index = global_state.subtitles.events.push(new_event.clone());
            let position = global_state.subtitles.events.position(new_index);

            undo.put_no_batch("Add event", Message::DeleteEvents(vec![new_index]));
            undo.override_redo(Message::RestoreEvents(vec![(
                subtitle::Tombstone::new(new_index),
                position,
                new_event,
            )]));
        }
        Message::DeleteEvents(event_indices) => {
            let mut set = HashSet::from_iter(event_indices);
            let removed = global_state.subtitles.events.remove_from_set(&mut set);

            undo.put_no_batch("Delete events", Message::RestoreEvents(removed));
        }
        Message::DeleteSelectedEvents => {
            let selected: Vec<subtitle::EventIndex> = global_state
                .selected_event_indices
                .iter()
                .copied()
                .collect();
            let removed = global_state
                .subtitles
                .events
                .remove_from_set(&mut global_state.selected_event_indices);
            global_state.selected_event_indices.clear();

            undo.put_no_batch("Delete events", Message::RestoreEvents(removed));
            undo.put_no_batch("Delete events", Message::SelectEvents(selected.clone()));
            undo.override_redo(Message::DeleteEvents(selected));
        }
        Message::RestoreEvents(events) => {
            let new_indices: Vec<subtitle::EventIndex> = events
                .into_iter()
                .map(|(tombstone, pos, event)| {
                    global_state.subtitles.events.restore(tombstone, pos, event)
                })
                .collect();

            undo.put_no_batch("Restore events", Message::DeleteEvents(new_indices));
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
        Message::SelectEvents(indices) => {
            global_state.selected_event_indices.extend(indices);
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
            if let Some(event_index) =
                subtitle::EventTrack::active_event_index(&global_state.selected_event_indices)
            {
                let event = &global_state.subtitles.events[event_index];
                global_state.subtitles.events.update_event_times(
                    event_index,
                    new_start_time,
                    event.duration,
                );
            }
        }
        Message::SetActiveEventDuration(new_duration) => {
            if let Some(event_index) =
                subtitle::EventTrack::active_event_index(&global_state.selected_event_indices)
            {
                let event = &global_state.subtitles.events[event_index];
                global_state.subtitles.events.update_event_times(
                    event_index,
                    event.start,
                    new_duration,
                );
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
            global_state
                .subtitles
                .events
                .update_event_times(event_index, start, duration);
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
            for event in global_state.subtitles.events.iter_events_mut() {
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
        Message::DeleteNodes(node_ids) => {
            if let Some(filter) = global_state.subtitles.events.active_nde_filter_mut(
                &global_state.selected_event_indices,
                &mut global_state.subtitles.extradata,
            ) {
                // Create a visitor that will remap selected subtitles on node editor panes.
                struct Visitor(Vec<Option<nde::graph::NodeId>>);
                impl pane::Visitor for Visitor {
                    fn visit_node_editor(
                        &mut self,
                        node_editor_state: &mut pane::node_editor::State,
                    ) {
                        node_editor_state.remap_selected(&self.0);
                    }
                }

                // Delete the nodes (what we actually want to do)
                let mapping = filter.graph.delete_nodes(&node_ids);

                // Remap selected node IDs on all node panes
                let mut visitor = Visitor(mapping);
                for (_, pane_state) in global_state.panes.iter_mut() {
                    pane_state.local.visit(&mut visitor);
                }
            }
        }
        Message::MoveNode(node_id, point) => {
            if let Some(filter) = global_state.subtitles.events.active_nde_filter_mut(
                &global_state.selected_event_indices,
                &mut global_state.subtitles.extradata,
            ) {
                let node = &mut filter.graph.nodes[node_id.0];
                node.position = point;
            }
        }
        Message::MoveNodeGroup(node_ids, delta) => {
            if let Some(filter) = global_state.subtitles.events.active_nde_filter_mut(
                &global_state.selected_event_indices,
                &mut global_state.subtitles.extradata,
            ) {
                for node_id in node_ids {
                    let node = &mut filter.graph.nodes[node_id.0];
                    node.position += delta;
                }
            }
        }
        Message::ConnectNodes(previous, next) => {
            if let Some(filter) = global_state.subtitles.events.active_nde_filter_mut(
                &global_state.selected_event_indices,
                &mut global_state.subtitles.extradata,
            ) {
                filter.graph.connect(previous, next);
            }
        }
        Message::DisconnectNodes(previous, next) => {
            if let Some(filter) = global_state.subtitles.events.active_nde_filter_mut(
                &global_state.selected_event_indices,
                &mut global_state.subtitles.extradata,
            ) {
                let maybe_previous = filter.graph.disconnect(next);
                if let Some(true_previous) = maybe_previous
                    && true_previous.node_index != previous.node_index
                    && true_previous.socket_index != previous.socket_index
                {
                    println!("warning: previous {previous:?} != true_previous {true_previous:?}");
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
                && let Some(node) = filter.graph.nodes.get_mut(reticules.source_node_index.0)
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
/// selected events to update their internal representations.
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
/// NDE filter list to update their internal representations.
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
