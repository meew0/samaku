use std::{collections::HashMap, fmt::Display, sync::LazyLock};

use crate::{media, message, model, subtitle, view};

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct State {
    pub selected_style_index: usize,
    #[serde(skip, default = "default_preview_events")]
    pub preview_events: subtitle::EventTrack,
}

#[typetag::serde(name = "style_editor")]
impl super::LocalState for State {
    fn view<'a>(
        &'a self,
        self_pane: super::Pane,
        global_state: &'a crate::Samaku,
    ) -> super::View<'a> {
        let styles = global_state.subtitles.styles.as_slice();

        // We know that this conversion is sound, since we statically assert that `StyleWrapper` and
        // `Style` have the same size and alignment. However, Rust does not allow us to do this safely,
        // so we have to use unsafe.
        let wrapped_styles: &[StyleWrapper] =
            unsafe { std::slice::from_raw_parts(styles.as_ptr().cast(), styles.len()) };

        let selection_list = iced_aw::widgets::selection_list_with(
            wrapped_styles,
            move |selection_index, _| {
                message::Message::Pane(
                    self_pane,
                    message::Pane::StyleEditorStyleSelected(selection_index),
                )
            },
            14.0,
            2.0,
            iced_aw::style::selection_list::primary,
            Some(self.selected_style_index),
            crate::DEFAULT_FONT,
        )
        .width(iced::Length::Fixed(200.0))
        .height(iced::Length::Fixed(200.0));

        let create_button = iced::widget::button(iced::widget::text("Create new"))
            .on_press(message::Message::CreateStyle);
        let delete_button = iced::widget::button(iced::widget::text("Delete")).on_press_maybe(
            // Do not allow deleting the first style
            (self.selected_style_index != 0)
                .then_some(message::Message::DeleteStyle(self.selected_style_index)),
        );

        let left_column = iced::widget::column![
            iced::widget::text("Styles").size(20),
            selection_list,
            iced::widget::row![create_button, delete_button].spacing(5)
        ]
        .spacing(5);

        // Right column starts here

        let i = self.selected_style_index;
        let selected_style = &global_state.subtitles.styles[i];

        let bold_checkbox = iced::widget::checkbox("Bold", selected_style.bold)
            .on_toggle(move |val| message::Message::SetStyleBold(i, val));
        let italic_checkbox = iced::widget::checkbox("Italic", selected_style.italic)
            .on_toggle(move |val| message::Message::SetStyleItalic(i, val));
        let underline_checkbox = iced::widget::checkbox("Underline", selected_style.underline)
            .on_toggle(move |val| message::Message::SetStyleUnderline(i, val));
        let strike_out_checkbox = iced::widget::checkbox("Strike out", selected_style.strike_out)
            .on_toggle(move |val| message::Message::SetStyleStrikeOut(i, val));

        let flags_row = iced::widget::row![
            bold_checkbox,
            italic_checkbox,
            underline_checkbox,
            strike_out_checkbox
        ];

        let right_column = iced::widget::column![flags_row].spacing(5);

        // Preview starts here

        // We don't need to actually compile the subtitles here, since the `preview_events` will never use any NDE filters. So we can
        // just pretend they have already been compiled.
        let ass = media::subtitle::OpaqueTrack::from_compiled(
            self.preview_events.iter_events(),
            std::slice::from_ref(selected_style),
            // Match the script metadata we use globally, except, ignore any extra info (since it never really contains any useful data
            // anyway, and would be costly to clone) and set the playback resolution to a useful value to maximise visibility.
            // TODO: make the playback resolution scale configurable
            &subtitle::ScriptInfo {
                wrap_style: global_state.subtitles.script_info.wrap_style,
                scaled_border_and_shadow: global_state
                    .subtitles
                    .script_info
                    .scaled_border_and_shadow,
                kerning: global_state.subtitles.script_info.kerning,
                timer: global_state.subtitles.script_info.timer,
                ycbcr_matrix: global_state.subtitles.script_info.ycbcr_matrix,
                playback_resolution: subtitle::Resolution {
                    x: i32::from(PREVIEW_WIDTH) * 2,
                    y: i32::from(PREVIEW_HEIGHT) * 2,
                },
                extra_info: HashMap::new(),
            },
        );

        // Render the preview subtitles
        let images = {
            let mut view_state = global_state.view.borrow_mut();

            let frame_size = subtitle::Resolution {
                x: i32::from(PREVIEW_WIDTH),
                y: i32::from(PREVIEW_HEIGHT),
            };

            view_state.subtitle_renderer.render_subtitles_onto_base(
                &ass,
                TRANSPARENT_IMAGE.clone(),
                model::FrameNumber(0_i32),
                media::FrameRate {
                    numerator: 24,
                    denominator: 1,
                },
                frame_size,
                frame_size,
            )
        };

        // Create an `ImageStack` showing the preview subs
        let image_stack = view::widget::ImageStack::new(images, view::widget::EmptyProgram)
            .set_image_size_override(iced::Size {
                width: u32::from(PREVIEW_WIDTH),
                height: u32::from(PREVIEW_HEIGHT),
            });

        let content =
            iced::widget::column![image_stack, iced::widget::row![left_column, right_column]];

        super::View {
            title: iced::widget::text("Style editor").into(),
            content: iced::widget::container(content)
                .center_x(iced::Length::Fill)
                .center_y(iced::Length::Fill)
                .into(),
        }
    }

    fn update(&mut self, pane_message: message::Pane) -> iced::Task<message::Message> {
        match pane_message {
            message::Pane::StyleEditorStyleSelected(style_index) => {
                self.selected_style_index = style_index;
            }
            _ => (),
        }

        iced::Task::none()
    }

    fn update_style_lists(
        &mut self,
        styles: &[subtitle::Style],
        _copy_styles: bool,
        _active_event_style_index: Option<usize>,
    ) {
        // A style might have been deleted, which might cause the style selected in a
        // style editor pane to no longer exist. In that case, set it to 0 which will
        // always exist.
        if self.selected_style_index >= styles.len() {
            self.selected_style_index = 0;
        }
    }
}

