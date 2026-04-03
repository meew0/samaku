use std::{collections::HashMap, fmt::Display, sync::LazyLock};

use crate::{media, message, model, nde, subtitle, view};

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
        let left_column = left_column(self_pane, global_state, self.selected_style_index);
        let right_column = right_column(global_state, self.selected_style_index);
        let preview = preview(
            global_state,
            self.selected_style_index,
            self.preview_events.iter_events(),
        );

        let content = iced::widget::column![
            preview,
            iced::widget::row![left_column, right_column].height(iced::Length::Fill),
        ]
        .height(iced::Length::Fill);

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

fn left_column(
    self_pane: super::Pane,
    global_state: &'_ crate::Samaku,
    selected_style_index: usize,
) -> iced::widget::Column<'_, message::Message> {
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
        Some(selected_style_index),
        crate::DEFAULT_FONT,
    )
    .width(iced::Length::Fixed(200.0))
    .height(iced::Length::Fixed(200.0));

    let create_button = iced::widget::button(iced::widget::text("Create new"))
        .on_press(message::Message::CreateStyle);
    let delete_button = iced::widget::button(iced::widget::text("Delete")).on_press_maybe(
        // Do not allow deleting the first style
        (selected_style_index != 0).then_some(message::Message::DeleteStyle(selected_style_index)),
    );

    iced::widget::column![
        iced::widget::text("Styles").size(20),
        selection_list,
        iced::widget::row![create_button, delete_button].spacing(5)
    ]
    .spacing(5)
}

