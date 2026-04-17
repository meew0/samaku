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
            iced::widget::container(preview).center_x(iced::Length::Fill),
            iced::widget::row![left_column, right_column],
        ]
        .spacing(8);

        super::View {
            title: iced::widget::text("Style editor").into(),
            content: iced::widget::scrollable(content)
                .direction(iced::widget::scrollable::Direction::Vertical(
                    iced::widget::scrollable::Scrollbar::default(),
                ))
                .width(iced::Length::Fill)
                .height(iced::Length::Fill)
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

    fn update_style_lists(&mut self, styles: &[subtitle::Style], _copy_styles: bool) {
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
    global_state: &'_ crate::Samaku,
    selected_style_index: usize,
) -> iced::Element<'_, message::Message> {
    let i = selected_style_index;
    let style = &global_state.subtitles.styles[i];

    let inner = iced::widget::column![
        section_name_font(i, style),
        view::separator(),
        section_colours(i, style),
        view::separator(),
        section_formatting(i, style),
        view::separator(),
        section_border_shadow(i, style),
        view::separator(),
        section_positioning(i, style),
    ]
    .spacing(8)
    .padding(iced::Padding::new(8.0))
    .width(iced::Length::Fill);

    inner.into()
}

fn section_name_font(i: usize, style: &subtitle::Style) -> iced::Element<'_, message::Message> {
    let name_input = iced::widget::text_input("Style name", style.name())
        .on_input(move |value| message::Message::SetStyleName(i, value));
    let font_input = iced::widget::text_input("Font name", &style.font_name)
        .on_input(move |value| message::Message::SetStyleFontName(i, value));
    let font_size_input = iced_aw::number_input(&style.font_size, 1.0..=9999.0_f64, move |value| {
        message::Message::SetStyleFontSize(i, value)
    });

    iced::widget::column![
        section_label("Name & Font"),
        labeled_row("Name", name_input.into()),
        labeled_row("Font", font_input.into()),
        labeled_row("Size", font_size_input.into()),
    ]
    .spacing(4)
    .into()
}

fn section_colours(i: usize, style: &subtitle::Style) -> iced::Element<'_, message::Message> {
    let colour_header = iced::widget::row![
        iced::widget::text("").width(COL_LABEL_W),
        iced::widget::text("R").width(COL_COLOUR_W),
        iced::widget::text("G").width(COL_COLOUR_W),
        iced::widget::text("B").width(COL_COLOUR_W),
        iced::widget::text("Alpha").width(COL_COLOUR_W),
    ]
    .spacing(4);

    let pc = style.primary_colour;
    let sc = style.secondary_colour;
    let bc = style.border_colour;
    let shc = style.shadow_colour;

    let primary_row = colour_input_row(
        "Primary",
        &style.primary_colour.red,
        &style.primary_colour.green,
        &style.primary_colour.blue,
        &style.primary_transparency.0,
        move |red| message::Message::SetStylePrimaryColour(i, nde::tags::Colour { red, ..pc }),
        move |green| message::Message::SetStylePrimaryColour(i, nde::tags::Colour { green, ..pc }),
        move |blue| message::Message::SetStylePrimaryColour(i, nde::tags::Colour { blue, ..pc }),
        move |alpha| {
            message::Message::SetStylePrimaryTransparency(i, nde::tags::Transparency(alpha))
        },
    );
    let secondary_row = colour_input_row(
        "Secondary",
        &style.secondary_colour.red,
        &style.secondary_colour.green,
        &style.secondary_colour.blue,
        &style.secondary_transparency.0,
        move |red| message::Message::SetStyleSecondaryColour(i, nde::tags::Colour { red, ..sc }),
        move |green| {
            message::Message::SetStyleSecondaryColour(i, nde::tags::Colour { green, ..sc })
        },
        move |blue| message::Message::SetStyleSecondaryColour(i, nde::tags::Colour { blue, ..sc }),
        move |alpha| {
            message::Message::SetStyleSecondaryTransparency(i, nde::tags::Transparency(alpha))
        },
    );
    let border_colour_row = colour_input_row(
        "Border",
        &style.border_colour.red,
        &style.border_colour.green,
        &style.border_colour.blue,
        &style.border_transparency.0,
        move |red| message::Message::SetStyleBorderColour(i, nde::tags::Colour { red, ..bc }),
        move |green| message::Message::SetStyleBorderColour(i, nde::tags::Colour { green, ..bc }),
        move |blue| message::Message::SetStyleBorderColour(i, nde::tags::Colour { blue, ..bc }),
        move |alpha| {
            message::Message::SetStyleBorderTransparency(i, nde::tags::Transparency(alpha))
        },
    );
    let shadow_colour_row = colour_input_row(
        "Shadow",
        &style.shadow_colour.red,
        &style.shadow_colour.green,
        &style.shadow_colour.blue,
        &style.shadow_transparency.0,
        move |red| message::Message::SetStyleShadowColour(i, nde::tags::Colour { red, ..shc }),
        move |green| message::Message::SetStyleShadowColour(i, nde::tags::Colour { green, ..shc }),
        move |blue| message::Message::SetStyleShadowColour(i, nde::tags::Colour { blue, ..shc }),
        move |alpha| {
            message::Message::SetStyleShadowTransparency(i, nde::tags::Transparency(alpha))
        },
    );

    iced::widget::column![
        section_label("Colours"),
        colour_header,
        primary_row,
        secondary_row,
        border_colour_row,
        shadow_colour_row,
    ]
    .spacing(4)
    .into()
}

