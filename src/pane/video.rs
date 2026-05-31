use crate::media::motion;
use crate::{media, message, model, nde, style, subtitle, view};
use glam::DVec2;
use iced::mouse;
use iced::widget::{Action, canvas};
use std::cell::RefCell;
use std::collections::HashSet;

const EXTRA_SCROLL: f32 = 500.0;

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct State {
    show_controls: bool,
    controls_mode: ControlsMode,
    split_at: f32,
    #[serde(skip)]
    blend_box_state: view::widget::blend_box::State,
    limit_to_event: bool,
    track_settings: motion::TrackSettings,
    track_expando_open: bool,
    track_settings_expando_open: bool,
    marker_settings_expando_open: bool,
    zoom: f32,
    #[serde(skip, default = "iced::widget::Id::unique")]
    scroll_id: iced::widget::Id,
    #[serde(skip)]
    scroll_offset: iced::widget::scrollable::AbsoluteOffset,
    #[serde(skip)]
    scroll_viewport_size: iced::Size,
    #[serde(skip, default = "default_true")]
    needs_center_scroll: bool,
    #[serde(skip)]
    cached_video_dims: RefCell<Option<(f32, f32)>>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            show_controls: true,
            controls_mode: ControlsMode::Reticules,
            split_at: 0.8,
            blend_box_state: view::widget::blend_box::State::default(),
            limit_to_event: true,
            track_settings: motion::TrackSettings::default(),
            track_expando_open: true,
            track_settings_expando_open: false,
            marker_settings_expando_open: false,
            zoom: 0.5,
            scroll_id: iced::widget::Id::unique(),
            scroll_offset: iced::widget::scrollable::AbsoluteOffset::default(),
            scroll_viewport_size: iced::Size::ZERO,
            needs_center_scroll: true,
            cached_video_dims: RefCell::new(None),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ControlsMode {
    Reticules,
    MotionTrack,
}

#[derive(Debug, Clone)]
pub enum TrackingOption {
    Model(motion::Model),
    MatchMode(motion::MatchMode),
    PrePass(bool),
    Normalize(bool),
    Channel(motion::Channel, bool),
}

#[typetag::serde(name = "video")]
impl super::LocalState for State {
    fn view<'a>(
        &'a self,
        self_pane: super::Pane,
        global_state: &'a crate::Samaku,
    ) -> super::View<'a> {
        let content = match global_state.actual_frame {
            None => view_empty(),
            Some((frame_number, ref video_frame)) => match global_state.video_metadata.as_ref() {
                None => view_empty(),
                Some(video_metadata) => view_video(
                    self,
                    self_pane,
                    global_state,
                    video_metadata,
                    video_frame,
                    frame_number,
                ),
            },
        };

        super::View {
            title: iced::widget::text("Video").into(),
            content,
        }
    }

    fn update(&mut self, pane_message: message::Pane) -> iced::Task<message::Message> {
        match pane_message {
            message::Pane::VideoSetControlsMode(controls_mode) => {
                self.controls_mode = controls_mode;
                // Since changing controls mode resets the video container,
                // we need to set the scroll position again.
                return iced::widget::operation::scroll_to(
                    self.scroll_id.clone(),
                    self.scroll_offset,
                );
            }
            message::Pane::VideoToggleTrackExpando => {
                self.track_expando_open = !self.track_expando_open;
            }
            message::Pane::VideoToggleTrackSettingsExpando => {
                self.track_settings_expando_open = !self.track_settings_expando_open;
            }
            message::Pane::VideoToggleMarkerSettingsExpando => {
                self.marker_settings_expando_open = !self.marker_settings_expando_open;
            }
            message::Pane::VideoScrolled(offset, viewport_size) => {
                self.scroll_offset = offset;
                self.scroll_viewport_size = viewport_size;
                // On first render the scrollable reports its initial viewport; use that to center.
                if self.needs_center_scroll
                    && viewport_size.width > 0.0
                    && viewport_size.height > 0.0
                {
                    self.needs_center_scroll = false;
                    let (cx, cy) = if let Some((video_w, video_h)) =
                        *self.cached_video_dims.borrow()
                    {
                        let cx = (EXTRA_SCROLL + (video_w - viewport_size.width) / 2.0).max(0.0);
                        let cy = (EXTRA_SCROLL + (video_h - viewport_size.height) / 2.0).max(0.0);
                        (cx, cy)
                    } else {
                        (EXTRA_SCROLL, EXTRA_SCROLL)
                    };
                    self.scroll_offset = iced::widget::scrollable::AbsoluteOffset { x: cx, y: cy };
                    return iced::widget::operation::scroll_to(
                        self.scroll_id.clone(),
                        iced::widget::scrollable::AbsoluteOffset {
                            x: Some(cx),
                            y: Some(cy),
                        },
                    );
                }
            }
            message::Pane::VideoPan(dx, dy) => {
                return iced::widget::operation::scroll_by(
                    self.scroll_id.clone(),
                    iced::widget::scrollable::AbsoluteOffset { x: dx, y: dy },
                );
            }
            message::Pane::VideoZoom(step, cursor_pos) => {
                let old_zoom = self.zoom;
                let new_zoom = (old_zoom * (1.0 + step)).clamp(0.10, 10.0);
                self.zoom = new_zoom;
                let ratio = new_zoom / old_zoom;
                // cursor_pos is the cursor's position within the canvas (ImageStack bounds).
                // Keeping that canvas pixel at the same screen position gives this formula
                // (EXTRA_SCROLL cancels out):
                //   new_scroll = cursor_canvas * (ratio - 1) + old_scroll
                let new_x = cursor_pos
                    .x
                    .mul_add(ratio - 1.0, self.scroll_offset.x)
                    .max(0.0);
                let new_y = cursor_pos
                    .y
                    .mul_add(ratio - 1.0, self.scroll_offset.y)
                    .max(0.0);
                self.scroll_offset =
                    iced::widget::scrollable::AbsoluteOffset { x: new_x, y: new_y };
                return iced::widget::operation::scroll_to(
                    self.scroll_id.clone(),
                    iced::widget::scrollable::AbsoluteOffset {
                        x: Some(new_x),
                        y: Some(new_y),
                    },
                );
            }
            _ => {}
        }

        iced::Task::none()
    }
}