fn right_column(
    global_state: &crate::Samaku,
    selected_style_index: usize,
) -> iced::Element<message::Message> {
    let i = selected_style_index;
    let s = &global_state.subtitles.styles[i];

    // ── Name & Font ──────────────────────────────────────────────────────
    let name_input = iced::widget::text_input("Style name", s.name())
        .on_input(move |v| message::Message::SetStyleName(i, v));
    let font_input = iced::widget::text_input("Font name", &s.font_name)
        .on_input(move |v| message::Message::SetStyleFontName(i, v));
    let font_size_input = iced_aw::number_input(&s.font_size, 1.0..=9999.0_f64, move |v| {
        message::Message::SetStyleFontSize(i, v)
    });

    let section_name_font = iced::widget::column![
        section_label("Name & Font"),
        labeled_row("Name", name_input.into()),
        labeled_row("Font", font_input.into()),
        labeled_row("Size", font_size_input.into()),
    ]
    .spacing(4);

    // ── Colours ──────────────────────────────────────────────────────────
    let colour_header = iced::widget::row![
        iced::widget::text("").width(COL_LABEL_W),
        iced::widget::text("R").width(COL_COLOUR_W),
        iced::widget::text("G").width(COL_COLOUR_W),
        iced::widget::text("B").width(COL_COLOUR_W),
        iced::widget::text("Alpha").width(COL_COLOUR_W),
    ]
    .spacing(4);

    let pc = s.primary_colour;
    let sc = s.secondary_colour;
    let bc = s.border_colour;
    let shc = s.shadow_colour;

    let primary_row = colour_input_row(
        "Primary",
        &s.primary_colour.red,
        &s.primary_colour.green,
        &s.primary_colour.blue,
        &s.primary_transparency.0,
        move |r| message::Message::SetStylePrimaryColour(i, nde::tags::Colour { red: r, ..pc }),
        move |g| message::Message::SetStylePrimaryColour(i, nde::tags::Colour { green: g, ..pc }),
        move |b| message::Message::SetStylePrimaryColour(i, nde::tags::Colour { blue: b, ..pc }),
        move |a| message::Message::SetStylePrimaryTransparency(i, nde::tags::Transparency(a)),
    );
    let secondary_row = colour_input_row(
        "Secondary",
        &s.secondary_colour.red,
        &s.secondary_colour.green,
        &s.secondary_colour.blue,
        &s.secondary_transparency.0,
        move |r| message::Message::SetStyleSecondaryColour(i, nde::tags::Colour { red: r, ..sc }),
        move |g| message::Message::SetStyleSecondaryColour(i, nde::tags::Colour { green: g, ..sc }),
        move |b| message::Message::SetStyleSecondaryColour(i, nde::tags::Colour { blue: b, ..sc }),
        move |a| message::Message::SetStyleSecondaryTransparency(i, nde::tags::Transparency(a)),
    );
    let border_colour_row = colour_input_row(
        "Border",
        &s.border_colour.red,
        &s.border_colour.green,
        &s.border_colour.blue,
        &s.border_transparency.0,
        move |r| message::Message::SetStyleBorderColour(i, nde::tags::Colour { red: r, ..bc }),
        move |g| message::Message::SetStyleBorderColour(i, nde::tags::Colour { green: g, ..bc }),
        move |b| message::Message::SetStyleBorderColour(i, nde::tags::Colour { blue: b, ..bc }),
        move |a| message::Message::SetStyleBorderTransparency(i, nde::tags::Transparency(a)),
    );
    let shadow_colour_row = colour_input_row(
        "Shadow",
        &s.shadow_colour.red,
        &s.shadow_colour.green,
        &s.shadow_colour.blue,
        &s.shadow_transparency.0,
        move |r| message::Message::SetStyleShadowColour(i, nde::tags::Colour { red: r, ..shc }),
        move |g| message::Message::SetStyleShadowColour(i, nde::tags::Colour { green: g, ..shc }),
        move |b| message::Message::SetStyleShadowColour(i, nde::tags::Colour { blue: b, ..shc }),
        move |a| message::Message::SetStyleShadowTransparency(i, nde::tags::Transparency(a)),
    );

    let section_colours = iced::widget::column![
        section_label("Colours"),
        colour_header,
        primary_row,
        secondary_row,
        border_colour_row,
        shadow_colour_row,
    ]
    .spacing(4);

    // ── Formatting ───────────────────────────────────────────────────────
    let bold_cb = iced::widget::checkbox(s.bold)
        .label("Bold")
        .on_toggle(move |v| message::Message::SetStyleBold(i, v));
    let italic_cb = iced::widget::checkbox(s.italic)
        .label("Italic")
        .on_toggle(move |v| message::Message::SetStyleItalic(i, v));
    let underline_cb = iced::widget::checkbox(s.underline)
        .label("Underline")
        .on_toggle(move |v| message::Message::SetStyleUnderline(i, v));
    let strike_out_cb = iced::widget::checkbox(s.strike_out)
        .label("Strike-out")
        .on_toggle(move |v| message::Message::SetStyleStrikeOut(i, v));

    let scale_x_input = iced_aw::number_input(&s.scale.x, 0.01..=1000.0_f64, move |v| {
        message::Message::SetStyleScaleX(i, v)
    });
    let scale_y_input = iced_aw::number_input(&s.scale.y, 0.01..=1000.0_f64, move |v| {
        message::Message::SetStyleScaleY(i, v)
    });
    let spacing_input = iced_aw::number_input(&s.spacing, -1000.0..=1000.0_f64, move |v| {
        message::Message::SetStyleSpacing(i, v)
    });
    let angle_input = iced_aw::number_input(&s.angle.0, 0.0..=360.0_f64, move |v| {
        message::Message::SetStyleAngle(i, v)
    });
    let blur_input = iced_aw::number_input(&s.blur, 0.0..=100.0_f64, move |v| {
        message::Message::SetStyleBlur(i, v)
    });

    let section_formatting = iced::widget::column![
        section_label("Formatting"),
        iced::widget::row![bold_cb, italic_cb, underline_cb, strike_out_cb].spacing(12),
        iced::widget::row![
            iced::widget::text("Scale X").width(COL_LABEL_W),
            scale_x_input,
            iced::widget::text("Y").width(20),
            scale_y_input,
        ]
        .spacing(4)
        .align_y(iced::Alignment::Center),
        iced::widget::row![
            iced::widget::text("Spacing").width(COL_LABEL_W),
            spacing_input,
            iced::widget::text("Angle").width(50),
            angle_input,
            iced::widget::text("Blur").width(35),
            blur_input,
        ]
        .spacing(4)
        .align_y(iced::Alignment::Center),
    ]
    .spacing(6);

    // ── Border & Shadow ──────────────────────────────────────────────────
    let border_style_list =
        iced::widget::pick_list(BORDER_STYLES, Some(s.border_style), move |v| {
            message::Message::SetStyleBorderStyle(i, v)
        });
    let border_width_input = iced_aw::number_input(&s.border_width, 0.0..=1000.0_f64, move |v| {
        message::Message::SetStyleBorderWidth(i, v)
    });
    let shadow_dist_input = iced_aw::number_input(&s.shadow_distance, 0.0..=1000.0_f64, move |v| {
        message::Message::SetStyleShadowDistance(i, v)
    });

    let section_border = iced::widget::column![
        section_label("Border & Shadow"),
        labeled_row("Style", border_style_list.into()),
        iced::widget::row![
            iced::widget::text("Width").width(COL_LABEL_W),
            border_width_input,
            iced::widget::text("Distance").width(65),
            shadow_dist_input,
        ]
        .spacing(4)
        .align_y(iced::Alignment::Center),
    ]
    .spacing(4);

    // ── Positioning ──────────────────────────────────────────────────────
    // 3×3 numpad-style alignment grid (7=top-left … 1=bottom-left)
    let alignment_grid = {
        use nde::tags::{Alignment, HorizontalAlignment as H, VerticalAlignment as V};
        let cur = s.alignment;

        // (numpad label, vertical, horizontal)
        let cells: [(u8, V, H); 9] = [
            (7, V::Top, H::Left),
            (8, V::Top, H::Center),
            (9, V::Top, H::Right),
            (4, V::Center, H::Left),
            (5, V::Center, H::Center),
            (6, V::Center, H::Right),
            (1, V::Sub, H::Left),
            (2, V::Sub, H::Center),
            (3, V::Sub, H::Right),
        ];

        let rows: Vec<iced::Element<'_, message::Message>> = cells
            .chunks(3)
            .map(|row_cells| {
                let btns: Vec<iced::Element<'_, message::Message>> = row_cells
                    .iter()
                    .map(|&(label, va, ha)| {
                        let align = Alignment {
                            vertical: va,
                            horizontal: ha,
                        };
                        let selected = cur == align;
                        iced::widget::button(iced::widget::text(label.to_string()).size(13))
                            .style(move |_, _| iced::widget::button::Style {
                                background: Some(
                                    if selected {
                                        crate::style::SAMAKU_PRIMARY
                                    } else {
                                        crate::style::SAMAKU_BACKGROUND_WEAK
                                    }
                                    .into(),
                                ),
                                text_color: crate::style::SAMAKU_TEXT,
                                border: iced::Border::default(),
                                shadow: iced::Shadow::default(),
                                snap: false,
                            })
                            .on_press(message::Message::SetStyleAlignment(i, align))
                            .into()
                    })
                    .collect();
                iced::widget::row(btns).spacing(2).into()
            })
            .collect();

        iced::widget::column(rows).spacing(2)
    };

    let justify_list = iced::widget::pick_list(JUSTIFY_MODES, Some(s.justify), move |v| {
        message::Message::SetStyleJustify(i, v)
    });
    let margin_l_input = iced_aw::number_input(&s.margins.left, 0..=9999_i32, move |v| {
        message::Message::SetStyleMarginLeft(i, v)
    });
    let margin_r_input = iced_aw::number_input(&s.margins.right, 0..=9999_i32, move |v| {
        message::Message::SetStyleMarginRight(i, v)
    });
    let margin_v_input = iced_aw::number_input(&s.margins.vertical, 0..=9999_i32, move |v| {
        message::Message::SetStyleMarginVertical(i, v)
    });

    let section_positioning = iced::widget::column![
        section_label("Positioning"),
        iced::widget::row![
            iced::widget::column![iced::widget::text("Alignment"), alignment_grid,].spacing(4),
            iced::widget::column![
                labeled_row("Justify", justify_list.into()),
                iced::widget::row![
                    iced::widget::text("Margins").width(COL_LABEL_W),
                    iced::widget::text("L").width(12),
                    margin_l_input,
                    iced::widget::text("R").width(12),
                    margin_r_input,
                    iced::widget::text("V").width(12),
                    margin_v_input,
                ]
                .spacing(4)
                .align_y(iced::Alignment::Center),
            ]
            .spacing(6)
            .padding(iced::Padding {
                left: 12.0,
                ..Default::default()
            }),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Start),
    ]
    .spacing(6);

    // ── Assemble ─────────────────────────────────────────────────────────
    let inner = iced::widget::column![
        section_name_font,
        view::separator(),
        section_colours,
        view::separator(),
        section_formatting,
        view::separator(),
        section_border,
        view::separator(),
        section_positioning,
    ]
    .spacing(8)
    .padding(iced::Padding::new(8.0))
    .width(iced::Length::Fill);

    iced::widget::scrollable(inner)
        .height(iced::Length::Fill)
        .width(iced::Length::Fill)
        .into()
}