#[expect(
    clippy::similar_names,
    reason = "symmetric naming is intentional for symmetric controls"
)]
fn section_formatting(i: usize, style: &subtitle::Style) -> iced::Element<'_, message::Message> {
    let bold_cb = iced::widget::checkbox(style.bold)
        .label("Bold")
        .on_toggle(move |value| message::Message::SetStyleBold(i, value));
    let italic_cb = iced::widget::checkbox(style.italic)
        .label("Italic")
        .on_toggle(move |value| message::Message::SetStyleItalic(i, value));
    let underline_cb = iced::widget::checkbox(style.underline)
        .label("Underline")
        .on_toggle(move |value| message::Message::SetStyleUnderline(i, value));
    let strike_out_cb = iced::widget::checkbox(style.strike_out)
        .label("Strike-out")
        .on_toggle(move |value| message::Message::SetStyleStrikeOut(i, value));

    let scale_x_input = iced_aw::number_input(&style.scale.x, 0.01..=1000.0_f64, move |value| {
        message::Message::SetStyleScaleX(i, value)
    })
    .step(0.01_f64);
    let scale_y_input = iced_aw::number_input(&style.scale.y, 0.01..=1000.0_f64, move |value| {
        message::Message::SetStyleScaleY(i, value)
    })
    .step(0.01_f64);
    let spacing_input = iced_aw::number_input(&style.spacing, -1000.0..=1000.0_f64, move |value| {
        message::Message::SetStyleSpacing(i, value)
    });
    let angle_input = iced_aw::number_input(&style.angle.0, 0.0..=360.0_f64, move |value| {
        message::Message::SetStyleAngle(i, value)
    });
    let blur_input = iced_aw::number_input(&style.blur, 0.0..=100.0_f64, move |value| {
        message::Message::SetStyleBlur(i, value)
    });

    iced::widget::column![
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
    .spacing(6)
    .into()
}

fn section_border_shadow(i: usize, style: &subtitle::Style) -> iced::Element<'_, message::Message> {
    let border_style_list =
        iced::widget::pick_list(BORDER_STYLES, Some(style.border_style), move |value| {
            message::Message::SetStyleBorderStyle(i, value)
        });
    let border_width_input =
        iced_aw::number_input(&style.border_width, 0.0..=1000.0_f64, move |value| {
            message::Message::SetStyleBorderWidth(i, value)
        });
    let shadow_dist_input =
        iced_aw::number_input(&style.shadow_distance, 0.0..=1000.0_f64, move |value| {
            message::Message::SetStyleShadowDistance(i, value)
        });

    iced::widget::column![
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
    .spacing(4)
    .into()
}

