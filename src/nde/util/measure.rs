use iced::advanced::graphics::text as iced_text;
use iced_text::cosmic_text;
use std::cell::RefCell;
use std::sync::Arc;

use crate::{nde, resources, subtitle};

thread_local! {
    /// The font system used for measuring text
    static FONT_SYSTEM: RefCell<Option<cosmic_text::FontSystem>> = const { RefCell::new(None) };
}

fn init_font_system() -> cosmic_text::FontSystem {
    cosmic_text::FontSystem::new_with_fonts([cosmic_text::fontdb::Source::Binary(Arc::new(
        resources::BARLOW,
    ))])
}

/// Calculate the bounding box width and height for the given event.
/// Always returns a bounding box starting at the origin.
///
/// Dimensions are in ASS script coordinate units (PlayRes pixels at 1:1 scale), matching
/// the values that libass would use for text placement and collision detection.
///
/// # Panics
/// Panics on weight overflow.
#[expect(
    clippy::cast_possible_truncation,
    reason = "f64 → f32 for font sizes/spacing; values are in typical display ranges where precision loss is acceptable"
)]
#[must_use]
pub fn measure(event: &nde::Event, style: &subtitle::Style) -> nde::BoundingBox {
    // TODO: update this method to handle runs individually

    let font_name = event.effective_font_name(style);
    let font_size = event.effective_font_size(style);
    let font_weight = event.effective_font_weight(style);
    let italic = event.effective_italic(style);
    let letter_spacing = event.effective_letter_spacing(style);

    // Collect text from all tag spans (Drawing spans are skipped for now)
    // TODO: parse and draw drawings
    let mut full_text = String::new();
    for span in &event.text {
        if let &nde::Span::Tags(_, ref text) = span {
            full_text.push_str(text);
        }
    }

    let mut total_width = 0.0_f64;
    let mut total_height = 0.0_f64;

    FONT_SYSTEM.with_borrow_mut(|font_system_opt| {
        // Initialize font system if this did not already happen
        let font_system = font_system_opt.get_or_insert_with(init_font_system);

        // libass sizes fonts using FT_SIZE_REQUEST_TYPE_REAL_DIM, which maps the Win cell height
        // (usWinAscent + usWinDescent from the OS/2 table) to the requested font size, rather than
        // the em-square.  Applying this ratio to cosmic_text's em-square-based font size makes the
        // glyph advances match libass's output.  The line height stays at `font_size` (= the Win
        // cell height) in both cases, so height calculations are unaffected.
        let em_ratio = win_cell_to_em_ratio(font_name, font_weight, italic, font_system.db());
        let effective_em = font_size as f32 * em_ratio;

        // effective_em < font_size: set the glyph size to the smaller effective em-square.
        // line_height = font_size: preserves the full Win-cell-height line height that libass uses.
        let metrics = cosmic_text::Metrics::new(effective_em, font_size as f32);
        let mut buffer = cosmic_text::Buffer::new(font_system, metrics);
        buffer.set_size(font_system, None, None);

        // letter_spacing is in PlayRes units (same space as font_size).  Dividing by effective_em
        // keeps the spacing at the same absolute pixel value that libass applies.
        let letter_spacing_em = (letter_spacing / f64::from(effective_em)) as f32;
        let attrs = get_cosmic_attrs(font_name, font_weight, italic, letter_spacing_em);

        // Measure each hard line break (\N in ASS) independently, like GetLineBaseExtents
        for line in full_text.split("\\N") {
            buffer.set_text(
                font_system,
                line,
                &attrs,
                cosmic_text::Shaping::Advanced,
                None,
            );

            let (line_width, line_height) = buffer
                .layout_runs()
                .fold((0.0_f32, 0.0_f32), |(max_width, total_h), run| {
                    (run.line_w.max(max_width), total_h + run.line_height)
                });

            total_width = total_width.max(f64::from(line_width));
            total_height += f64::from(line_height);
        }
    });

    // \fscx / \fscy: uniform horizontal and vertical scale of the text block.
    // Values are pure factors (1.0 = no scale, 1.1 = 110%).
    let font_scale = event.effective_font_scale(style);
    total_width *= font_scale.x;
    total_height *= font_scale.y;

    // \bord / \xbord / \ybord: outline extends equally outward on all sides.
    let border = event.effective_border(style);
    total_width += 2.0 * border.x;
    total_height += 2.0 * border.y;

    // \shad / \xshad / \yshad: shadow is displaced to one side; the bounding box
    // grows by the absolute displacement in each axis regardless of sign.
    let shadow = event.effective_shadow(style);
    total_width += shadow.x.abs();
    total_height += shadow.y.abs();

    nde::BoundingBox {
        top_left: nde::tags::Position::new(0.0, 0.0),
        bottom_right: nde::tags::Position::new(total_width, total_height),
    }
}