/// Returns a small bold-ish section header.
fn section_label(label: &str) -> iced::widget::Text<'_> {
    iced::widget::text(label).size(13)
}

/// A labeled row: a fixed-width label on the left followed by any element.
fn labeled_row<'a>(
    label: &'a str,
    content: iced::Element<'a, message::Message>,
) -> iced::Element<'a, message::Message> {
    iced::widget::row![iced::widget::text(label).width(COL_LABEL_W), content]
        .spacing(4)
        .align_y(iced::Alignment::Center)
        .into()
}

/// One row in the colour grid: label + R, G, B, Alpha number inputs.
#[expect(
    clippy::too_many_arguments,
    reason = "colour rows need separate callbacks per channel"
)]
fn colour_input_row<'a>(
    label: &'a str,
    r: &'a u8,
    g: &'a u8,
    b: &'a u8,
    alpha: &'a i32,
    on_r: impl Fn(u8) -> message::Message + Copy + 'static,
    on_g: impl Fn(u8) -> message::Message + Copy + 'static,
    on_b: impl Fn(u8) -> message::Message + Copy + 'static,
    on_a: impl Fn(i32) -> message::Message + Copy + 'static,
) -> iced::Element<'a, message::Message> {
    iced::widget::row![
        iced::widget::text(label).width(COL_LABEL_W),
        iced_aw::number_input(r, 0..=255_u8, on_r).width(COL_COLOUR_W),
        iced_aw::number_input(g, 0..=255_u8, on_g).width(COL_COLOUR_W),
        iced_aw::number_input(b, 0..=255_u8, on_b).width(COL_COLOUR_W),
        iced_aw::number_input(alpha, 0..=255_i32, on_a).width(COL_COLOUR_W),
    ]
    .spacing(4)
    .align_y(iced::Alignment::Center)
    .into()
}