fn view_video<'a>(
    pane_state: &'a State,
    self_pane: super::Pane,
    global_state: &'a crate::Samaku,
    video_metadata: &'a media::VideoMetadata,
    video_frame: &'a iced::widget::image::Handle,
    frame_number: model::FrameNumber,
) -> iced::Element<'a, message::Message> {
    let storage_size = subtitle::Resolution {
        x: video_metadata.width,
        y: video_metadata.height,
    };

    // If we have some subtitles, render them onto the video.
    // Otherwise, just show the frame directly.
    let images = if global_state.subtitles.events.is_empty() {
        vec![view::widget::StackedImage {
            handle: video_frame.clone(),
            x: 0,
            y: 0,
        }]
    } else {
        let context = global_state.compile_context(None);
        render_subtitles(
            &global_state.subtitles,
            context,
            video_metadata,
            storage_size,
            frame_number,
            video_frame,
            &global_state.view,
        )
    };

    // Explicit pixel dimensions driven by zoom level (zoom=1.0 = native resolution).
    // ContentFit::Contain (default) fits the image into the widget; since the widget
    // has the same aspect ratio as the video, there is no letterboxing.
    #[expect(
        clippy::cast_precision_loss,
        reason = "precision loss acceptable for rendering"
    )]
    let video_w = pane_state.zoom * video_metadata.width as f32;
    #[expect(
        clippy::cast_precision_loss,
        reason = "precision loss acceptable for rendering"
    )]
    let video_h = pane_state.zoom * video_metadata.height as f32;

    // Cache dims so the update handler can compute the centering scroll offset.
    *pane_state.cached_video_dims.borrow_mut() = Some((video_w, video_h));

    // Create the canvas program to run (either reticules from a node, or motion tracking controls)
    // and overlay it onto the images, creating an `ImageStack` widget.
    let image_stack: iced::Element<message::Message> = match pane_state.controls_mode {
        ControlsMode::Reticules => view::widget::ImageStack::new(
            images,
            view_reticule_program(global_state, storage_size, self_pane),
        )
        .set_stack_width(iced::Length::Fixed(video_w))
        .set_stack_height(iced::Length::Fixed(video_h))
        .into(),
        ControlsMode::MotionTrack => view::widget::ImageStack::new(
            images,
            view_motion_track_program(global_state, storage_size, frame_number, self_pane),
        )
        .set_stack_width(iced::Length::Fixed(video_w))
        .set_stack_height(iced::Length::Fixed(video_h))
        .into(),
    };

    // Surround the video with a fixed-size padding region so the user can pan around
    // even when zoomed out (the video is smaller than the viewport).  The padding is
    // EXTRA_SCROLL pixels on each side; the image stack is centered inside it.
    let content_w = 2.0_f32.mul_add(EXTRA_SCROLL, video_w);
    let content_h = 2.0_f32.mul_add(EXTRA_SCROLL, video_h);
    let padded_image = iced::widget::container(image_stack)
        .center_x(iced::Length::Fixed(content_w))
        .center_y(iced::Length::Fixed(content_h));

    let scroll_id = pane_state.scroll_id.clone();
    let video_scroll = iced::widget::scrollable(padded_image)
        .id(scroll_id)
        .direction(iced::widget::scrollable::Direction::Both {
            vertical: iced::widget::scrollable::Scrollbar::default(),
            horizontal: iced::widget::scrollable::Scrollbar::default(),
        })
        .on_scroll(move |vp| {
            message::Message::Pane(
                self_pane,
                message::Pane::VideoScrolled(vp.absolute_offset(), vp.bounds().size()),
            )
        })
        .width(iced::Length::Fill)
        .height(iced::Length::Fill);

    let video_container = iced::widget::container(video_scroll)
        .width(iced::Length::Fill)
        .height(iced::Length::Fill)
        .clip(true);

    let split = match pane_state.controls_mode {
        ControlsMode::Reticules => video_container.into(),
        ControlsMode::MotionTrack => {
            let motion_track_controls =
                view_motion_track_controls(pane_state, self_pane, global_state, frame_number);
            let motion_track_scroll = iced::widget::scrollable(motion_track_controls);

            iced::widget::row![
                video_container,
                view::vertical_separator(),
                motion_track_scroll,
            ]
            .into()
        }
    };

    if pane_state.show_controls {
        let bottom_bar = view_bottom_bar(pane_state, self_pane, global_state);
        iced::widget::column![split, view::separator(), bottom_bar].into()
    } else {
        split
    }
}

pub fn render_subtitles<'a>(
    subtitles: &'a subtitle::File,
    mut context: subtitle::compile::Context<'a>,
    video_metadata: &media::VideoMetadata,
    storage_size: subtitle::Resolution,
    num_frame: model::FrameNumber,
    handle: &iced::widget::image::Handle,
    view_state_cell: &RefCell<crate::ViewState>,
) -> Vec<view::widget::StackedImage<iced::widget::image::Handle>> {
    let instant = std::time::Instant::now();
    let current_frame_time = video_metadata.frame_rate.frame_to_ms(num_frame);
    let compiled = subtitles.events.compile_range(
        &subtitles.extradata,
        &mut context,
        subtitle::StartTime(current_frame_time).stab(),
    );
    let elapsed_compile = instant.elapsed();

    let instant2 = std::time::Instant::now();
    let ass = media::subtitle::OpaqueTrack::from_compiled(
        compiled.iter(),
        subtitles.styles.as_slice(),
        &subtitles.script_info,
    );
    let elapsed_copy = instant2.elapsed();

    let instant3 = std::time::Instant::now();
    let stack = {
        let mut view_state = view_state_cell.borrow_mut();
        view_state.subtitle_renderer.render_subtitles_onto_base(
            &ass,
            handle.clone(),
            num_frame,
            video_metadata.frame_rate,
            storage_size, // TODO use the actual frame size here (maybe with responsive?)
            storage_size,
        )
    };
    let elapsed_render = instant3.elapsed();
    println!(
        "Subtitle profiling: compiling {} source events to {} compiled events took {:.2?}, copying them into libass took {:.2?}, rendering them took {:.2?}",
        subtitles.events.len(),
        compiled.len(),
        elapsed_compile,
        elapsed_copy,
        elapsed_render
    );

    stack
}

fn view_reticule_program(
    global_state: &'_ crate::Samaku,
    storage_size: subtitle::Resolution,
    pane: super::Pane,
) -> ReticuleProgram<'_> {
    let (reticule_list, node) = if let Some(ref reticules) = global_state.reticules {
        let node = global_state
            .subtitles
            .extradata
            .get_node(reticules.source_filter_index, reticules.source_node_index)
            .ok();
        (reticules.list.as_slice(), node)
    } else {
        let list: &[model::reticule::Reticule] = &[];
        (list, None)
    };

    ReticuleProgram {
        reticules: reticule_list,
        node,
        storage_size,
        current_frame: global_state.current_frame(),
        pane,
    }
}

fn view_motion_track_program(
    global_state: &crate::Samaku,
    storage_size: subtitle::Resolution,
    frame_number: model::FrameNumber,
    pane: super::Pane,
) -> MotionTrackProgram<'_> {
    MotionTrackProgram {
        tracks: global_state.motion_tracks.stab(frame_number),
        selected_tracks: &global_state.selected_tracks,
        frame: frame_number,
        storage_size,
        modifiers: global_state.modifiers,
        pane,
    }
}

