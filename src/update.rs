//! Global update logic: update the global state ([`Samaku`] object) based on an incoming message.

use crate::message::Message;
use crate::{action, history, media, message, model, nde, pane, project, subtitle};
use anyhow::Context as _;
use smol::io::AsyncBufReadExt as _;
use std::borrow::Cow;
use std::collections::HashSet;
use std::fmt::Write as _;
use std::mem::replace;

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
    update_internal(global_state, message, undo)
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
        Message::Batch(messages) => {
            let tasks: Vec<iced::Task<Message>> = messages
                .into_iter()
                .map(|batch_message| update(global_state, batch_message))
                .collect();
            return iced::Task::batch(tasks);
        }
        Message::ModifiersChanged(modifiers) => {
            global_state.modifiers = modifiers;
        }
        Message::SplitPane(axis) => {
            if let Some(focused_pane) = global_state.focus {
                let result =
                    global_state
                        .panes
                        .split(axis, focused_pane, pane::State::unassigned());

                if let Some((new_pane, _)) = result {
                    global_state.focus = Some(new_pane);
                }
            }
        }
        Message::ClosePane => {
            if let Some(focused_pane) = global_state.focus
                && global_state.panes.get(focused_pane).is_some()
                && let Some((_, sibling)) = global_state.panes.close(focused_pane)
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
            }
        }
        Message::SetFocusedPaneType(constructor) => {
            if let Some(focused_pane) = global_state.focus
                && let Some(focused_pane_state) = global_state.panes.get_mut(focused_pane)
            {
                *focused_pane_state = pane::State::new(constructor());

                notify_selected_events(global_state);
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
                .map(|rewind_message| {
                    update_direct(global_state, rewind_message, &mut history::Key::Dummy)
                })
                .collect();
            return iced::Task::batch(tasks);
        }
        Message::Redo => {
            let messages = global_state.history.redo();
            // Redo messages need to be processed in forward order
            let tasks: Vec<iced::Task<Message>> = messages
                .into_iter()
                .map(|replay_message| {
                    update_direct(global_state, replay_message, &mut history::Key::Dummy)
                })
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
        Message::VideoIndexed(path_buf, index_never_clone) => {
            let model::NeverClone(index) = index_never_clone;
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
            global_state.history.clear();
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
            global_state.history.clear();
            let opaque = media::subtitle::OpaqueTrack::parse(&content);
            let (new_file, leftover) = subtitle::File::from_opaque(&opaque);

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
            global_state.history.clear();

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
                Some(global_state.compile_context(None)),
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
            if let Some(ref video_metadata) = global_state.video_metadata {
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
            let (index, old_style) = global_state.subtitles.styles.insert(new_style.clone());

            assert!(old_style.is_none(), "Duplicate style was somehow created");
            undo.put_no_batch("Create style", Message::DeleteStyle(index));
            undo.override_redo(Message::RestoreStyle(index, new_style, HashSet::new()));
        }
        Message::DeleteStyle(index) => {
            let (style, shift) = global_state.subtitles.styles.remove(index);

            // Update style references in events: assign the default style to all events that had
            // the removed style assigned, and decrement style indices of applicable events (with
            // existing style indices > the removed index)
            let mut collect = HashSet::new();
            global_state
                .subtitles
                .events
                .shift_styles(&shift, &mut collect);

            undo.put_no_batch("Delete style", Message::RestoreStyle(index, style, collect));
        }
        Message::RestoreStyle(index, style, mut collect) => {
            let shift = global_state.subtitles.styles.restore(index, style);
            global_state
                .subtitles
                .events
                .shift_styles(&shift, &mut collect);

            undo.put_no_batch("Restore style", Message::DeleteStyle(index));
        }
        Message::SetStyleName(index, name) => {
            let current_name = global_state.subtitles.styles[index].name.clone();
            let renamed = global_state.subtitles.styles.rename(index, name);

            if let Some(new_name) = renamed {
                global_state.toasts.info(
                    "Duplicate style name",
                    format!("Style names must be unique. Name was changed to: {new_name}"),
                );
            }

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

            let mut to_delete = HashSet::new();
            to_delete.insert(new_index);
            undo.put_no_batch("Add event", Message::DeleteEvents(to_delete));
            undo.override_redo(Message::RestoreEvents(
                vec![(subtitle::Tombstone::new(new_index), position, new_event)],
                model::select::Selection::default(),
            ));
        }
        Message::DeleteEvents(set) => {
            let removed = global_state.subtitles.events.remove_from_set(&set);
            let deselected = global_state
                .selected_events
                .deselect_all(set.iter().copied());
            notify_selected_events(global_state);

            undo.put_no_batch("Delete events", Message::RestoreEvents(removed, deselected));
        }
        Message::DeleteSelectedEvents => {
            let selection = global_state.selected_events.clear();
            let removed = global_state
                .subtitles
                .events
                .remove_from_set(&selection.indices);
            notify_selected_events(global_state);

            let selected = selection.indices.clone();
            undo.put_no_batch("Delete events", Message::RestoreEvents(removed, selection));
            undo.override_redo(Message::DeleteEvents(selected));
        }
        Message::RestoreEvents(events, selection) => {
            let new_indices: HashSet<subtitle::EventIndex> = events
                .into_iter()
                .map(|(tombstone, pos, event)| {
                    global_state.subtitles.events.restore(tombstone, pos, event)
                })
                .collect();
            global_state.selected_events.select_from(&selection);
            notify_selected_events(global_state);

            undo.put_no_batch("Restore events", Message::DeleteEvents(new_indices));
        }
        Message::ToggleEventSelection(index) => {
            let old_last = global_state.selected_events.last;
            let previously_selected = if global_state.selected_events.contains(index) {
                global_state.selected_events.deselect(index);
                true
            } else {
                global_state.selected_events.select(index);
                false
            };
            notify_selected_events(global_state);

            undo.put_incremental(
                "Toggle event selection",
                Message::SetEventSelectionSingle(index, previously_selected, old_last),
            );
            undo.override_redo(Message::SetEventSelectionSingle(
                index,
                !previously_selected,
                global_state.selected_events.last,
            ));
        }
        Message::GroupSelectEvents(index_1, index_2, keep_previous) => {
            let n_1 = global_state.subtitles.events.position(index_1);
            let n_2 = global_state.subtitles.events.position(index_2);
            let (n_first, n_last) = (n_1.min(n_2), n_1.max(n_2));
            let events_to_select = global_state
                .subtitles
                .events
                .iter_range_in_order(n_first..(n_last + 1));

            if keep_previous {
                // Select the group in addition to the currently selected ones
                let (selected, old_last) =
                    global_state.selected_events.select_all(events_to_select);
                notify_selected_events(global_state);

                undo.put_instant(
                    "Select multiple events",
                    Message::DeselectEvents(selected, old_last),
                );
            } else {
                // Select only the group
                let mut new_selection =
                    model::select::Selection::from_indices(events_to_select.collect());
                new_selection.last = Some(index_2);
                let old_selection = replace(&mut global_state.selected_events, new_selection);
                notify_selected_events(global_state);

                undo.put_instant(
                    "Select multiple events",
                    Message::SetEventSelection(old_selection),
                );
            }
        }
        Message::SetEventSelectionSingle(index, state, last) => {
            let (old_state, old_last) = global_state.selected_events.set_single(index, state, last);
            notify_selected_events(global_state);

            undo.put_instant(
                "Set event selection (single)",
                Message::SetEventSelectionSingle(index, old_state, old_last),
            );
        }
        Message::SelectOnlyEvent(index) => {
            let old = global_state.selected_events.clear();
            global_state.selected_events.select(index);
            notify_selected_events(global_state);

            undo.put_instant("Select event", Message::SetEventSelection(old));
        }
        Message::SetEventSelection(new_selected_events) => {
            let old = replace(&mut global_state.selected_events, new_selected_events);
            notify_selected_events(global_state);

            undo.put_instant("Select events", Message::SetEventSelection(old));
        }
        Message::DeselectEvents(to_deselect, old_last) => {
            global_state
                .selected_events
                .deselect_all(to_deselect.into_iter());
            global_state.selected_events.last = old_last;
            notify_selected_events(global_state);

            // This message does not need to be undone.
        }
        Message::SelectAllEvents => {
            let new_selection = model::select::Selection::from_indices(
                global_state.subtitles.events.iter_indices().collect(),
            );
            let old_selection = replace(&mut global_state.selected_events, new_selection);
            notify_selected_events(global_state);

            undo.put_no_batch(
                "Select all events",
                Message::SetEventSelection(old_selection),
            );
        }
        Message::MultiEditEventText(new_text) => {
            let old = new_text
                .apply_accessor(&mut global_state.subtitles.events, |event| &mut event.text);
            notify_selected_events(global_state);
            undo.put_instant("Set event text", Message::MultiEditEventText(old));
        }
        Message::MultiEditEventActor(new_actor) => {
            let old = new_actor
                .apply_accessor(&mut global_state.subtitles.events, |event| &mut event.actor);
            notify_selected_events(global_state);
            undo.put_instant("Set event actor", Message::MultiEditEventActor(old));
        }
        Message::MultiEditEventEffect(new_effect) => {
            let old = new_effect.apply_accessor(&mut global_state.subtitles.events, |event| {
                &mut event.effect
            });
            notify_selected_events(global_state);
            undo.put_instant("Set event effect", Message::MultiEditEventEffect(old));
        }
        Message::MultiEditEventStartTime(edit) => {
            let old = edit.apply_function(
                &mut global_state.subtitles.events,
                |event_track, event_index, new_start_time| {
                    let duration = event_track[event_index].duration;
                    let (old, _) =
                        event_track.update_event_times(event_index, new_start_time, duration);
                    old
                },
            );
            notify_selected_events(global_state);
            undo.put_instant(
                "Set event start time",
                Message::MultiEditEventStartTime(old),
            );
        }
        Message::MultiEditEventDuration(edit) => {
            let old = edit.apply_function(
                &mut global_state.subtitles.events,
                |event_track, event_index, new_duration| {
                    let start_time = event_track[event_index].start;
                    let (_, old) =
                        event_track.update_event_times(event_index, start_time, new_duration);
                    old
                },
            );
            notify_selected_events(global_state);
            undo.put_instant("Set event duration", Message::MultiEditEventDuration(old));
        }
        Message::MultiEditEventStyleIndex(new_style_index) => {
            let old = new_style_index.apply_accessor(&mut global_state.subtitles.events, |event| {
                &mut event.style_index
            });
            notify_selected_events(global_state);
            undo.put_no_batch(
                "Set event style index",
                Message::MultiEditEventStyleIndex(old),
            );
        }
        Message::MultiEditEventLayerIndex(new_layer_index) => {
            let old = new_layer_index.apply_accessor(&mut global_state.subtitles.events, |event| {
                &mut event.layer_index
            });
            notify_selected_events(global_state);
            undo.put_instant(
                "Set event layer index",
                Message::MultiEditEventLayerIndex(old),
            );
        }
        Message::MultiEditEventType(new_type) => {
            let old = new_type.apply_accessor(&mut global_state.subtitles.events, |event| {
                &mut event.event_type
            });
            notify_selected_events(global_state);
            undo.put_no_batch("Set event type", Message::MultiEditEventType(old));
        }
        Message::SetEventStartTimeAndDuration(event_index, start, duration) => {
            let (old_start, old_duration) =
                global_state
                    .subtitles
                    .events
                    .update_event_times(event_index, start, duration);
            notify_selected_events(global_state);
            undo.put_instant(
                "Set event timing",
                Message::SetEventStartTimeAndDuration(event_index, old_start, old_duration),
            );
        }
        Message::TextEditorActionPerformed(pane, action_multi_edit) => {
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
                        let new_text_option =
                            text_editor_state.perform(self.action.take().unwrap());

                        if let Some(new_text) = new_text_option {
                            self.new_text = Some(new_text);
                        }
                    }
                }

                let (action, multi_edit) = action_multi_edit.unwrap_value(());

                let mut visitor = Visitor {
                    action: Some(action),
                    new_text: None,
                };

                pane_state.local.visit(&mut visitor);

                // Check if the text has changed, and if it has, update the affected events.
                if let Some(new_text) = visitor.new_text {
                    let ((), new_multi_edit) = multi_edit.unwrap_value(Cow::Owned(new_text));
                    let old = new_multi_edit
                        .clone()
                        .apply_accessor(&mut global_state.subtitles.events, |event| {
                            &mut event.text
                        });
                    undo.put_instant("Edit event text", Message::MultiEditEventText(old));
                    undo.override_redo(Message::MultiEditEventText(new_multi_edit));

                    // Notify all other text editors
                    notify_selected_events(global_state);
                }
            }
        }
        Message::CreateEmptyFilterAndAssignToSelected => {
            let filter = nde::Filter {
                name: String::new(),
                graph: nde::graph::Graph::identity(),
            };
            let filter_index = global_state.subtitles.extradata.push_filter(filter.clone());

            let mut old_selected = Vec::with_capacity(global_state.selected_events.len());
            for selected_event_index in &global_state.selected_events {
                old_selected.push((
                    selected_event_index,
                    global_state.subtitles.events[selected_event_index]
                        .assign_nde_filter(filter_index, &global_state.subtitles.extradata),
                ));
            }

            undo.put_no_batch("Create filter", Message::DeleteFilter(filter_index));
            undo.put_no_batch(
                "Create filter and assign",
                Message::MultiAssignFiltersToEvents(old_selected),
            );
            undo.override_redo(Message::RestoreFilter(
                filter_index,
                filter,
                global_state.selected_events.indices.clone(),
            ));
        }
        Message::AssignFilterToEvents(filter_index, events) => {
            let mut old = Vec::with_capacity(events.len());
            for &event_index in &events {
                old.push((
                    event_index,
                    global_state.subtitles.events[event_index]
                        .assign_nde_filter(filter_index, &global_state.subtitles.extradata),
                ));
            }

            undo.put_no_batch(
                "Assign filter to events",
                Message::MultiAssignFiltersToEvents(old),
            );
        }
        Message::AssignFilterToSelectedEvents(filter_index) => {
            let mut old = Vec::with_capacity(global_state.selected_events.len());
            for selected_event_index in &global_state.selected_events {
                old.push((
                    selected_event_index,
                    global_state.subtitles.events[selected_event_index]
                        .assign_nde_filter(filter_index, &global_state.subtitles.extradata),
                ));
            }

            undo.put_no_batch(
                "Assign filter to events",
                Message::MultiAssignFiltersToEvents(old),
            );
            undo.override_redo(Message::AssignFilterToEvents(
                filter_index,
                global_state.selected_events.indices.clone(),
            ));
        }
        Message::UnassignFilterFromEvents(filter_index, events) => {
            let mut affected_events = HashSet::with_capacity(events.len());
            for event_index in events {
                if global_state.subtitles.events[event_index]
                    .unassign_nde_filter_by_id(filter_index)
                {
                    affected_events.insert(event_index);
                }
            }

            undo.put_no_batch(
                "Unassign filter from events",
                Message::AssignFilterToEvents(filter_index, affected_events),
            );
        }
        Message::UnassignFilterFromSelectedEvents(filter_index) => {
            let mut affected_events = HashSet::new();
            for selected_event_index in &global_state.selected_events {
                if global_state.subtitles.events[selected_event_index]
                    .unassign_nde_filter_by_id(filter_index)
                {
                    affected_events.insert(selected_event_index);
                }
            }

            undo.put_no_batch(
                "Unassign filter from events",
                Message::AssignFilterToEvents(filter_index, affected_events),
            );
            undo.override_redo(Message::UnassignFilterFromEvents(
                filter_index,
                global_state.selected_events.indices.clone(),
            ));
        }
        Message::MultiAssignFiltersToEvents(filters_by_event) => {
            for (event_index, filter_id_opt) in filters_by_event {
                if let Some(filter_id) = filter_id_opt {
                    global_state.subtitles.events[event_index]
                        .assign_nde_filter(filter_id, &global_state.subtitles.extradata);
                } else {
                    global_state.subtitles.events[event_index]
                        .unassign_nde_filter(&global_state.subtitles.extradata);
                }
            }

            // No undo/redo for now.
        }
        Message::SetFilterGraph(filter_index, new_graph) => {
            // Create a visitor that will clear selected nodes on node editor panes,
            // so there are no invalid node references in case the node indices have changed.
            struct Visitor;
            impl pane::Visitor for Visitor {
                fn visit_node_editor(&mut self, node_editor_state: &mut pane::node_editor::State) {
                    node_editor_state.clear_selected();
                }
            }

            let old_graph = replace(
                &mut global_state.subtitles.extradata[filter_index]
                    .assert_filter_mut()
                    .graph,
                new_graph,
            );

            // Remap selected node IDs on all node panes
            let mut visitor = Visitor;
            for (_, pane_state) in global_state.panes.iter_mut() {
                pane_state.local.visit(&mut visitor);
            }

            // Clear reticules, just like we cleared the node selection,
            // since the reticules may have referred to a now-invalid node.
            global_state.reticules = None;

            undo.put_no_batch(
                "Set filter graph",
                Message::SetFilterGraph(filter_index, old_graph),
            );
        }
        Message::SetFilterName(filter_index, new_name) => {
            let entry = &mut global_state.subtitles.extradata[filter_index];
            let filter = entry.assert_filter_mut();
            let old_name = replace(&mut filter.name, new_name);

            undo.put_instant(
                "Set filter name",
                Message::SetFilterName(filter_index, old_name),
            );
        }
        Message::DeleteFilter(filter_index) => {
            // Unassign filters from events that might have it assigned
            let mut assigned_events = HashSet::new();
            for (index, event) in global_state.subtitles.events.enumerate_events_mut() {
                if event.unassign_nde_filter_by_id(filter_index) {
                    assigned_events.insert(index);
                }
            }

            // Remove the filter itself
            let removed = global_state.subtitles.extradata.remove(filter_index);

            if let Some(subtitle::ExtradataEntry::NdeFilter(filter)) = removed {
                if let Some(ref mut reticules) = global_state.reticules
                    && reticules.source_filter_index == filter_index
                {
                    global_state.reticules = None;
                }

                undo.put_no_batch(
                    "Delete filter",
                    Message::RestoreFilter(filter_index, filter, assigned_events),
                );
            } else {
                panic!("Tried to remove NDE filter, but instead found: {removed:?}");
            }
        }
        Message::RestoreFilter(filter_index, filter, assigned_events) => {
            global_state
                .subtitles
                .extradata
                .insert_filter(filter_index, filter);
            for event_index in assigned_events {
                global_state.subtitles.events[event_index]
                    .assign_nde_filter(filter_index, &global_state.subtitles.extradata);
            }

            undo.put_no_batch("Restore filter", Message::DeleteFilter(filter_index));
        }
        Message::AddNode(filter_id, node_constructor) => {
            let filter = global_state.subtitles.extradata[filter_id].assert_filter_mut();
            let old_graph = filter.graph.clone();

            let visual_node = nde::graph::VisualNode {
                node: node_constructor(),
                position: iced::Point::new(0.0, 0.0),
            };
            filter.graph.nodes.push(visual_node);

            undo.put_no_batch("Add node", Message::SetFilterGraph(filter_id, old_graph));
        }
        Message::DeleteNodes(filter_id, node_ids) => {
            // Create a visitor that will remap selected nodes on node editor panes.
            struct Visitor(Vec<Option<nde::graph::NodeId>>);
            impl pane::Visitor for Visitor {
                fn visit_node_editor(&mut self, node_editor_state: &mut pane::node_editor::State) {
                    node_editor_state.remap_selected(&self.0);
                }
            }

            let filter = global_state.subtitles.extradata[filter_id].assert_filter_mut();
            let old_graph = filter.graph.clone();

            // Delete the nodes (what we actually want to do)
            let mapping = filter.graph.delete_nodes(&node_ids);

            // Remap reticules
            if let Some(ref mut reticules) = global_state.reticules
                && let Some(ref new_node_index) = mapping[reticules.source_node_index.0]
            {
                reticules.source_node_index = *new_node_index;
            } else {
                global_state.reticules = None;
            }

            // Remap selected node IDs on all node panes
            let mut visitor = Visitor(mapping);
            for (_, pane_state) in global_state.panes.iter_mut() {
                pane_state.local.visit(&mut visitor);
            }

            // TODO: make this and `AddNodes` incremental at some point rather than replacing the entire filter.
            // This will be a bit annoying since node IDs have to be remapped in a complex way.
            undo.put_no_batch(
                "Delete nodes",
                Message::SetFilterGraph(filter_id, old_graph),
            );
        }
        Message::MoveNode(filter_id, node_id, point) => {
            let filter = global_state.subtitles.extradata[filter_id].assert_filter_mut();
            let node = &mut filter.graph.nodes[node_id.0];
            let old_point = replace(&mut node.position, point);

            undo.put_instant(
                "Move node",
                Message::MoveNode(filter_id, node_id, old_point),
            );
        }
        Message::MoveNodeGroup(filter_id, node_ids, delta) => {
            let filter = global_state.subtitles.extradata[filter_id].assert_filter_mut();
            for node_id in &node_ids {
                let node = &mut filter.graph.nodes[node_id.0];
                node.position += delta;
            }

            undo.put_incremental(
                "Move nodes",
                Message::MoveNodeGroup(filter_id, node_ids, -delta),
            );
        }
        Message::ConnectNodes(filter_id, previous, next) => {
            let filter = global_state.subtitles.extradata[filter_id].assert_filter_mut();
            let old_previous = filter.graph.connect(previous, next);

            undo.put_no_batch(
                "Connect nodes",
                Message::SetNodeConnection(filter_id, old_previous, next),
            );
        }
        Message::DisconnectNodes(filter_id, previous, next) => {
            let filter = global_state.subtitles.extradata[filter_id].assert_filter_mut();
            let maybe_previous = filter.graph.disconnect(next);
            if let Some(true_previous) = maybe_previous
                && true_previous.node_index != previous.node_index
                && true_previous.socket_index != previous.socket_index
            {
                println!("warning: previous {previous:?} != true_previous {true_previous:?}");
            }

            undo.put_no_batch(
                "Disconnect nodes",
                Message::SetNodeConnection(filter_id, maybe_previous, next),
            );
        }
        Message::SetNodeConnection(filter_id, previous, next) => {
            let filter = global_state.subtitles.extradata[filter_id].assert_filter_mut();
            let maybe_previous = filter.graph.set_connection(previous, next);

            undo.put_no_batch(
                "Set node connection state",
                Message::SetNodeConnection(filter_id, maybe_previous, next),
            );
        }
        Message::ActivateNodes(filter_id, nodes) => {
            if nodes.len() == 1 {
                let frame_rate = global_state.frame_rate();
                let layout_resolution = global_state.effective_layout_resolution();

                let filter = global_state.subtitles.extradata[filter_id].assert_filter_mut();
                let node_id = nodes[0];
                let node = &mut filter.graph.nodes[node_id.0];
                let active_event = global_state
                    .subtitles
                    .events
                    .active_event(&global_state.selected_events);
                // We need to construct this manually since we mutably borrow `global_state` above
                // TODO: refactor this to be more ergonomic?
                let context = subtitle::compile::Context {
                    frame_rate,
                    source_event: active_event,
                    styles: &global_state.subtitles.styles,
                    playback_resolution: global_state.subtitles.script_info.playback_resolution,
                    layout_resolution,
                };
                let reticule_list = node.node.reticule_activate(&context);

                global_state.reticules = if reticule_list.is_empty() {
                    None
                } else {
                    Some(model::reticule::Reticules {
                        list: reticule_list,
                        source_filter_index: filter_id,
                        source_node_index: node_id,
                    })
                }
            } else {
                global_state.reticules = None;
            }
        }
        Message::UpdateReticulePosition(index, position) => {
            if let Some(ref mut reticules) = global_state.reticules {
                let result = global_state
                    .subtitles
                    .extradata
                    .reticule_update(reticules, index, position);

                if let Some(old_position) = global_state.toasts.anyhow(result) {
                    // TODO: this undo logic is vulnerable to the list of reticules changing in the meantime,
                    // since reticule setting is not covered by undo/redo.
                    // This is only a minor concern, the user would have to act in a very specific way
                    // to observe aberrant behaviour. Nevertheless it's worth noting here in case
                    // it ever becomes an actual problem.
                    undo.put_incremental(
                        "Move reticule",
                        Message::UpdateReticulePosition(index, old_position),
                    );
                }
            }
        }
        Message::TrackMotionForNode(filter_index, node_index, initial_region) => {
            if let Some(video_metadata) = global_state.video_metadata.as_ref() {
                let current_frame = global_state.current_frame().unwrap(); // video is loaded

                // Update the node's cached track to put the marker it requested at the
                // position of the current frame.
                // The node can't do this itself, because it does not know the number of
                // the current frame.
                let result = global_state
                    .subtitles
                    .extradata
                    .update_node(
                        filter_index,
                        node_index,
                        message::Node::MotionTrackUpdate(current_frame, initial_region),
                    )
                    .context("Failed to dispatch message to node");
                global_state.toasts.anyhow(result);

                if let Some(event) = global_state
                    .subtitles
                    .events
                    .active_event(&global_state.selected_events)
                {
                    global_state.workers.emit_track_motion_for_node(
                        filter_index,
                        node_index,
                        initial_region,
                        current_frame,
                        video_metadata.frame_rate.ms_to_frame(event.end().0),
                    );
                }
            }
        }
        Message::Node(filter_index, node_index, node_message) => {
            let result = global_state
                .subtitles
                .extradata
                .update_node(filter_index, node_index, node_message)
                .context("Failed to dispatch message to node");
            global_state.toasts.anyhow(result);

            // If this node is the active reticule source, re-activate to pick up any structural
            // changes to the reticule list (e.g. a mode toggle that changes how many handles exist).
            let is_reticule_source = global_state.reticules.as_ref().is_some_and(|reticules| {
                reticules.source_filter_index == filter_index
                    && reticules.source_node_index == node_index
            });
            if is_reticule_source {
                let new_list = {
                    let frame_rate = global_state.frame_rate();
                    let layout_resolution = global_state.effective_layout_resolution();

                    let filter = global_state.subtitles.extradata[filter_index].assert_filter_mut();
                    let active_event = global_state
                        .subtitles
                        .events
                        .active_event(&global_state.selected_events);
                    // We need to construct this manually since we mutably borrow `global_state` above
                    let context = subtitle::compile::Context {
                        frame_rate,
                        source_event: active_event,
                        styles: &global_state.subtitles.styles,
                        playback_resolution: global_state.subtitles.script_info.playback_resolution,
                        layout_resolution,
                    };
                    filter.graph.nodes[node_index.0]
                        .node
                        .reticule_activate(&context)
                };
                if new_list.is_empty() {
                    global_state.reticules = None;
                } else if let Some(ref mut reticules) = global_state.reticules {
                    reticules.list = new_list;
                } else {
                    // Reticules were already cleared concurrently; nothing to update.
                }
            }
        }
        Message::CreateTrack => {
            let origin_frame = global_state
                .current_frame()
                .expect("video should be loaded");

            let marker = media::motion::Marker::default();
            let track = media::motion::Track::new(origin_frame, marker, "New track".to_owned());

            let new_id = global_state.motion_tracks.add(track);

            global_state.selected_tracks.clear();
            global_state.selected_tracks.select(new_id);
        }
        Message::DeleteTrack(track_id) => {
            todo!();
        }
        Message::SetTrackName(track_id, name) => {
            if let Some(track) = global_state.motion_tracks.get_mut(track_id) {
                let old_name = replace(&mut track.name, name);

                undo.put_instant("Set track name", Message::SetTrackName(track_id, old_name));
            }
        }
        Message::SetTrackMarker(track_id, frame, new_marker) => {
            if let Some(track) = global_state.motion_tracks.get_mut(track_id)
                && let Some(marker) = track.get_marker_mut(frame)
            {
                let old_marker = replace(marker, new_marker);
                undo.put_instant(
                    "Set motion track marker",
                    Message::SetTrackMarker(track_id, frame, old_marker),
                );
            }
        }
        Message::MoveTrackMarkerRegion(track_id, frame, new_center) => {
            if let Some(track) = global_state.motion_tracks.get_mut(track_id)
                && let Some(marker) = track.get_marker_mut(frame)
            {
                let old_center = marker.region.center;
                let delta = new_center - marker.region.center;
                marker.move_delta(delta);
                undo.put_instant(
                    "Move motion track marker",
                    Message::MoveTrackMarkerRegion(track_id, frame, old_center),
                );
            }
        }
        Message::SetTrackMarkerRegion(track_id, frame, new_region) => {
            if let Some(track) = global_state.motion_tracks.get_mut(track_id)
                && let Some(marker) = track.get_marker_mut(frame)
            {
                let old_marker = marker.clone();
                marker.update_region(new_region);
                undo.put_instant(
                    "Update motion track marker",
                    Message::SetTrackMarker(track_id, frame, old_marker),
                );
            }
        }
        Message::SetTrackMarkerCenterCoordinate(axis, track_id, frame, new_value) => {
            if let Some(track) = global_state.motion_tracks.get_mut(track_id)
                && let Some(marker) = track.get_marker_mut(frame)
            {
                let old_center = marker.region.center;
                marker.move_delta(axis.vector(new_value - old_center[axis]));
                undo.put_instant(
                    "Move motion track marker",
                    Message::MoveTrackMarkerRegion(track_id, frame, old_center),
                );
            }
        }
        Message::SetTrackMarkerOffsetCoordinate(_axis, _track_id, _frame, _new_value) => {
            // TODO implement offset tracking
        }
        Message::SetTrackMarkerSizeCoordinate(axis, track_id, frame, new_value) => {
            if let Some(track) = global_state.motion_tracks.get_mut(track_id)
                && let Some(marker) = track.get_marker_mut(frame)
            {
                let old_marker = marker.clone();
                let bounding_box = marker.region.bounding_box();
                let mut new_size = bounding_box.size;
                new_size[axis] = new_value;
                marker.update_region(marker.region.scale(new_size / bounding_box.size));
                undo.put_instant(
                    "Scale motion track marker",
                    Message::SetTrackMarker(track_id, frame, old_marker),
                );
            }
        }
        Message::SetTrackMarkerSearchAreaOriginCoordinate(axis, track_id, frame, new_value) => {
            if let Some(track) = global_state.motion_tracks.get_mut(track_id)
                && let Some(marker) = track.get_marker_mut(frame)
            {
                let old_marker = marker.clone();
                let mut new_search_area = marker.search_area;
                new_search_area.origin[axis] = new_value;
                marker.update_search_area(new_search_area);
                undo.put_instant(
                    "Move motion track search area",
                    Message::SetTrackMarker(track_id, frame, old_marker),
                );
            }
        }
        Message::SetTrackMarkerSearchAreaSizeCoordinate(axis, track_id, frame, new_value) => {
            if let Some(track) = global_state.motion_tracks.get_mut(track_id)
                && let Some(marker) = track.get_marker_mut(frame)
            {
                let old_marker = marker.clone();
                let mut new_search_area = marker.search_area;
                let old_value = new_search_area.size[axis];
                new_search_area.size[axis] = new_value;
                new_search_area.origin[axis] -= (new_value - old_value) / 2.0;
                marker.update_search_area(new_search_area);
                undo.put_instant(
                    "Resize motion track search area",
                    Message::SetTrackMarker(track_id, frame, old_marker),
                );
            }
        }
        Message::TrackMotionForSelectedTracks(origin_frame, direction, target) => {
            todo!();
        }
        Message::ToggleTrackSelection(track_id) => {
            todo!();
        }
        Message::SetTrackSelectionSingle(track_id, state, last) => {
            todo!();
        }
        Message::SelectOnlyTrack(track_id) => {
            let old = global_state.selected_tracks.clear();
            global_state.selected_tracks.select(track_id);
            notify_selected_events(global_state);

            undo.put_instant("Select event", Message::SetTrackSelection(old));
        }
        Message::SetTrackSelection(new_selected_tracks) => {
            todo!();
        }
        Message::DeselectTracks(to_deselect, old_last) => {
            todo!();
        }
        Message::SelectAllTracks => {
            todo!();
        }
    }

    iced::Task::none()
}

/// Notifies all entities (like node and text editor panes) that keep some internal copy of the
/// selected events to update their internal representations.
pub(crate) fn notify_selected_events(global_state: &mut super::Samaku) {
    for pane in global_state.panes.panes.values_mut() {
        pane.local.update_selected_events(
            &global_state.selected_events,
            &global_state.subtitles.events,
        );
    }
}

pub(crate) fn notify_selected_tracks(global_state: &mut super::Samaku) {
    // TODO, if even necessary?
}

async fn select_file_and_save(data: String) -> anyhow::Result<()> {
    if let Some(handle) = rfd::AsyncFileDialog::new().save_file().await {
        smol::fs::write(handle.path(), data).await?;
    }

    // No file selected
    Ok(())
}