/// Returns `units_per_em / (usWinAscent + usWinDescent)` for the best-matching font face.
///
/// This is the ratio by which cosmic_text's em-based font size must be scaled to produce
/// the same glyph advances as libass's `FT_SIZE_REQUEST_TYPE_REAL_DIM` request.
/// Falls back to `1.0` when the font cannot be located or has no OS/2 Win metrics.
fn win_cell_to_em_ratio(
    font_name: &str,
    font_weight: nde::tags::FontWeight,
    italic: bool,
    db: &cosmic_text::fontdb::Database,
) -> f32 {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "font weight value fits in u16"
    )]
    let query = cosmic_text::fontdb::Query {
        families: &[cosmic_text::fontdb::Family::Name(font_name)],
        weight: cosmic_text::fontdb::Weight(font_weight.weight() as u16),
        stretch: cosmic_text::fontdb::Stretch::Normal,
        style: if italic {
            cosmic_text::fontdb::Style::Italic
        } else {
            cosmic_text::fontdb::Style::Normal
        },
    };

    let Some(id) = db.query(&query) else {
        return 1.0;
    };

    db.with_face_data(id, |data, face_index| {
        let face = ttf_parser::Face::parse(data, face_index).ok()?;
        let upm = face.units_per_em();
        let os2 = face.tables().os2?;
        let win_asc = f32::from(os2.windows_ascender().unsigned_abs());
        let win_desc = f32::from(os2.windows_descender().unsigned_abs());
        let win_total = win_asc + win_desc;
        if win_total == 0.0 {
            return Some(1.0_f32);
        }
        Some(f32::from(upm) / win_total)
    })
    .flatten()
    .unwrap_or(1.0)
}