fn view_motion_track_controls<'a>(
    pane_state: &'a State,
    self_pane: super::Pane,
    global_state: &'a crate::Samaku,
    frame_number: model::FrameNumber,
) -> iced::Element<'a, message::Message> {
    let active_track_id_opt = global_state.selected_tracks.active();
    let active_track_data_opt = if let Some(active_track_id) = active_track_id_opt
        && let Some(active_track) = global_state.motion_tracks.get(active_track_id)
    {
        Some((active_track_id, active_track))
    } else {
        None
    };

    let mut column = iced::widget::Column::with_capacity(5);

    column = column.push(view_track_selector(
        pane_state,
        global_state,
        active_track_id_opt,
    ));

    if let Some(active_track_data) = active_track_data_opt {
        column = column.push(view_track_rename(active_track_data));
    }

    if !global_state.selected_tracks.is_empty() {
        let track_buttons = view_track_buttons(pane_state, self_pane, global_state, frame_number);
        column = column.push(view::expando(
            pane_state.track_expando_open,
            self_pane,
            message::Pane::VideoToggleTrackExpando,
            "Track",
            track_buttons,
        ));

        let track_settings = view_track_settings(pane_state, self_pane);
        column = column.push(view::expando(
            pane_state.track_settings_expando_open,
            self_pane,
            message::Pane::VideoToggleTrackSettingsExpando,
            "Tracking settings",
            track_settings,
        ));
    }

    if let Some(active_track_data) = active_track_data_opt
        && let Some(active_marker) = active_track_data.1.get_marker(frame_number)
    {
        let marker_settings =
            view_marker_settings(global_state, active_track_data, frame_number, active_marker);
        column = column.push(view::expando(
            pane_state.marker_settings_expando_open,
            self_pane,
            message::Pane::VideoToggleMarkerSettingsExpando,
            "Marker settings",
            marker_settings,
        ));
    }

    column.width(200.0).spacing(20.0).padding(5.0).into()
}

fn view_track_selector<'a>(
    pane_state: &'a State,
    global_state: &'a crate::Samaku,
    active_track_id_opt: Option<motion::TrackId>,
) -> iced::Element<'a, message::Message> {
    let selection = if let Some(active_track_id) = active_track_id_opt
        && let Some(active_track) = global_state.motion_tracks.get(active_track_id)
    {
        Some(model::NamedEntry {
            id: active_track_id,
            name: model::Named::name(active_track),
        })
    } else {
        None
    };

    let controls_spec = view::widget::BlendBoxControls {
        add_text: "New track",
        add_message: message::Message::CreateTrack,
        unassign_text: "",
        unassign_message: None::<fn(motion::TrackId) -> message::Message>,
        delete_text: "Delete track",
        delete_message: Some(|track| {
            let mut set = HashSet::new();
            set.insert(track);
            message::Message::DeleteTracks(set)
        }),
        _phantom: std::marker::PhantomData,
    };

    let placeholder_text = if global_state.selected_tracks.len() > 1 {
        "Multiple tracks selected"
    } else {
        "Select track"
    };

    view::widget::blend_box_controls(
        &pane_state.blend_box_state,
        &global_state.motion_tracks,
        placeholder_text,
        selection,
        message::Message::SelectOnlyTrack,
        iced::Length::Fill,
        controls_spec,
    )
}

fn view_track_rename(
    active_track_data: (motion::TrackId, &motion::Track),
) -> iced::Element<'_, message::Message> {
    let (active_track_id, active_track) = active_track_data;

    let rename_field = iced::widget::text_input("Track name", model::Named::name(active_track))
        .on_input(move |value| message::Message::SetTrackName(active_track_id, value));

    iced::widget::column![view::section_label("Rename"), rename_field]
        .spacing(5.0)
        .into()
}

fn view_track_buttons<'a>(
    pane_state: &'a State,
    self_pane: super::Pane,
    global_state: &'a crate::Samaku,
    frame_number: model::FrameNumber,
) -> iced::Element<'a, message::Message> {
    // Check if we should allow tracking backward and forward, if we are limited to the current event.
    let (mut allow_backward, mut allow_forward) = (true, true);
    let (mut event_start_frame, mut event_end_frame) = (None, None);
    if pane_state.limit_to_event
        && let Some(active_event) = global_state
            .subtitles
            .events
            .active_event(&global_state.selected_events)
        && let &Some(ref video_metadata) = &global_state.video_metadata
    {
        let start_frame = video_metadata.frame_rate.ms_to_frame(active_event.start.0);
        let end_frame = video_metadata.frame_rate.ms_to_frame(active_event.end().0);
        (event_start_frame, event_end_frame) = (Some(start_frame), Some(end_frame));

        if frame_number < start_frame || frame_number > end_frame {
            (allow_backward, allow_forward) = (false, false);
        } else if frame_number == start_frame {
            (allow_backward, allow_forward) = (false, true);
        } else if frame_number == end_frame {
            (allow_backward, allow_forward) = (true, false);
        } else {
            (allow_backward, allow_forward) = (true, true);
        }
    }

    let backward_frame = view::tooltip(
        view::Icon::BoxArrowInLeft
            .button()
            .on_press_maybe(allow_backward.then_some(
                message::Message::TrackMotionForSelectedTracks(
                    frame_number,
                    motion::Direction::Backward,
                    motion::Target::Frame(frame_number - model::FrameDelta(1)),
                ),
            ))
            .width(iced::Length::Fill),
        "Track backward one frame",
    );
    let backward = view::tooltip(
        view::Icon::ArrowBarLeft
            .button()
            .on_press_maybe(allow_backward.then_some(
                message::Message::TrackMotionForSelectedTracks(
                    frame_number,
                    motion::Direction::Backward,
                    motion::Target::event(pane_state.limit_to_event, event_start_frame),
                ),
            ))
            .width(iced::Length::Fill),
        "Track backward as far as possible",
    );
    let forward = view::tooltip(
        view::Icon::ArrowBarRight
            .button()
            .on_press_maybe(allow_forward.then_some(
                message::Message::TrackMotionForSelectedTracks(
                    frame_number,
                    motion::Direction::Forward,
                    motion::Target::event(pane_state.limit_to_event, event_end_frame),
                ),
            ))
            .width(iced::Length::Fill),
        "Track forward as far as possible",
    );
    let forward_frame = view::tooltip(
        view::Icon::BoxArrowInRight
            .button()
            .on_press_maybe(allow_forward.then_some(
                message::Message::TrackMotionForSelectedTracks(
                    frame_number,
                    motion::Direction::Forward,
                    motion::Target::Frame(frame_number + model::FrameDelta(1)),
                ),
            ))
            .width(iced::Length::Fill),
        "Track forward one frame",
    );

    let buttons = iced::widget::row![backward_frame, backward, forward, forward_frame].spacing(5.0);

    let limit_checkbox = view::tooltip(
        iced::widget::checkbox(pane_state.limit_to_event && event_start_frame.is_some())
            .on_toggle_maybe(event_start_frame.map(move |_| {
                move |new_value| {
                    message::Message::Pane(
                        self_pane,
                        message::Pane::VideoSetLimitToEvent(new_value),
                    )
                }
            }))
            .label("Limit to event"),
        "Limit motion tracking to the currently active event",
    );

    iced::widget::column![buttons, limit_checkbox]
        .spacing(5.0)
        .into()
}