fn alignment_grid(i: usize, style: &subtitle::Style) -> iced::widget::Column<'_, message::Message> {
    use nde::tags::{Alignment, HorizontalAlignment, VerticalAlignment};
    let cur = style.alignment;

    // (numpad label, vertical, horizontal)
    let cells: [(u8, VerticalAlignment, HorizontalAlignment); 9] = [
        (7, VerticalAlignment::Top, HorizontalAlignment::Left),
        (8, VerticalAlignment::Top, HorizontalAlignment::Center),
        (9, VerticalAlignment::Top, HorizontalAlignment::Right),
        (4, VerticalAlignment::Center, HorizontalAlignment::Left),
        (5, VerticalAlignment::Center, HorizontalAlignment::Center),
        (6, VerticalAlignment::Center, HorizontalAlignment::Right),
        (1, VerticalAlignment::Sub, HorizontalAlignment::Left),
        (2, VerticalAlignment::Sub, HorizontalAlignment::Center),
        (3, VerticalAlignment::Sub, HorizontalAlignment::Right),
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
                    iced::widget::button(
                        iced::widget::text(label.to_string())
                            .size(13)
                            .width(iced::Length::Fill)
                            .align_x(iced::alignment::Horizontal::Center),
                    )
                    .width(iced::Length::Fixed(28.0))
                    .height(iced::Length::Fixed(28.0))
                    .style(move |_, _| iced::widget::button::Style {
                        background: Some(
                            if selected {
                                crate::style::SAMAKU_PRIMARY
                            } else {
                                crate::style::SAMAKU_BACKGROUND_WEAK
                            }
                            .into(),
                        ),
                        text_color: if selected {
                            crate::style::SAMAKU_BACKGROUND
                        } else {
                            crate::style::SAMAKU_TEXT
                        },
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
}

#[expect(
    clippy::similar_names,
    reason = "symmetric naming is intentional for symmetric controls"
)]
fn section_positioning(i: usize, style: &subtitle::Style) -> iced::Element<'_, message::Message> {
    let justify_list = iced::widget::pick_list(JUSTIFY_MODES, Some(style.justify), move |value| {
        message::Message::SetStyleJustify(i, value)
    });
    let margin_l_input = iced_aw::number_input(&style.margins.left, 0..=9999_i32, move |value| {
        message::Message::SetStyleMarginLeft(i, value)
    });
    let margin_r_input = iced_aw::number_input(&style.margins.right, 0..=9999_i32, move |value| {
        message::Message::SetStyleMarginRight(i, value)
    });
    let margin_v_input =
        iced_aw::number_input(&style.margins.vertical, 0..=9999_i32, move |value| {
            message::Message::SetStyleMarginVertical(i, value)
        });

    iced::widget::column![
        section_label("Positioning"),
        iced::widget::row![
            iced::widget::column![iced::widget::text("Alignment"), alignment_grid(i, style),]
                .spacing(4),
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
    .spacing(6)
    .into()
}

/// Returns a small section header.
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
    red: &'a u8,
    green: &'a u8,
    blue: &'a u8,
    alpha: &'a i32,
    on_red: impl Fn(u8) -> message::Message + Copy + 'static,
    on_green: impl Fn(u8) -> message::Message + Copy + 'static,
    on_blue: impl Fn(u8) -> message::Message + Copy + 'static,
    on_alpha: impl Fn(i32) -> message::Message + Copy + 'static,
) -> iced::Element<'a, message::Message> {
    iced::widget::row![
        iced::widget::text(label).width(COL_LABEL_W),
        iced_aw::number_input(red, 0..=255_u8, on_red).width(COL_COLOUR_W),
        iced_aw::number_input(green, 0..=255_u8, on_green).width(COL_COLOUR_W),
        iced_aw::number_input(blue, 0..=255_u8, on_blue).width(COL_COLOUR_W),
        iced_aw::number_input(alpha, 0..=255_i32, on_alpha).width(COL_COLOUR_W),
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