fn get_cosmic_attrs(
    font_family: &'_ str,
    font_weight: nde::tags::FontWeight,
    italic: bool,
    letter_spacing_em: f32,
) -> cosmic_text::Attrs<'_> {
    let mut attrs = cosmic_text::Attrs::new()
        .family(cosmic_text::Family::Name(font_family))
        .weight(cosmic_text::Weight(
            font_weight.weight().try_into().expect("weight overflow"),
        ))
        .style(if italic {
            cosmic_text::Style::Italic
        } else {
            cosmic_text::Style::Normal
        })
        .letter_spacing(letter_spacing_em);

    if letter_spacing_em != 0.0 {
        attrs
            .font_features
            .disable(cosmic_text::FeatureTag::KERNING);
    }

    attrs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media;
    use assert_float_eq::assert_float_relative_eq;

    /// Helper: create the canonical test event used by both `defined_line` and
    /// `defined_line_vs_libass`.
    fn test_event_and_style() -> (nde::Event, subtitle::Style) {
        let (global, spans) = nde::tags::parse(
            "{\\pos(0,0)\\an7\\b1\\i1\\fs160\\fsp5\\fnBarlow}Sphinx of black quartz,\\Njudge my vow",
        );
        let event = nde::Event {
            start: subtitle::StartTime(0),
            duration: subtitle::Duration(1000),
            layer_index: 0,
            style_index: 0,
            margins: subtitle::Margins::default(),
            global_tags: *global,
            overrides: nde::tags::Local::empty(),
            text: spans,
        };
        let style = subtitle::Style::default();
        (event, style)
    }

    #[test]
    fn defined_line() {
        let (event, style) = test_event_and_style();
        let bounding_box = measure(&event, &style);

        assert_float_relative_eq!(bounding_box.top_left.x, 0.0, 0.01);
        assert_float_relative_eq!(bounding_box.top_left.y, 0.0, 0.01);
        assert_float_relative_eq!(bounding_box.bottom_right.x, 1293.0, 0.01);
        assert_float_relative_eq!(bounding_box.bottom_right.y, 335.0, 0.01);
    }

    fn calc_cosmic_libass(template: &str, event_text: &str) -> (f64, f64) {
        let ass_content = template.replace("[[EVENT TEXT]]", event_text);

        let track = media::subtitle::OpaqueTrack::parse(&ass_content);
        let event_track = track.to_event_track();
        let (_, ass_event) = event_track.get_nth(0).unwrap();
        let style = &track.styles()[ass_event.style_index];
        let nde_event = nde::Event::from_ass_event(ass_event);

        let cosmic_bb = measure(&nde_event, style);
        println!(
            "cosmic_text:  width={:.1}  height={:.1}",
            cosmic_bb.bottom_right.x, cosmic_bb.bottom_right.y
        );

        let frame = track.script_info().playback_resolution;
        let mut renderer = media::subtitle::Renderer::new();

        // We only compare the width, because libass' height is less predictable (since libass
        // reports the ink bounding box).
        let mut x_min = i32::MAX;
        let mut x_max = i32::MIN;
        renderer.render_subtitles_with_callback(&track, 1000, frame, frame, &mut |img| {
            x_min = x_min.min(img.metadata.dst_x);
            x_max = x_max.max(img.metadata.dst_x + img.metadata.w);
        });

        assert!(x_max > x_min, "no images were rendered");

        let libass_w = f64::from(x_max - x_min);
        (cosmic_bb.bottom_right.x, libass_w)
    }

    /// Compare cosmic_text measurements against a libass pixel-render of the same text.
    #[test]
    fn defined_line_vs_libass() {
        media::subtitle::set_libass_test_callback();

        let ass_content = std::fs::read_to_string(crate::test_utils::test_file(
            "test_files/measure_template.ass",
        ))
        .unwrap();

        let (cosmic_w, libass_w) = calc_cosmic_libass(
            &ass_content,
            r"{\pos(0,0)\an7\b1\i1\fs160\fsp5\bord0\shad0\fnBarlow}Sphinx of black quartz,",
        );
        assert_float_relative_eq!(cosmic_w, libass_w, 0.02);

        let (cosmic_w, libass_w) = calc_cosmic_libass(
            &ass_content,
            r"{\pos(0,0)\an7\fs160\fsp5\bord0\shad0\fnBarlow}Sphinx of black quartz,",
        );
        assert_float_relative_eq!(cosmic_w, libass_w, 0.02);

        let (cosmic_w, libass_w) = calc_cosmic_libass(
            &ass_content,
            r"{\pos(0,0)\an7\fs160\bord0\shad0\fnBarlow}Sphinx of black quartz,",
        );
        assert_float_relative_eq!(cosmic_w, libass_w, 0.02);

        let (cosmic_w, libass_w) = calc_cosmic_libass(
            &ass_content,
            r"{\pos(0,0)\an7\fs40\bord0\shad0\fnBarlow}Sphinx of black quartz,",
        );
        assert_float_relative_eq!(cosmic_w, libass_w, 0.05); // use a bit more tolerance at smaller font sizes

        let (cosmic_w, libass_w) = calc_cosmic_libass(
            &ass_content,
            r"{\pos(0,0)\an7\fs40\fscx110\fnBarlow}Sphinx of black quartz,",
        );
        assert_float_relative_eq!(cosmic_w, libass_w, 0.05);

        let (cosmic_w, libass_w) = calc_cosmic_libass(
            &ass_content,
            r"{\pos(0,0)\an7\fs40\fscx110\shad30\fnBarlow}Sphinx of black quartz,",
        );
        assert_float_relative_eq!(cosmic_w, libass_w, 0.05);

        let (_cosmic_w, _libass_w) = calc_cosmic_libass(
            &ass_content,
            r"{\pos(0,0)\an7\fs120\bord0\shad0\fnBarlow}色は匂えど散りぬるを",
        );
        // Ignore this one for now since we do not yet predict fallback fonts.
        // TODO implement fallback fonts in some way?
        // assert_float_relative_eq!(cosmic_w, libass_w, 0.02);
    }
}