fn view_track_settings(
    pane_state: &State,
    self_pane: super::Pane,
) -> iced::Element<'_, message::Message> {
    const MOTION_MODELS: &[motion::Model] = &[
        motion::Model::Translation,
        motion::Model::TranslationRotation,
        motion::Model::TranslationScale,
        motion::Model::TranslationRotationScale,
        motion::Model::Affine,
        // motion::Model::Homography,
    ];

    const MATCH_MODES: &[motion::MatchMode] =
        &[motion::MatchMode::Key, motion::MatchMode::Previous];

    let motion_model_row = iced::widget::row![
        iced::widget::text("Motion:").width(iced::Length::FillPortion(1)),
        iced::widget::pick_list(
            MOTION_MODELS,
            Some(pane_state.track_settings.model),
            move |new_model| message::Message::Pane(
                self_pane,
                message::Pane::VideoSetTrackingOption(TrackingOption::Model(new_model))
            )
        )
        .width(iced::Length::FillPortion(1)),
    ]
    .align_y(iced::Alignment::Center);

    let match_row = iced::widget::row![
        iced::widget::text("Match:").width(iced::Length::FillPortion(1)),
        iced::widget::pick_list(
            MATCH_MODES,
            Some(pane_state.track_settings.match_mode),
            move |new_match_mode| message::Message::Pane(
                self_pane,
                message::Pane::VideoSetTrackingOption(TrackingOption::MatchMode(new_match_mode))
            )
        )
        .width(iced::Length::FillPortion(1)),
    ]
    .align_y(iced::Alignment::Center);

    let pre_pass_cb = view::tooltip(
        iced::widget::checkbox(pane_state.track_settings.pre_pass)
            .label("Prepass")
            .on_toggle(move |new_value| {
                message::Message::Pane(
                    self_pane,
                    message::Pane::VideoSetTrackingOption(TrackingOption::PrePass(new_value)),
                )
            }),
        "Use a brute-force translation only pre-track before refinement [NYI]",
    );

    let normalize_cb = view::tooltip(
        iced::widget::checkbox(pane_state.track_settings.normalize)
            .label("Normalize")
            .on_toggle(move |new_value| {
                message::Message::Pane(
                    self_pane,
                    message::Pane::VideoSetTrackingOption(TrackingOption::Normalize(new_value)),
                )
            }),
        "Normalize light intensities while tracking (slower) [NYI]",
    );

    let channels_cbs = [
        motion::Channel::Red,
        motion::Channel::Green,
        motion::Channel::Blue,
    ]
    .iter()
    .map(|&channel| {
        iced::widget::checkbox(pane_state.track_settings.channels[channel])
            .label(channel.name())
            .on_toggle(move |new_value| {
                message::Message::Pane(
                    self_pane,
                    message::Pane::VideoSetTrackingOption(TrackingOption::Channel(
                        channel, new_value,
                    )),
                )
            })
            .width(iced::Length::FillPortion(1))
            .into()
    })
    .collect();
    let channels_row = iced::widget::Row::from_vec(channels_cbs).spacing(5.0);

    iced::widget::column![
        motion_model_row,
        match_row,
        pre_pass_cb,
        normalize_cb,
        channels_row,
    ]
    .spacing(5.0)
    .into()
}

fn view_marker_settings<'a>(
    global_state: &'a crate::Samaku,
    active_track_data: (motion::TrackId, &'a motion::Track),
    frame_number: model::FrameNumber,
    active_marker: &'a motion::Marker,
) -> iced::Element<'a, message::Message> {
    type MessageFn = fn(model::Axis, motion::TrackId, model::FrameNumber, f64) -> message::Message;

    let (active_track_id, _) = active_track_data;

    let video_metadata = global_state.video_metadata.as_ref().unwrap();
    let x_bounds = 0.0..=f64::from(video_metadata.width);
    let y_bounds = 0.0..=f64::from(video_metadata.height);
    let (x_bounds_ref, y_bounds_ref) = (&x_bounds, &y_bounds);

    let nd = move |vector: DVec2,
                   axis: model::Axis,
                   bounds: std::ops::RangeInclusive<f64>,
                   message_fn: MessageFn,
                   tooltip: &'static str| {
        view::tooltip(
            view::widget::number_dragger(vector[axis], bounds, move |value| {
                message_fn(axis, active_track_id, frame_number, value)
            })
            .width(iced::Length::FillPortion(1)),
            tooltip,
        )
    };
    let nd_row = move |vector: DVec2,
                       message_fn: MessageFn,
                       x_tooltip: &'static str,
                       y_tooltip: &'static str| {
        iced::widget::row![
            nd(
                vector,
                model::Axis::X,
                x_bounds_ref.clone(),
                message_fn,
                x_tooltip
            ),
            nd(
                vector,
                model::Axis::Y,
                y_bounds_ref.clone(),
                message_fn,
                y_tooltip
            ),
        ]
        .spacing(5.0)
    };

    let position_row = nd_row(
        active_marker.region.center,
        message::Message::SetTrackMarkerCenterCoordinate,
        "X coordinate of marker center",
        "Y coordinate of marker center",
    );

    let offset_row = nd_row(
        active_marker.offset,
        message::Message::SetTrackMarkerOffsetCoordinate,
        "X coordinate of marker offset [NYI]",
        "Y coordinate of marker offset [NYI]",
    );

    let bounding_box = active_marker.region.bounding_box();
    let pattern_area_row = nd_row(
        bounding_box.size,
        message::Message::SetTrackMarkerSizeCoordinate,
        "Width of marker bounding box",
        "Height of marker bounding box",
    );

    let search_area_origin_row = nd_row(
        active_marker.search_area.origin,
        message::Message::SetTrackMarkerSearchAreaOriginCoordinate,
        "X coordinate of search area origin",
        "Y coordinate of search area origin",
    );

    let search_area_size_row = nd_row(
        active_marker.search_area.size,
        message::Message::SetTrackMarkerSearchAreaSizeCoordinate,
        "Width of search area",
        "Height of search area",
    );

    iced::widget::column![
        "Position:",
        position_row,
        "Offset:",
        offset_row,
        "Marker area:",
        pattern_area_row,
        "Search area:",
        search_area_origin_row,
        search_area_size_row
    ]
    .spacing(5.0)
    .into()
}

pub fn frame_number_text(global_state: &crate::Samaku) -> String {
    if let Some(metadata) = global_state.video_metadata.as_ref() {
        let frame_number = global_state
            .shared
            .playback_position
            .current_frame(metadata.frame_rate)
            .0;
        format!("{frame_number}")
    } else {
        "No video loaded".to_owned()
    }
}