/// Width of the label column used in property rows.
const COL_LABEL_W: iced::Length = iced::Length::Fixed(70.0);
/// Width of each colour-component input.
const COL_COLOUR_W: iced::Length = iced::Length::Fixed(58.0);

const BORDER_STYLES: &[subtitle::BorderStyle] = &[
    subtitle::BorderStyle::Default,
    subtitle::BorderStyle::OpaqueBox,
    subtitle::BorderStyle::Background,
];

const JUSTIFY_MODES: &[subtitle::JustifyMode] = &[
    subtitle::JustifyMode::Auto,
    subtitle::JustifyMode::Left,
    subtitle::JustifyMode::Center,
    subtitle::JustifyMode::Right,
];

fn preview<'a, 'b, 'c>(
    global_state: &'a crate::Samaku,
    selected_style_index: usize,
    preview_events: impl Iterator<Item = &'b subtitle::Event<'c>> + 'b,
) -> iced::Element<'a, message::Message>
where
    'c: 'b,
{
    let selected_style = &global_state.subtitles.styles[selected_style_index];

    // We don't need to actually compile the subtitles here, since the `preview_events` will never use any NDE filters. So we can
    // just pretend they have already been compiled.
    let ass = media::subtitle::OpaqueTrack::from_compiled(
        preview_events,
        std::slice::from_ref(selected_style),
        // Match the script metadata we use globally, except, ignore any extra info (since it never really contains any useful data
        // anyway, and would be costly to clone) and set the playback resolution to a useful value to maximise visibility.
        // TODO: make the playback resolution scale configurable
        &subtitle::ScriptInfo {
            wrap_style: global_state.subtitles.script_info.wrap_style,
            scaled_border_and_shadow: global_state.subtitles.script_info.scaled_border_and_shadow,
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
    view::widget::ImageStack::new(images, view::widget::EmptyProgram)
        .set_image_size_override(iced::Size {
            width: u32::from(PREVIEW_WIDTH),
            height: u32::from(PREVIEW_HEIGHT),
        })
        .into()
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