inventory::submit! {
    super::Shell::new(
        "Style editor",
        || Box::new(State::default())
    )
}

fn default_preview_events() -> subtitle::EventTrack {
    vec![subtitle::Event {
        start: subtitle::StartTime(0_i64),
        duration: subtitle::Duration(1000_i64),
        text: "Sphinx of black quartz, judge my vow".into(),
        ..Default::default()
    }]
    .into_iter()
    .collect()
}

impl Default for State {
    fn default() -> Self {
        Self {
            selected_style_index: 0,
            preview_events: default_preview_events(),
        }
    }
}

/// A custom wrapper around `Style` that implements `Display`, `Eq`, and `Hash`,
/// by only referencing the names. We can do this because `StyleList` guarantees that styles have
/// unique names.
#[derive(Clone)]
struct StyleWrapper(subtitle::Style);

static_assertions::assert_eq_size!(StyleWrapper, subtitle::Style);
static_assertions::assert_eq_align!(StyleWrapper, subtitle::Style);

impl Display for StyleWrapper {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}", self.0.name())
    }
}

impl PartialEq for StyleWrapper {
    fn eq(&self, other: &Self) -> bool {
        self.0.name() == other.0.name()
    }
}

impl Eq for StyleWrapper {}

impl std::hash::Hash for StyleWrapper {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.name().hash(state);
    }
}

// An image containing a single transparent pixel, to be used as a base for the subtitle preview
static TRANSPARENT_IMAGE_PIXELS: &[u8] = &[0_u8; 4];
static TRANSPARENT_IMAGE: LazyLock<iced::widget::image::Handle> =
    LazyLock::new(|| iced::widget::image::Handle::from_rgba(1, 1, TRANSPARENT_IMAGE_PIXELS));

// TODO: make this scale to the actual size of the pane, by using `responsive` or the like
static PREVIEW_WIDTH: u16 = 500_u16;
static PREVIEW_HEIGHT: u16 = 100_u16;