fn view_bottom_bar<'a>(
    pane_state: &'a State,
    self_pane: super::Pane,
    global_state: &'a crate::Samaku,
) -> iced::Element<'a, message::Message> {
    let frame_number_text_widget = iced::widget::text(frame_number_text(global_state));
    let reticules_radio = iced::widget::radio(
        "Reticules",
        ControlsMode::Reticules,
        Some(pane_state.controls_mode),
        |mode| message::Message::Pane(self_pane, message::Pane::VideoSetControlsMode(mode)),
    );
    let motion_track_radio = iced::widget::radio(
        "Motion track",
        ControlsMode::MotionTrack,
        Some(pane_state.controls_mode),
        |mode| message::Message::Pane(self_pane, message::Pane::VideoSetControlsMode(mode)),
    );

    iced::widget::container(
        iced::widget::row![
            frame_number_text_widget,
            iced::widget::space::horizontal().width(iced::Length::Fixed(10.0)),
            iced::widget::text("Mode:"),
            iced::widget::space::horizontal().width(iced::Length::Fixed(5.0)),
            reticules_radio,
            iced::widget::space::horizontal().width(iced::Length::Fixed(10.0)),
            motion_track_radio,
            iced::widget::space::horizontal().width(iced::Length::Fixed(5.0)),
        ]
        .spacing(5.0)
        .align_y(iced::Alignment::Center),
    )
    .clip(true)
    .padding(5.0)
    .into()
}

// Elements to display if no video is loaded
fn view_empty<'a>() -> iced::Element<'a, message::Message> {
    let scroll = iced::widget::scrollable(iced::widget::row![iced::widget::text(
        "No video loaded. Press V to load something."
    )]);

    iced::widget::container(scroll)
        .center_x(iced::Length::Fill)
        .center_y(iced::Length::Fill)
        .into()
}

inventory::submit! {
    super::Shell::new(
        "Video",
        || Box::new(State::default())
    )
}

/// Tracks right-click-drag panning state across canvas events.
#[derive(Default)]
enum PanDrag {
    #[default]
    Inactive,
    /// Button pressed; waiting for the first CursorMoved to record a position.
    Started,
    /// Actively panning; holds the last raw OS cursor position.
    Active(iced::Point),
}

struct ReticuleProgram<'a> {
    reticules: &'a [model::reticule::Reticule],
    node: Option<&'a nde::graph::VisualNode>,
    storage_size: subtitle::Resolution,
    current_frame: Option<model::FrameNumber>,
    pane: super::Pane,
}

#[derive(Default)]
struct ReticuleState {
    dragging: Option<model::reticule::Index>,
    drag_offset: iced::Vector,
    pan_drag: PanDrag,
}

impl ReticuleProgram<'_> {
    fn find_hovered_reticule(
        &self,
        mouse_position: iced::Point,
        bounds: iced::Rectangle,
    ) -> Option<(
        model::reticule::Index,
        &model::reticule::Reticule,
        iced::Point,
    )> {
        for (i, reticule) in self.reticules.iter().enumerate().rev() {
            let iced_pos = reticule.iced_position(bounds.size(), self.storage_size);
            if iced_pos.distance(mouse_position) < reticule.radius {
                return Some((model::reticule::Index(i), reticule, iced_pos));
            }
        }

        None
    }
}

impl canvas::Program<message::Message> for ReticuleProgram<'_> {
    type State = ReticuleState;

    fn update(
        &self,
        state: &mut Self::State,
        event: &canvas::Event,
        bounds: iced::Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<Action<message::Message>> {
        if let Some(action) =
            handle_zoom_pan_event(&mut state.pan_drag, self.pane, event, cursor, bounds)
        {
            return Some(action);
        }

        if let Some(position) = cursor.position_in(bounds)
            && let canvas::Event::Mouse(ref mouse_event) = *event
        {
            match *mouse_event {
                mouse::Event::ButtonPressed(mouse::Button::Left) => {
                    if let Some((index, _reticule, iced_pos)) =
                        self.find_hovered_reticule(position, bounds)
                    {
                        state.dragging = Some(index);
                        state.drag_offset = position - iced_pos;
                        return Some(Action::capture());
                    }
                }
                mouse::Event::CursorMoved { .. } => {
                    if let Some(dragging_reticule_index) = state.dragging {
                        return Some(
                            Action::publish(message::Message::UpdateReticulePosition(
                                dragging_reticule_index,
                                model::reticule::Reticule::position_from_iced(
                                    position,
                                    state.drag_offset,
                                    bounds.size(),
                                    self.storage_size,
                                ),
                            ))
                            .and_capture(),
                        );
                    }
                }
                mouse::Event::ButtonReleased(mouse::Button::Left) if state.dragging.is_some() => {
                    state.dragging = None;
                    return Some(Action::capture());
                }
                _ => {}
            }
        }

        None
    }

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &iced::Renderer,
        _theme: &iced::Theme,
        bounds: iced::Rectangle,
        cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());

        if let Some(visual_node) = self.node {
            visual_node.node.draw_reticule_base_layer(
                &mut frame,
                bounds,
                self.storage_size,
                self.current_frame,
                cursor,
            );
        }

        let hovered_reticule_index = cursor
            .position_in(bounds)
            .and_then(|mouse_position| self.find_hovered_reticule(mouse_position, bounds))
            .map(|(i, _, _)| i);

        for (i, reticule) in self.reticules.iter().enumerate() {
            let hovered = hovered_reticule_index.is_some_and(|hovered_index| i == hovered_index.0);
            let center_point = reticule.iced_position(bounds.size(), self.storage_size);
            draw_reticule(&mut frame, center_point, reticule, hovered);
        }

        vec![frame.into_geometry()]
    }

    fn mouse_interaction(
        &self,
        state: &Self::State,
        bounds: iced::Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if state.dragging.is_some() || !matches!(state.pan_drag, PanDrag::Inactive) {
            return mouse::Interaction::Grabbing;
        }

        if let Some(mouse_position) = cursor.position_in(bounds)
            && let Some(_) = self.find_hovered_reticule(mouse_position, bounds)
        {
            return mouse::Interaction::Grab;
        }

        mouse::Interaction::None
    }
}

fn draw_reticule(
    frame: &mut canvas::Frame,
    center_point: iced::Point,
    reticule: &model::reticule::Reticule,
    hovered: bool,
) {
    let alpha_factor: f32 = if hovered { 0.4 } else { 0.2 };

    match reticule.shape {
        model::reticule::Shape::Cross => {
            let rect_top_left =
                center_point - iced::Vector::new(reticule.radius * 0.5, reticule.radius * 0.5);
            let rect_size = iced::Size::new(reticule.radius, reticule.radius);
            frame.fill_rectangle(
                rect_top_left,
                rect_size,
                style::SAMAKU_TEXT.scale_alpha(alpha_factor),
            );

            frame.stroke_rectangle(
                rect_top_left,
                rect_size,
                canvas::Stroke::default()
                    .with_color(style::SAMAKU_BACKGROUND)
                    .with_width(1.0),
            );

            let thin_path = canvas::Path::new(|path| {
                path.move_to(center_point + iced::Vector::new(-reticule.radius, 0.0));
                path.line_to(center_point + iced::Vector::new(reticule.radius, 0.0));
                path.move_to(center_point + iced::Vector::new(0.0, -reticule.radius));
                path.line_to(center_point + iced::Vector::new(0.0, reticule.radius));
            });

            frame.stroke(
                &thin_path,
                canvas::Stroke::default()
                    .with_color(style::SAMAKU_BACKGROUND)
                    .with_width(2.0),
            );

            frame.stroke(
                &thin_path,
                canvas::Stroke::default()
                    .with_color(style::SAMAKU_PRIMARY)
                    .with_width(1.0),
            );
        }
        model::reticule::Shape::CornerTopLeft => {
            draw_corner_reticule(frame, center_point, reticule.radius, 1.0, 1.0, alpha_factor);
        }
        model::reticule::Shape::CornerTopRight => {
            draw_corner_reticule(
                frame,
                center_point,
                reticule.radius,
                -1.0,
                1.0,
                alpha_factor,
            );
        }
        model::reticule::Shape::CornerBottomLeft => {
            draw_corner_reticule(
                frame,
                center_point,
                reticule.radius,
                1.0,
                -1.0,
                alpha_factor,
            );
        }
        model::reticule::Shape::CornerBottomRight => {
            draw_corner_reticule(
                frame,
                center_point,
                reticule.radius,
                -1.0,
                -1.0,
                alpha_factor,
            );
        }
        model::reticule::Shape::Circle => {
            let circle = canvas::Path::circle(center_point, reticule.radius * 0.5);
            frame.fill(&circle, style::SAMAKU_TEXT.scale_alpha(alpha_factor));
            frame.stroke(
                &circle,
                canvas::Stroke::default()
                    .with_color(style::SAMAKU_BACKGROUND)
                    .with_width(2.0),
            );
            frame.stroke(
                &circle,
                canvas::Stroke::default()
                    .with_color(style::SAMAKU_PRIMARY)
                    .with_width(1.0),
            );
        }
    }
}

fn draw_corner_reticule(
    frame: &mut canvas::Frame,
    center_point: iced::Point,
    radius: f32,
    x_sign: f32,
    y_sign: f32,
    alpha_factor: f32,
) {
    let path = canvas::Path::new(|path| {
        path.move_to(center_point + iced::Vector::new(x_sign * radius, 0.0));
        path.line_to(center_point);
        path.line_to(center_point + iced::Vector::new(0.0, y_sign * radius));
    });

    frame.fill(
        &canvas::Path::circle(center_point, radius * 0.5),
        style::SAMAKU_TEXT.scale_alpha(alpha_factor),
    );

    frame.stroke(
        &canvas::Path::circle(center_point, radius * 0.5),
        canvas::Stroke::default()
            .with_color(style::SAMAKU_BACKGROUND)
            .with_width(1.0),
    );

    frame.stroke(
        &path,
        canvas::Stroke::default()
            .with_color(style::SAMAKU_BACKGROUND)
            .with_width(2.5),
    );

    frame.stroke(
        &path,
        canvas::Stroke::default()
            .with_color(style::SAMAKU_PRIMARY)
            .with_width(1.0),
    );
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CornerIndex {
    TopLeft,
    TopRight,
    BottomRight,
    BottomLeft,
}

#[derive(Debug, Clone, Copy)]
enum DragTarget {
    WholeRegion { offset: iced::Vector },
    Corner(CornerIndex),
}

struct MotionTrackProgram<'a> {
    tracks: Vec<(motion::TrackId, &'a motion::Track)>,
    selected_tracks: &'a model::select::Selection<motion::TrackId>,
    frame: model::FrameNumber,
    storage_size: subtitle::Resolution,
    modifiers: iced::keyboard::Modifiers,
    pane: super::Pane,
}

#[derive(Default)]
struct MotionTrackState {
    dragging: Option<(motion::TrackId, model::FrameNumber, DragTarget)>,
    pan_drag: PanDrag,
}

const CORNER_HIT_RADIUS: f32 = 8.0;

impl MotionTrackProgram<'_> {
    fn marker_corners(&self, marker: &motion::Marker, bounds: iced::Rectangle) -> [iced::Point; 4] {
        [
            view::frame_coordinates_to_iced(
                marker.region.top_left,
                bounds.size(),
                self.storage_size,
            ),
            view::frame_coordinates_to_iced(
                marker.region.top_right,
                bounds.size(),
                self.storage_size,
            ),
            view::frame_coordinates_to_iced(
                marker.region.bottom_right,
                bounds.size(),
                self.storage_size,
            ),
            view::frame_coordinates_to_iced(
                marker.region.bottom_left,
                bounds.size(),
                self.storage_size,
            ),
        ]
    }

    fn find_hovered_corner(
        &self,
        mouse_position: iced::Point,
        bounds: iced::Rectangle,
    ) -> Option<(motion::TrackId, CornerIndex)> {
        const CORNER_ORDER: [CornerIndex; 4] = [
            CornerIndex::TopLeft,
            CornerIndex::TopRight,
            CornerIndex::BottomRight,
            CornerIndex::BottomLeft,
        ];
        for &(track_id, track) in &self.tracks {
            if let Some(marker) = track.get_marker(self.frame) {
                for (corner, &iced_pos) in CORNER_ORDER
                    .iter()
                    .zip(self.marker_corners(marker, bounds).iter())
                {
                    if iced_pos.distance(mouse_position) < CORNER_HIT_RADIUS {
                        return Some((track_id, *corner));
                    }
                }
            }
        }
        None
    }

    fn find_hovered_marker(
        &self,
        mouse_position: iced::Point,
        bounds: iced::Rectangle,
    ) -> Option<(motion::TrackId, iced::Point)> {
        for &(track_id, track) in &self.tracks {
            if let Some(marker) = track.get_marker(self.frame) {
                let corners = self.marker_corners(marker, bounds);
                let center = view::frame_coordinates_to_iced(
                    marker.region.center,
                    bounds.size(),
                    self.storage_size,
                );
                if point_in_quad(mouse_position, corners) {
                    return Some((track_id, center));
                }
            }
        }
        None
    }

    fn find_track_marker(&self, track_id: motion::TrackId) -> Option<&motion::Marker> {
        self.tracks
            .iter()
            .find(|item| item.0 == track_id)
            .and_then(|item| item.1.get_marker(self.frame))
    }
}

impl canvas::Program<message::Message> for MotionTrackProgram<'_> {
    type State = MotionTrackState;

    fn update(
        &self,
        state: &mut Self::State,
        event: &canvas::Event,
        bounds: iced::Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<Action<message::Message>> {
        if let Some(action) =
            handle_zoom_pan_event(&mut state.pan_drag, self.pane, event, cursor, bounds)
        {
            return Some(action);
        }

        if let Some(position) = cursor.position_in(bounds)
            && let canvas::Event::Mouse(ref mouse_event) = *event
        {
            match *mouse_event {
                mouse::Event::ButtonPressed(mouse::Button::Left) => {
                    if let Some((track_id, corner)) = self.find_hovered_corner(position, bounds) {
                        state.dragging = Some((track_id, self.frame, DragTarget::Corner(corner)));
                        return Some(
                            Action::publish(message::Message::SelectOnlyTrack(track_id))
                                .and_capture(),
                        );
                    }
                    if let Some((track_id, center_iced)) =
                        self.find_hovered_marker(position, bounds)
                    {
                        let offset = position - center_iced;
                        state.dragging =
                            Some((track_id, self.frame, DragTarget::WholeRegion { offset }));
                        return Some(
                            Action::publish(message::Message::SelectOnlyTrack(track_id))
                                .and_capture(),
                        );
                    }
                }
                mouse::Event::CursorMoved { .. } => {
                    if let Some((track_id, frame, drag_target)) = state.dragging {
                        match drag_target {
                            DragTarget::WholeRegion { offset } => {
                                let new_center = model::reticule::Reticule::position_from_iced(
                                    position,
                                    offset,
                                    bounds.size(),
                                    self.storage_size,
                                );
                                return Some(
                                    Action::publish(message::Message::MoveTrackMarkerRegion(
                                        track_id, frame, new_center,
                                    ))
                                    .and_capture(),
                                );
                            }
                            DragTarget::Corner(corner) => {
                                if let Some(marker) = self.find_track_marker(track_id) {
                                    let frame_pos = model::reticule::Reticule::position_from_iced(
                                        position,
                                        iced::Vector { x: 0.0, y: 0.0 },
                                        bounds.size(),
                                        self.storage_size,
                                    );
                                    let new_region = if self.modifiers.alt() {
                                        rotation_corner_drag(frame_pos, corner, &marker.region)
                                    } else {
                                        scale_corner_drag(
                                            frame_pos,
                                            corner,
                                            &marker.region,
                                            !self.modifiers.shift(),
                                        )
                                    };
                                    return Some(
                                        Action::publish(message::Message::SetTrackMarkerRegion(
                                            track_id, frame, new_region,
                                        ))
                                        .and_capture(),
                                    );
                                }
                            }
                        }
                    }
                }
                mouse::Event::ButtonReleased(mouse::Button::Left) if state.dragging.is_some() => {
                    state.dragging = None;
                    return Some(Action::capture());
                }
                _ => {}
            }
        }

        None
    }

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &iced::Renderer,
        _theme: &iced::Theme,
        bounds: iced::Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        for &(track_id, track) in &self.tracks {
            let selected = self.selected_tracks.contains(track_id);

            if let Some(marker) = track.get_marker(self.frame) {
                draw_marker(
                    &mut frame,
                    selected,
                    marker,
                    bounds.size(),
                    self.storage_size,
                );
            }
        }

        vec![frame.into_geometry()]
    }

    fn mouse_interaction(
        &self,
        state: &Self::State,
        bounds: iced::Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if state.dragging.is_some() || !matches!(state.pan_drag, PanDrag::Inactive) {
            return mouse::Interaction::Grabbing;
        }

        if let Some(mouse_position) = cursor.position_in(bounds) {
            if self.find_hovered_corner(mouse_position, bounds).is_some() {
                return mouse::Interaction::Crosshair;
            }
            if self.find_hovered_marker(mouse_position, bounds).is_some() {
                return mouse::Interaction::Grab;
            }
        }

        mouse::Interaction::None
    }
}

fn handle_zoom_pan_event(
    pan_drag: &mut PanDrag,
    pane: super::Pane,
    event: &canvas::Event,
    cursor: mouse::Cursor,
    bounds: iced::Rectangle,
) -> Option<Action<message::Message>> {
    let canvas::Event::Mouse(ref mouse_event) = *event else {
        return None;
    };
    match *mouse_event {
        mouse::Event::WheelScrolled { delta } => {
            // cursor.position_in(bounds) is correct here: it returns the cursor
            // coordinate within the canvas, accounting for scroll offset. This is
            // exactly what VideoZoom needs for the zoom-to-cursor formula.
            let position = cursor.position_in(bounds)?;
            let step = match delta {
                mouse::ScrollDelta::Lines { y, .. } => y * 0.1,
                mouse::ScrollDelta::Pixels { y, .. } => y / 200.0,
            };
            Some(
                Action::publish(message::Message::Pane(
                    pane,
                    message::Pane::VideoZoom(step, position),
                ))
                .and_capture(),
            )
        }
        mouse::Event::ButtonPressed(mouse::Button::Right) => {
            // Guard: only start panning when the cursor is inside the canvas.
            cursor.position_in(bounds)?;
            // Raw position will be captured on first CursorMoved.
            *pan_drag = PanDrag::Started;
            Some(Action::capture())
        }
        mouse::Event::CursorMoved { position } if !matches!(*pan_drag, PanDrag::Inactive) => {
            // Use the raw OS cursor position from the event (not cursor.position()),
            // which the scrollable translates by the current scroll offset, causing
            // the position to change on every scroll_by even when the cursor is still.
            if let PanDrag::Active(last) = *pan_drag {
                let delta = position - last;
                *pan_drag = PanDrag::Active(position);
                return Some(
                    Action::publish(message::Message::Pane(
                        pane,
                        message::Pane::VideoPan(-delta.x, -delta.y),
                    ))
                    .and_capture(),
                );
            }
            // First CursorMoved after button press — record position, no delta yet.
            *pan_drag = PanDrag::Active(position);
            Some(Action::capture())
        }
        mouse::Event::ButtonReleased(mouse::Button::Right)
            if !matches!(*pan_drag, PanDrag::Inactive) =>
        {
            *pan_drag = PanDrag::Inactive;
            Some(Action::capture())
        }
        _ => None,
    }
}

fn scale_corner_drag(
    target: DVec2,
    corner: CornerIndex,
    region: &motion::Region,
    center_fixed: bool,
) -> motion::Region {
    let e1 = (region.top_right - region.top_left) * 0.5;
    let e2 = (region.bottom_left - region.top_left) * 0.5;

    let Some(e1_hat) = e1.try_normalize() else {
        return *region;
    };
    let Some(e2_hat) = e2.try_normalize() else {
        return *region;
    };

    let (opposite, s1, s2): (DVec2, f64, f64) = match corner {
        CornerIndex::TopLeft => (region.bottom_right, -1.0, -1.0),
        CornerIndex::TopRight => (region.bottom_left, 1.0, -1.0),
        CornerIndex::BottomRight => (region.top_left, 1.0, 1.0),
        CornerIndex::BottomLeft => (region.top_right, -1.0, 1.0),
    };

    // Center-fixed: resize symmetrically around the existing center.
    // Opposite-fixed (shift): the opposite corner stays in place, center moves.
    let new_center = if center_fixed {
        region.center
    } else {
        (target + opposite) * 0.5
    };
    // Corner = new_center + s1*new_e1 + s2*new_e2 = target
    // => s1*new_e1 + s2*new_e2 = target - new_center
    // Solve 2x2: [s1*e1_hat | s2*e2_hat] * [lambda; mu] = rhs
    let rhs = target - new_center;
    let col1 = s1 * e1_hat;
    let col2 = s2 * e2_hat;
    let det = col1.x.mul_add(col2.y, -(col2.x * col1.y));
    if det.abs() < 1e-10 {
        return *region;
    }
    let lambda = rhs.x.mul_add(col2.y, -(col2.x * rhs.y)) / det;
    let mu = col1.x.mul_add(rhs.y, -(rhs.x * col1.y)) / det;
    let new_e1 = lambda * e1_hat;
    let new_e2 = mu * e2_hat;

    motion::Region {
        top_left: new_center - new_e1 - new_e2,
        top_right: new_center + new_e1 - new_e2,
        bottom_right: new_center + new_e1 + new_e2,
        bottom_left: new_center - new_e1 + new_e2,
        center: new_center,
    }
}

fn rotation_corner_drag(
    target: DVec2,
    corner: CornerIndex,
    region: &motion::Region,
) -> motion::Region {
    let corner_pos = match corner {
        CornerIndex::TopLeft => region.top_left,
        CornerIndex::TopRight => region.top_right,
        CornerIndex::BottomRight => region.bottom_right,
        CornerIndex::BottomLeft => region.bottom_left,
    };
    let center = region.center;
    let from = corner_pos - center;
    let to = target - center;
    if from.length_squared() < 1e-20 || to.length_squared() < 1e-20 {
        return *region;
    }
    let angle_delta = to.y.atan2(to.x) - from.y.atan2(from.x);
    let (sin_d, cos_d) = angle_delta.sin_cos();
    let rot = |point: DVec2| -> DVec2 {
        let delta = point - center;
        center
            + DVec2::new(
                delta.x.mul_add(cos_d, -(delta.y * sin_d)),
                delta.x.mul_add(sin_d, delta.y * cos_d),
            )
    };
    motion::Region {
        top_left: rot(region.top_left),
        top_right: rot(region.top_right),
        bottom_right: rot(region.bottom_right),
        bottom_left: rot(region.bottom_left),
        center,
    }
}

fn point_in_quad(point: iced::Point, corners: [iced::Point; 4]) -> bool {
    let mut sign: Option<bool> = None;
    for i in 0..4 {
        let pa = corners[i];
        let pb = corners[(i + 1) % 4];
        let cross = (pb.y - pa.y).mul_add(-(point.x - pa.x), (pb.x - pa.x) * (point.y - pa.y));
        let positive = cross >= 0.0;
        match sign {
            None => sign = Some(positive),
            Some(new_s) if new_s != positive => return false,
            _ => {}
        }
    }
    true
}

fn draw_marker(
    frame: &mut canvas::Frame,
    selected: bool,
    marker: &motion::Marker,
    bounds: iced::Size,
    storage_size: subtitle::Resolution,
) {
    // Search area: dashed outline, only shown when the marker is selected.
    if selected {
        let sa = &marker.search_area;
        let sa_path = canvas::Path::new(|path| {
            path.move_to(view::frame_coordinates_to_iced(
                sa.origin,
                bounds,
                storage_size,
            ));
            path.line_to(view::frame_coordinates_to_iced(
                sa.origin + DVec2::new(sa.size.x, 0.0),
                bounds,
                storage_size,
            ));
            path.line_to(view::frame_coordinates_to_iced(
                sa.origin + sa.size,
                bounds,
                storage_size,
            ));
            path.line_to(view::frame_coordinates_to_iced(
                sa.origin + DVec2::new(0.0, sa.size.y),
                bounds,
                storage_size,
            ));
            path.close();
        });
        let dash_segments = [4.0_f32, 4.0];
        frame.stroke(
            &sa_path,
            canvas::Stroke::default()
                .with_color(style::SAMAKU_BACKGROUND)
                .with_width(2.0),
        );
        frame.stroke(
            &sa_path,
            canvas::Stroke {
                line_dash: canvas::LineDash {
                    segments: &dash_segments,
                    offset: 0,
                },
                ..canvas::Stroke::default()
                    .with_color(style::SAMAKU_TEXT)
                    .with_width(1.0)
            },
        );
    }

    // Marker quad.
    let tl = view::frame_coordinates_to_iced(marker.region.top_left, bounds, storage_size);
    let tr = view::frame_coordinates_to_iced(marker.region.top_right, bounds, storage_size);
    let br = view::frame_coordinates_to_iced(marker.region.bottom_right, bounds, storage_size);
    let bl = view::frame_coordinates_to_iced(marker.region.bottom_left, bounds, storage_size);

    let quad = canvas::Path::new(|path| {
        path.move_to(tl);
        path.line_to(tr);
        path.line_to(br);
        path.line_to(bl);
        path.close();
    });

    let (outer_width, inner_width, inner_color) = if selected {
        (5.0_f32, 3.0_f32, style::SAMAKU_PRIMARY)
    } else {
        (4.0_f32, 2.0_f32, style::SAMAKU_TEXT)
    };

    frame.stroke(
        &quad,
        canvas::Stroke::default()
            .with_color(style::SAMAKU_BACKGROUND)
            .with_width(outer_width),
    );
    frame.stroke(
        &quad,
        canvas::Stroke::default()
            .with_color(inner_color)
            .with_width(inner_width),
    );

    // Center crosshair: green for Key frames, white/yellow for tracked frames.
    let center = view::frame_coordinates_to_iced(marker.region.center, bounds, storage_size);
    draw_marker_center(frame, center, marker.key_state, selected);
}

fn draw_marker_center(
    frame: &mut canvas::Frame,
    center: iced::Point,
    key_state: motion::KeyState,
    selected: bool,
) {
    let radius = 8.0_f32;
    let (fill_color, line_color) = match key_state {
        motion::KeyState::Key => (style::SAMAKU_SUCCESS, style::SAMAKU_SUCCESS),
        motion::KeyState::Tracked => (style::SAMAKU_TEXT, style::SAMAKU_PRIMARY),
    };
    let alpha_factor: f32 = if selected { 0.4 } else { 0.2 };

    let rect_tl = center - iced::Vector::new(radius * 0.5, radius * 0.5);
    let rect_size = iced::Size::new(radius, radius);
    frame.fill_rectangle(rect_tl, rect_size, fill_color.scale_alpha(alpha_factor));
    frame.stroke_rectangle(
        rect_tl,
        rect_size,
        canvas::Stroke::default()
            .with_color(style::SAMAKU_BACKGROUND)
            .with_width(1.0),
    );

    let cross = canvas::Path::new(|path| {
        path.move_to(center + iced::Vector::new(-radius, 0.0));
        path.line_to(center + iced::Vector::new(radius, 0.0));
        path.move_to(center + iced::Vector::new(0.0, -radius));
        path.line_to(center + iced::Vector::new(0.0, radius));
    });
    frame.stroke(
        &cross,
        canvas::Stroke::default()
            .with_color(style::SAMAKU_BACKGROUND)
            .with_width(2.0),
    );
    frame.stroke(
        &cross,
        canvas::Stroke::default()
            .with_color(line_color)
            .with_width(1.0),
    );
}
