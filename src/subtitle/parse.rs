//! Functions for parsing `.ass` files. For parsing ASS override tags, see [`nde::tags::parse`].

use std::borrow::Cow;
use std::collections::HashMap;

use smol::stream::StreamExt as _;
use thiserror::Error;

use crate::nde::tags::{Alignment, Colour, Transparency};
use crate::{project, subtitle};

use super::{
    Angle, Attachment, AttachmentType, BorderStyle, Duration, Event, EventTrack, EventType,
    Extradata, ExtradataEntry, ExtradataId, File, FontEncoding, JustifyMode, Margins, Scale,
    ScriptInfo, StartTime, Style, StyleList, YCbCrMatrix,
};

#[expect(
    clippy::too_many_lines,
    reason = "uncoupling the different parts of parsing would make the code unnecessarily complicated"
)]
pub(super) async fn parse<R: smol::io::AsyncBufRead + Unpin>(
    input: smol::io::Lines<R>,
) -> Result<(File, Vec<Warning>), SubtitleParseError> {
    let mut state = ParseState::ScriptInfo;

    // Data of opaque/unknown sections
    let mut header = String::new();
    let mut section = vec![];
    let mut opaque_sections: HashMap<String, Vec<String>> = HashMap::new();

    let mut current_attachment: Option<Attachment> = None;

    let mut styles: Vec<Style> = vec![];
    let mut raw_events_and_style_names: Vec<(Event, String)> = vec![];
    let mut script_info = ScriptInfo::default();
    let mut extradata = Extradata::default();
    let mut aegi_metadata = HashMap::new();
    let mut attachments = vec![];

    let mut warnings: Vec<Warning> = vec![];

    let mut input_enumerate = input.enumerate();

    while let Some((line_index, line_result)) = input_enumerate.next().await {
        let line_number = line_index + 1;
        let line_string = line_result.map_err(SubtitleParseError::IoError)?;
        let line = line_string.trim();

        if let Some(mut attachment) = current_attachment.take() {
            match parse_attachment_line(line, &mut attachment) {
                AttachmentParseResult::NotFinished => {
                    current_attachment = Some(attachment);
                    continue;
                }
                AttachmentParseResult::FinishedAndLineConsumed => {
                    attachments.push(attachment);
                    continue;
                }
                AttachmentParseResult::FinishedWithoutConsumingLine => {
                    attachments.push(attachment);
                    // Do not continue the loop — we need to run the line parsing code below
                }
            }
        }

        if line.starts_with('[') && line.ends_with(']') {
            // Section header

            // Finalise opaque section, if it exists
            if !header.is_empty() {
                opaque_sections.insert(header, section);

                header = String::new();
                section = vec![];
            }

            if line.eq_ignore_ascii_case("[v4 styles]") {
                return Err(SubtitleParseError::V4StylesFound);
            } else if line.eq_ignore_ascii_case("[v4+ styles]") {
                state = ParseState::Styles;
            } else if line.eq_ignore_ascii_case("[events]") {
                state = ParseState::Events;
            } else if line.eq_ignore_ascii_case("[script info]") {
                state = ParseState::ScriptInfo;
            } else if line.eq_ignore_ascii_case("[aegisub project garbage]") {
                state = ParseState::AegiMetadata;
            } else if line.eq_ignore_ascii_case("[aegisub extradata]") {
                state = ParseState::Extradata;
            } else if line.eq_ignore_ascii_case("[graphics]") {
                state = ParseState::Graphics;
            } else if line.eq_ignore_ascii_case("[fonts]") {
                state = ParseState::Fonts;
            } else {
                state = ParseState::Unknown;
                #[expect(
                    clippy::string_slice,
                    reason = "safe because we previously verified that the first character '[' and the last character ']' are 1 byte each"
                )]
                header.push_str(&line[1..(line.len() - 1)]);
            }

            continue;
        }

        match state {
            ParseState::Unknown => {
                section.push(line_string);
            }
            ParseState::Styles => {
                if line.starts_with("Style:") {
                    match parse_style_line(line) {
                        Ok(style) => {
                            styles.push(style);
                        }
                        Err(parse_error) => {
                            warnings.push(Warning::StyleOnLine(line_number, parse_error));
                        }
                    }
                }
            }
            ParseState::Events => {
                if line.starts_with("Dialogue:") || line.starts_with("Comment:") {
                    match parse_event_line(line) {
                        Ok(event) => raw_events_and_style_names.push(event),
                        Err(parse_error) => {
                            warnings.push(Warning::EventOnLine(line_number, parse_error));
                        }
                    }
                }
            }
            ParseState::ScriptInfo => {
                parse_script_info_line(line, &mut script_info)?;
            }
            ParseState::AegiMetadata => {
                parse_aegi_metadata_line(line, &mut aegi_metadata);
            }
            ParseState::Extradata => {
                parse_extradata_line(line, &mut extradata)?;
            }
            ParseState::Graphics => {
                current_attachment =
                    parse_attachment_header(line, "filename: ", AttachmentType::Graphic);
            }
            ParseState::Fonts => {
                current_attachment =
                    parse_attachment_header(line, "fontname: ", AttachmentType::Font);
            }
        }
    }

    // Finalise `LayoutRes`
    if let Some(layout_resolution) = script_info.layout_resolution
        && (layout_resolution.x == 0 || layout_resolution.y == 0)
    {
        script_info.layout_resolution = None;
    }

    // Finalise opaque section, if it exists
    if !header.is_empty() {
        opaque_sections.insert(header, section);
    }

    // Create a StyleList from the styles we read. This ensures there will be at least one style,
    // and no styles will have duplicate names.
    let (style_list, leftovers) = StyleList::from_vec(styles);
    for style in &leftovers.leftover {
        warnings.push(Warning::DuplicateStyle(style.name().to_owned()));
    }

    // Match event style names to styles, and construct event track
    let mut events = EventTrack::new_empty();
    match_styles(
        &mut events,
        raw_events_and_style_names,
        &style_list,
        &mut warnings,
    );

    let file = File {
        script_info,
        aegi_metadata,
        attachments,
        other_sections: opaque_sections,
        styles: style_list,
        events,
        extradata,
    };

    Ok((file, warnings))
}

/// Takes a Vec of events and their styles referenced by name, matches the style names to the given style list,
/// then adds the events with the correct style reference to the given event track.
pub fn match_styles(
    target_event_track: &mut EventTrack,
    raw: Vec<(Event<'static>, String)>,
    style_list: &StyleList,
    warnings: &mut Vec<Warning>,
) {
    for (mut raw_event, style_name) in raw {
        if let Some(style_index) = style_list.find_by_name(&style_name) {
            raw_event.style_index = style_index;
            target_event_track.push(raw_event);
        } else {
            warnings.push(Warning::UnmatchedStyle(style_name));
        }
    }
}

enum ParseState {
    Unknown,
    Styles,
    Events,
    ScriptInfo,
    AegiMetadata,
    Extradata,
    Graphics,
    Fonts,
}

#[derive(Error, Debug)]
pub enum SubtitleParseError {
    #[error("No file was selected")]
    NoFileSelected,

    #[error("IO error: {0}")]
    IoError(smol::io::Error),

    #[error("Script type must be v4.00+, all other versions are unsupported")]
    UnsupportedScriptType,

    #[error("V4 Styles (not V4+) are unsupported")]
    V4StylesFound,

    #[error("Malformed style line")]
    MalformedStyleLine,

    #[error("Style line must have the “Style” key")]
    StyleLineInvalidKey,

    #[error("Invalid event type for line: {0}")]
    InvalidEventType(String),

    #[error("Truncated event or style line")]
    TruncatedLine,

    #[error("Could not parse integer: {0}")]
    ParseIntError(std::num::ParseIntError),

    #[error("Could not parse float: {0}")]
    ParseFloatError(std::num::ParseFloatError),

    #[error("Found invalid timecode: {0}")]
    InvalidTimecode(String),

    #[error("Found invalid alignment value in style")]
    InvalidAlignment,

    #[error("Invalid NDE filter format identifier: {0:?}")]
    InvalidNdeFilterFormat(Option<u8>),

    #[error("Failed to deserialize NDE filter: {0:?}")]
    NdeFilterDeserializeError(project::DeserializeError),

    #[error("Failed to decode UU-encoded extradata")]
    UuDecodeError(data_encoding::DecodeError),

    #[error("Invalid extradata value type: {0}")]
    InvalidExtradataValueType(String),

    #[error("Invalid extradata ID: {0}")]
    InvalidExtradataId(String),
}

/// Denotes that something could not be fully parsed, and was thus ignored.
#[derive(Error, Debug)]
pub enum Warning {
    #[error("Could not read style on line {0}: {1}")]
    StyleOnLine(usize, SubtitleParseError),

    #[error("Could not read event on line {0}: {1}")]
    EventOnLine(usize, SubtitleParseError),

    #[error("Unknown style {0} — replacing with default")]
    UnmatchedStyle(String),

    #[error("Skipping duplicate style {0}")]
    DuplicateStyle(String),
}

fn parse_style_line(line: &str) -> Result<Style, SubtitleParseError> {
    let Some((key, value)) = parse_kv_generic(line) else {
        return Err(SubtitleParseError::MalformedStyleLine);
    };

    if key != "Style" {
        return Err(SubtitleParseError::StyleLineInvalidKey);
    }

    let mut split = value.splitn(23, ',');

    let name = next_split_trim::<true>(&mut split)?.to_owned();
    let font_name = next_split_trim::<true>(&mut split)?.to_owned();
    let font_size = next_split_f64(&mut split)?;

    let (primary_colour, primary_transparency) =
        parse_packed_colour_and_transparency(next_split_trim::<true>(&mut split)?)?;
    let (secondary_colour, secondary_transparency) =
        parse_packed_colour_and_transparency(next_split_trim::<true>(&mut split)?)?;
    let (border_colour, border_transparency) =
        parse_packed_colour_and_transparency(next_split_trim::<true>(&mut split)?)?;
    let (shadow_colour, shadow_transparency) =
        parse_packed_colour_and_transparency(next_split_trim::<true>(&mut split)?)?;

    let bold = next_split_bool(&mut split)?;
    let italic = next_split_bool(&mut split)?;
    let underline = next_split_bool(&mut split)?;
    let strike_out = next_split_bool(&mut split)?;

    let scale_x = next_split_f64(&mut split)?.max(0.0) / 100.0;
    let scale_y = next_split_f64(&mut split)?.max(0.0) / 100.0;

    let spacing = next_split_f64(&mut split)?.max(0.0);
    let angle = Angle(next_split_f64(&mut split)?);

    let border_style = BorderStyle::from(next_split_i32(&mut split)?);
    let border_width = next_split_f64(&mut split)?.max(0.0);
    let shadow_distance = next_split_f64(&mut split)?.max(0.0);
    let alignment = Alignment::try_from_an(next_split_i32(&mut split)?)
        .ok_or(SubtitleParseError::InvalidAlignment)?;

    let margin_l = next_split_i32(&mut split)?;
    let margin_r = next_split_i32(&mut split)?;
    let margin_v = next_split_i32(&mut split)?;

    let encoding = FontEncoding(next_split_i32(&mut split)?);

    let style = Style {
        name,
        font_name,
        font_size,
        primary_colour,
        secondary_colour,
        border_colour,
        shadow_colour,
        primary_transparency,
        secondary_transparency,
        border_transparency,
        shadow_transparency,
        bold,
        italic,
        underline,
        strike_out,
        scale: Scale {
            x: scale_x,
            y: scale_y,
        },
        spacing,
        angle,
        border_style,
        border_width,
        shadow_distance,
        alignment,
        margins: Margins {
            left: margin_l,
            right: margin_r,
            vertical: margin_v,
        },
        encoding,

        // These two do not appear to be represented in Aegisub-flavour .ass files
        blur: 0.0,
        justify: JustifyMode::Auto,
    };

    Ok(style)
}

fn parse_event_line(line: &str) -> Result<(Event<'static>, String), SubtitleParseError> {
    let (event_type, fields_str) = if let Some(fields_str) = line.strip_prefix("Dialogue: ") {
        (EventType::Dialogue, fields_str)
    } else if let Some(fields_str) = line.strip_prefix("Comment: ") {
        (EventType::Comment, fields_str)
    } else {
        return Err(SubtitleParseError::InvalidEventType(line.to_owned()));
    };

    let mut split = fields_str.splitn(10, ',');

    // TODO: `Marked=`?
    // https://github.com/arch1t3cht/Aegisub/blob/d8c611d662480aea1fae6c438892b4327447765a/src/ass_dialogue.cpp#L106
    let layer = next_split_i32(&mut split)?;

    let start = parse_timecode(next_split_trim::<true>(&mut split)?)?;
    let end = parse_timecode(next_split_trim::<true>(&mut split)?)?;

    let start_time = StartTime(start);
    let duration = Duration(end - start);

    parse_event_line_tail(event_type, layer, start_time, duration, split)
}

/// Matroska files store ASS event data fields in the following order:
/// `ReadOrder,Layer,Style,Name,MarginL,MarginR,MarginV,Effect,Text`.
///
/// The timing info is stored outside of the line using Matroska's custom timing fields.
/// The event type can only be `Dialogue` (TODO is this correct?).
pub fn matroska_event_line(
    start: StartTime,
    duration: Duration,
    line: &str,
) -> Result<(Event<'static>, String), SubtitleParseError> {
    let mut split = line.splitn(9, ',');

    let _read_order = next_split_i32(&mut split)?; // ignore this one
    let layer = next_split_i32(&mut split)?;

    parse_event_line_tail(EventType::Dialogue, layer, start, duration, split)
}

fn parse_event_line_tail(
    event_type: EventType,
    layer_index: i32,
    start: StartTime,
    duration: Duration,
    mut split: std::str::SplitN<char>,
) -> Result<(Event<'static>, String), SubtitleParseError> {
    let style = next_split_trim::<true>(&mut split)?.to_owned();
    let actor = next_split_trim::<true>(&mut split)?.to_owned();

    let margin_l = next_split_i32(&mut split)?;
    let margin_r = next_split_i32(&mut split)?;
    let margin_v = next_split_i32(&mut split)?;

    let effect = next_split_trim::<true>(&mut split)?.to_owned();

    // Aegisub only trims the event text at its end. We match that behaviour, because why not.
    let mut text = next_split_trim::<false>(&mut split)?;

    let mut extradata_ids: Vec<ExtradataId> = vec![];

    if text.starts_with("{=")
        && let Some((new_extradata_ids, after)) = parse_extradata_references(text)
    {
        extradata_ids = new_extradata_ids;

        #[expect(
            clippy::string_slice,
            reason = "safe because we know parse_extradata_references will always return an index at a character boundary"
        )]
        let tail = &text[after..];
        text = tail;
    }

    let new_event = Event {
        start,
        duration,
        layer_index,
        style_index: 0,
        margins: Margins {
            left: margin_l,
            right: margin_r,
            vertical: margin_v,
        },
        text: Cow::Owned(text.to_owned()),
        actor: Cow::Owned(actor),
        effect: Cow::Owned(effect),
        event_type,
        extradata_ids,
    };

    Ok((new_event, style))
}

fn parse_script_info_line(
    line: &str,
    script_info: &mut ScriptInfo,
) -> Result<(), SubtitleParseError> {
    if line.starts_with(';') {
        // Comment
        return Ok(());
    }

    if let Some(value) = line.strip_prefix("ScriptType:") {
        let version_str = value.trim().to_ascii_lowercase();
        if version_str != "v4.00+" {
            return Err(SubtitleParseError::UnsupportedScriptType);
        }

        // Don't read this one as K/V data later on
        return Ok(());
    }

    let Some((key, value)) = parse_kv_generic(line) else {
        // ignore lines without a colon
        return Ok(());
    };

    if key == "PlayResX" {
        if let Ok(int_value) = value.parse::<i32>() {
            script_info.playback_resolution.x = int_value;
        }
    } else if key == "PlayResY" {
        if let Ok(int_value) = value.parse::<i32>() {
            script_info.playback_resolution.y = int_value;
        }
    } else if key == "LayoutResX" {
        if let Ok(int_value) = value.parse::<i32>() {
            if let Some(layout_resolution) = script_info.layout_resolution.as_mut() {
                layout_resolution.x = int_value;
            } else {
                script_info.layout_resolution = Some(subtitle::Resolution { x: int_value, y: 0 });
            }
        }
    } else if key == "LayoutResY" {
        if let Ok(int_value) = value.parse::<i32>() {
            if let Some(layout_resolution) = script_info.layout_resolution.as_mut() {
                layout_resolution.y = int_value;
            } else {
                script_info.layout_resolution = Some(subtitle::Resolution { x: 0, y: int_value });
            }
        }
    } else if key == "WrapStyle" {
        if let Ok(int_value) = value.parse::<i32>() {
            script_info.wrap_style = int_value.into();
        }
    } else if key == "ScaledBorderAndShadow" {
        script_info.scaled_border_and_shadow = key != "no";
    } else if key == "YCbCr Matrix" {
        script_info.ycbcr_matrix = match value {
            "TV.601" => YCbCrMatrix::Bt601Tv,
            "PC.601" => YCbCrMatrix::Bt601Pc,
            "TV.709" => YCbCrMatrix::Bt709Tv,
            "PC.709" => YCbCrMatrix::Bt709Pc,
            "TV.FCC" => YCbCrMatrix::FccTv,
            "PC.FCC" => YCbCrMatrix::FccPc,
            "TV.240M" => YCbCrMatrix::Smtpe240MTv,
            "PC.240M" => YCbCrMatrix::Smtpe240MPc,
            _ => YCbCrMatrix::None,
        };
    } else {
        script_info
            .extra_info
            .insert(key.to_owned(), value.to_owned());
    }

    Ok(())
}

fn parse_aegi_metadata_line(line: &str, aegi_metadata: &mut HashMap<String, String>) {
    if let Some((key, value)) = parse_kv_generic(line) {
        aegi_metadata.insert(key.to_owned(), value.to_owned());
    }
}

/// Splits off a non-empty run of ASCII digits at the start of `input`.
fn split_digits(input: &str) -> Option<(&str, &str)> {
    let run = input
        .find(|char: char| !char.is_ascii_digit())
        .unwrap_or(input.len());
    (run > 0).then(|| input.split_at(run))
}

/// The four fields captured from an Aegisub extradata `Data:` line.
struct ExtradataFields<'a> {
    id: &'a str,
    key: &'a str,
    value_type: &'a str,
    value_raw: &'a str,
}

/// Matches the remainder of a Data line, after the literal prefix has been consumed.
fn match_extradata_after_prefix(input: &str) -> Option<ExtradataFields<'_>> {
    // `[[:space:]]` is the POSIX ASCII class, which also contains the vertical tab that
    // `char::is_ascii_whitespace` leaves out.
    let after_space =
        input.trim_start_matches(|char: char| char.is_ascii_whitespace() || char == '\u{0b}');

    // The id is matched permissively, as everything up to the next comma, so that a `Data:` line
    // with a malformed id is rejected loudly by `parse_extradata_line` rather than silently
    // skipped here.
    let id_run = after_space.find(',')?;
    if id_run == 0 {
        return None;
    }
    let (id, after_id) = after_space.split_at(id_run);
    let key_start = after_id.strip_prefix(',')?;

    let key_run = key_start.find(',')?;
    if key_run == 0 {
        return None;
    }
    let (key, after_key) = key_start.split_at(key_run);
    let value_start = after_key.strip_prefix(',')?;

    // `.` matches exactly one character and `.*` matches as many as possible, but neither ever
    // matches a newline.
    let value_type_char = value_start.chars().next()?;
    if value_type_char == '\n' {
        return None;
    }
    let (value_type, after_type) = value_start.split_at(value_type_char.len_utf8());
    let (value_raw, _) = after_type.split_at(after_type.find('\n').unwrap_or(after_type.len()));

    Some(ExtradataFields {
        id,
        key,
        value_type,
        value_raw,
    })
}

/// Splits a Data line into its four comma-separated fields, searching for the leftmost `Data:`
/// prefix that is followed by a well-formed remainder.
///
/// This replaces the regex `Data:[[:space:]]*(\d+),([^,]+),(.)(.*)`, differing from it only in
/// that the id is not required to consist of digits — see [`match_extradata_after_prefix`].
fn match_extradata_line(line: &str) -> Option<ExtradataFields<'_>> {
    line.match_indices("Data:").find_map(|(start, prefix)| {
        match_extradata_after_prefix(line.split_at(start).1.split_at(prefix.len()).1)
    })
}

fn parse_extradata_line(line: &str, extradata: &mut Extradata) -> Result<(), SubtitleParseError> {
    if let Some(fields) = match_extradata_line(line) {
        let id_str = fields.id;
        let Ok(id_num) = id_str.parse::<u32>() else {
            return Err(SubtitleParseError::InvalidExtradataId(id_str.to_owned()));
        };

        let key = aegi_inline_string_decode(fields.key);
        let value_type = fields.value_type;
        let value_raw = fields.value_raw;

        let value = if value_type == "e" {
            aegi_inline_string_decode(value_raw).into_bytes()
        } else if value_type == "u" {
            super::uu::decode(value_raw).map_err(SubtitleParseError::UuDecodeError)?
        } else {
            return Err(SubtitleParseError::InvalidExtradataValueType(
                value_type.to_owned(),
            ));
        };

        extradata.next_id = extradata.next_id.max(ExtradataId(id_num + 1));
        extradata
            .entries
            .insert(ExtradataId(id_num), parse_extradata_entry(key, value)?);
    }

    Ok(())
}

fn parse_extradata_entry(
    key: String,
    value: Vec<u8>,
) -> Result<ExtradataEntry, SubtitleParseError> {
    if key == "_samaku_nde_filter" {
        let first_char = value.first().copied();
        if first_char == Some(b'1') {
            let filter = project::deserialize_czb(&value[1..])
                .map_err(SubtitleParseError::NdeFilterDeserializeError)?;
            Ok(ExtradataEntry::NdeFilter(filter))
        } else {
            Err(SubtitleParseError::InvalidNdeFilterFormat(first_char))
        }
    } else {
        Ok(ExtradataEntry::Opaque { key, value })
    }
}

fn parse_attachment_header(
    line: &str,
    filename_key: &str,
    attachment_type: AttachmentType,
) -> Option<Attachment> {
    line.strip_prefix(filename_key).map(|filename| Attachment {
        attachment_type,
        filename: filename.to_owned(),
        uu_data: String::new(),
    })
}

fn parse_attachment_line(line: &str, attachment: &mut Attachment) -> AttachmentParseResult {
    let is_filename = line.starts_with("filename: ") || line.starts_with("fontname: ");
    let mut valid_data = !line.is_empty() && line.len() <= 80;
    for byte in line.bytes() {
        if !(33..=97).contains(&byte) {
            valid_data = false;
            break;
        }
    }

    if !valid_data || is_filename {
        return AttachmentParseResult::FinishedWithoutConsumingLine;
    }

    attachment_add_data(line, attachment);

    if line.len() < 80 {
        AttachmentParseResult::FinishedAndLineConsumed
    } else {
        AttachmentParseResult::NotFinished
    }
}

enum AttachmentParseResult {
    NotFinished,
    FinishedAndLineConsumed,
    FinishedWithoutConsumingLine,
}

fn attachment_add_data(line: &str, attachment: &mut Attachment) {
    attachment.uu_data.push_str(line);
}

fn parse_extradata_references(text: &str) -> Option<(Vec<ExtradataId>, usize)> {
    let mut res = vec![];
    let mut match_start_option: Option<usize> = None;

    for (i, char) in text.char_indices() {
        if i == 0 {
            if char == '{' {
                continue;
            }

            return None;
        }

        match char {
            '=' => {
                if let Some(match_start) = match_start_option.take() {
                    // If we already have a match start set (i.e. this is not the first '=' in the line),
                    // parse and append it.
                    #[expect(
                        clippy::string_slice,
                        reason = "safe because both match_start and i come from char_indices"
                    )]
                    res.push(ExtradataId(text[match_start..i].parse::<u32>().unwrap()));
                } else if i != 1 {
                    // Double `=` are not allowed
                    return None;
                } else {
                    // It must be the first '=' in the line. Ignore it, the match start will be set
                    // by the number that follows.
                }
            }
            '0'..='9' => {
                if i == 1 {
                    // Needs a `=` before
                    return None;
                }

                match_start_option.get_or_insert(i);
            }
            '}' => {
                return if let Some(match_start) = match_start_option.take() {
                    #[expect(
                        clippy::string_slice,
                        reason = "safe because both match_start and i come from char_indices"
                    )]
                    res.push(ExtradataId(text[match_start..i].parse::<u32>().unwrap()));

                    // Returning `i + 1` is valid here because we know the character at `i` is '}', so 1 byte.
                    Some((res, i + 1))
                } else {
                    // Empty block
                    None
                };
            }
            _ => {
                // Invalid character
                return None;
            }
        }
    }

    // If we reached this point, we never hit the closing bracket, which is invalid
    None
}

fn next_split_trim<'a, const TRIM_START: bool>(
    split: &'a mut std::str::SplitN<char>,
) -> Result<&'a str, SubtitleParseError> {
    match split.next() {
        Some(str) => Ok(if TRIM_START {
            str.trim()
        } else {
            str.trim_end()
        }),
        None => Err(SubtitleParseError::TruncatedLine),
    }
}

fn next_split_i32(split: &mut std::str::SplitN<char>) -> Result<i32, SubtitleParseError> {
    next_split_trim::<true>(split)?
        .parse::<i32>()
        .map_err(SubtitleParseError::ParseIntError)
}

fn next_split_f64(split: &mut std::str::SplitN<char>) -> Result<f64, SubtitleParseError> {
    next_split_trim::<true>(split)?
        .parse::<f64>()
        .map_err(SubtitleParseError::ParseFloatError)
}

fn next_split_bool(split: &mut std::str::SplitN<char>) -> Result<bool, SubtitleParseError> {
    Ok(next_split_trim::<true>(split)?
        .parse::<i32>()
        .map_err(SubtitleParseError::ParseIntError)?
        != 0)
}

/// Parse a generic key/value line of the form `Key: Value`.
fn parse_kv_generic(line: &str) -> Option<(&str, &str)> {
    // ignore lines without a colon
    line.split_once(':')
        .map(|(key, value)| (key, value.trim_start()))
}

/// Splits off one `scanf` `%d` conversion: any leading whitespace, an optional sign, and a
/// non-empty run of ASCII digits. Returns whether the sign was negative, the digits themselves,
/// and the rest of the input.
///
/// The digits are handed back unparsed because the fractional part of a timecode interprets them
/// positionally rather than as a number.
fn split_scanf_integer(input: &str) -> Option<(bool, &str, &str)> {
    // C's `isspace`, which `scanf` skips before a `%d`, includes the vertical tab that
    // `char::is_ascii_whitespace` leaves out.
    let after_space =
        input.trim_start_matches(|char: char| char.is_ascii_whitespace() || char == '\u{0b}');

    let (negative, after_sign) = if let Some(rest) = after_space.strip_prefix('-') {
        (true, rest)
    } else if let Some(rest) = after_space.strip_prefix('+') {
        (false, rest)
    } else {
        (false, after_space)
    };

    let (digits, rest) = split_digits(after_sign)?;
    Some((negative, digits, rest))
}

/// Splits off one whole-number timecode component, returning [`None`] if it is missing or does not
/// fit into an [`i64`].
fn split_timecode_component(input: &str) -> Option<(i64, &str)> {
    let (negative, digits, rest) = split_scanf_integer(input)?;
    let magnitude = digits.parse::<i64>().ok()?;
    Some((if negative { -magnitude } else { magnitude }, rest))
}

/// Converts at most 3 digits following the decimal point into milliseconds, reading them positionally.
///
/// Two digits therefore mean centiseconds and three mean milliseconds,
/// so both the standard `0:00:01.50` and a more precise `0:00:01.500` denote one and a half seconds.
fn fraction_to_millis(digits: &str) -> i64 {
    digits
        .chars()
        .filter_map(|char| char.to_digit(10))
        .zip([100_i64, 10, 1])
        .map(|(digit, weight)| i64::from(digit) * weight)
        .sum()
}

/// Converts a timecode into a number of milliseconds, returning [`None`] if it is malformed or
/// overflows an [`i64`].
///
/// This mirrors libass' `string2timecode`, which is a single `sscanf(p, "%d:%d:%d.%d", &h, &m, &s, &ms)`.
/// The components are separated by literal colons and a literal period,
/// each one may be preceded by whitespace and a sign, all four are required,
/// and anything following them is ignored.
///
/// However, we intentionally deviate from libass' behavior for the fractional part,
/// which is read positionally (see [`fraction_to_millis`]) to allow for millisecond precision timecodes,
/// matching Aegisub in this case rather than libass.
fn timecode_to_millis(timecode: &str) -> Option<i64> {
    let (hours, after_hours) = split_timecode_component(timecode)?;
    let (minutes, after_minutes) = split_timecode_component(after_hours.strip_prefix(':')?)?;
    let (seconds, after_seconds) = split_timecode_component(after_minutes.strip_prefix(':')?)?;

    let (negative, fraction_digits, _) = split_scanf_integer(after_seconds.strip_prefix('.')?)?;
    let fraction = fraction_to_millis(fraction_digits);

    hours
        .checked_mul(60)?
        .checked_add(minutes)?
        .checked_mul(60)?
        .checked_add(seconds)?
        .checked_mul(1000)?
        .checked_add(if negative { -fraction } else { fraction })
}

fn parse_timecode(timecode: &str) -> Result<i64, SubtitleParseError> {
    timecode_to_millis(timecode)
        .ok_or_else(|| SubtitleParseError::InvalidTimecode(timecode.to_owned()))
}

fn parse_packed_colour_and_transparency(
    packed_colour_hex: &str,
) -> Result<(Colour, Transparency), SubtitleParseError> {
    let prefix_stripped = packed_colour_hex
        .strip_prefix("&H")
        .or_else(|| packed_colour_hex.strip_prefix("&h"))
        .unwrap_or(packed_colour_hex);
    let suffix_stripped = prefix_stripped.strip_suffix('&').unwrap_or(prefix_stripped);
    let number =
        u32::from_str_radix(suffix_stripped, 16).map_err(SubtitleParseError::ParseIntError)?;

    Ok(subtitle::unpack_colour_and_transparency_tbgr(number))
}

fn aegi_inline_string_decode(input: &str) -> String {
    let input_byte_size = input.len();
    let mut output = String::with_capacity(input_byte_size);
    let mut tag = String::with_capacity(3);

    for char in input.chars() {
        if char == '#' || !tag.is_empty() {
            if char.is_ascii() {
                tag.push(char);
            } else {
                // Aegisub doesn't handle the edge case that an UTF-8 character starts in the
                // middle of a tag. Let's do better than that
                output.push_str(&tag);
                tag.clear();
            }
        }

        if tag.len() == 3 {
            // Tag is done
            #[expect(
                clippy::string_slice,
                reason = "safe because the tag is guaranteed to only contain ascii chars"
            )]
            let tag_tail = &tag[1..];
            let represented_byte = u8::from_str_radix(tag_tail, 16).unwrap_or(0);
            output.push(represented_byte as char);
            tag.clear();
        } else if tag.is_empty() {
            output.push(char);
        } else {
            // The tag is still being filled, so we don't need to change the output.
        }
    }

    if !tag.is_empty() {
        output.push_str(&tag);
    }

    output
}

#[cfg(test)]
pub mod tests {
    use crate::nde::tags::{HorizontalAlignment, VerticalAlignment, WrapStyle};
    use crate::test_utils::test_file;
    use assert_float_eq::assert_float_absolute_eq;
    use assert_matches2::assert_matches;
    use smol::io::AsyncBufReadExt as _;
    use std::path::Path;

    use super::*;

    /// Parse the file at the given path to a `File`.
    ///
    /// # Panics
    /// Panics if any error occurs (IO or parsing).
    #[must_use]
    pub fn parse_blocking(path: &Path) -> (File, Vec<Warning>) {
        smol::block_on(async {
            let lines = smol::io::BufReader::new(smol::fs::File::open(path).await.unwrap()).lines();
            parse(lines).await
        })
        .unwrap()
    }

    /// Parse the given string to a `File`.
    ///
    /// # Panics
    /// Panics if a parse error occurs.
    #[must_use]
    pub fn parse_str(str: &str) -> (File, Vec<Warning>) {
        smol::block_on(async {
            let lines = smol::io::BufReader::new(str.as_bytes()).lines();
            parse(lines).await
        })
        .unwrap()
    }

    #[test]
    fn sections_file() {
        let path = test_file("test_files/extra_sections.ass");
        let ass_file = parse_blocking(&path).0;

        assert_eq!(ass_file.styles.len(), 1);
        assert_eq!(
            ass_file.styles[0].primary_colour,
            Colour {
                red: 255,
                green: 0,
                blue: 0,
            }
        );

        assert_eq!(ass_file.script_info.playback_resolution.x, 1920);
        assert_eq!(ass_file.attachments.len(), 1);
        assert_matches!(
            ass_file.attachments[0].attachment_type,
            AttachmentType::Graphic
        );

        let (_, event5) = ass_file.events.nth(5);
        assert_eq!(event5.style_index, 0);
        assert_matches!(
            ass_file.extradata.nde_filter_for_event(event5),
            Some(filter)
        );
        assert_eq!(filter.graph.nodes.len(), 4);
    }

    #[test]
    fn inline_decode() {
        assert_eq!(aegi_inline_string_decode("abcd"), "abcd");
        assert_eq!(aegi_inline_string_decode("abc#2Cd"), "abc,d");
        assert_eq!(aegi_inline_string_decode("abc#2C"), "abc,");
        assert_eq!(aegi_inline_string_decode("abc#2"), "abc#2");
        assert_eq!(aegi_inline_string_decode("abc#2ä"), "abc#2ä");
        assert_eq!(aegi_inline_string_decode("abc#GGd"), "abc\0d");
    }

    #[test]
    fn style() -> Result<(), SubtitleParseError> {
        let style = parse_style_line(
            "Style: Default,Arial,20,&H000000FF,&H00FFFFFF,&HFF000000,&H00000000,1,0,0,0,100,100,0,0,1,2,2,2,10,10,10,1",
        )?;

        assert_eq!(style.name, "Default");
        assert_eq!(style.font_name, "Arial");
        assert_float_absolute_eq!(style.font_size, 20.0, f64::EPSILON);
        assert_eq!(
            style.primary_colour,
            Colour {
                red: 255,
                green: 0,
                blue: 0,
            }
        );
        assert_eq!(
            style.secondary_colour,
            Colour {
                red: 255,
                green: 255,
                blue: 255,
            }
        );
        assert_eq!(
            style.border_colour,
            Colour {
                red: 0,
                green: 0,
                blue: 0,
            }
        );
        assert_eq!(
            style.shadow_colour,
            Colour {
                red: 0,
                green: 0,
                blue: 0,
            }
        );
        assert_eq!(style.primary_transparency, Transparency(0));
        assert_eq!(style.secondary_transparency, Transparency(0));
        assert_eq!(style.border_transparency, Transparency(255));
        assert_eq!(style.shadow_transparency, Transparency(0));
        assert!(style.bold);
        assert!(!style.italic);
        assert!(!style.underline);
        assert!(!style.strike_out);
        assert_float_absolute_eq!(style.scale.x, 1.0, f64::EPSILON);
        assert_float_absolute_eq!(style.scale.y, 1.0, f64::EPSILON);
        assert_float_absolute_eq!(style.spacing, 0.0, f64::EPSILON);
        assert_eq!(style.angle, Angle(0.0));
        assert_eq!(style.border_style, BorderStyle::Default);
        assert_float_absolute_eq!(style.border_width, 2.0, f64::EPSILON);
        assert_float_absolute_eq!(style.shadow_distance, 2.0, f64::EPSILON);
        assert_eq!(
            style.alignment,
            Alignment {
                vertical: VerticalAlignment::Sub,
                horizontal: HorizontalAlignment::Center,
            }
        );
        assert_eq!(style.margins.left, 10);
        assert_eq!(style.margins.right, 10);
        assert_eq!(style.margins.vertical, 10);
        assert_eq!(style.encoding.0, 1);

        Ok(())
    }

    #[test]
    fn event() -> Result<(), SubtitleParseError> {
        let (event, style_name) = parse_event_line(
            r"Dialogue: 0,0:00:05.00,0:00:07.00,Default,,1,2,3,,{=8=10}{\fs100}asdhasjkldhsajk",
        )?;

        assert_eq!(style_name, "Default");
        assert_eq!(event.layer_index, 0);
        assert_eq!(event.start, StartTime(5000));
        assert_eq!(event.duration, Duration(2000));
        assert_eq!(event.margins.left, 1);
        assert_eq!(event.margins.right, 2);
        assert_eq!(event.margins.vertical, 3);
        assert_eq!(
            event.extradata_ids.as_slice(),
            &[ExtradataId(8), ExtradataId(10)]
        );
        assert_eq!(event.actor, "");
        assert_eq!(event.effect, "");
        assert_eq!(event.text, r"{\fs100}asdhasjkldhsajk");

        Ok(())
    }

    #[test]
    fn timecode() {
        // The shape Aegisub and libass write, and the components' relative weights.
        assert_eq!(timecode_to_millis("0:00:00.00"), Some(0));
        assert_eq!(timecode_to_millis("0:00:05.00"), Some(5000));
        assert_eq!(timecode_to_millis("1:02:03.04"), Some(3_723_040));
        assert_eq!(timecode_to_millis("9:59:59.99"), Some(35_999_990));

        // The fractional part is positional, so centiseconds and milliseconds both work.
        assert_eq!(timecode_to_millis("0:00:01.5"), Some(1500));
        assert_eq!(timecode_to_millis("0:00:01.50"), Some(1500));
        assert_eq!(timecode_to_millis("0:00:01.500"), Some(1500));
        assert_eq!(timecode_to_millis("0:00:01.05"), Some(1050));
        assert_eq!(timecode_to_millis("0:00:01.005"), Some(1005));
        assert_eq!(timecode_to_millis("0:00:01.123"), Some(1123));
        // Anything past millisecond precision is truncated rather than rounded.
        assert_eq!(timecode_to_millis("0:00:01.1239"), Some(1123));

        // `scanf` skips whitespace before every component and accepts a sign on each.
        assert_eq!(timecode_to_millis(" 0:00:01.00"), Some(1000));
        assert_eq!(timecode_to_millis("0: 0: 1.00"), Some(1000));
        assert_eq!(timecode_to_millis("-1:02:03.04"), Some(-3_476_960));
        assert_eq!(timecode_to_millis("+1:02:03.04"), Some(3_723_040));

        // Components are not width-limited, and trailing junk is ignored.
        assert_eq!(timecode_to_millis("0:00:100.00"), Some(100_000));
        assert_eq!(timecode_to_millis("0:00:01.00,Default,,"), Some(1000));

        // All four components are required, and the separators are literal.
        assert_eq!(timecode_to_millis(""), None);
        assert_eq!(timecode_to_millis("0:00:01"), None);
        assert_eq!(timecode_to_millis("0:00:01."), None);
        assert_eq!(timecode_to_millis("0:00.01"), None);
        assert_eq!(timecode_to_millis("0:00:01,00"), None);
        assert_eq!(timecode_to_millis("0:00:01x00"), None);
        // Unlike the regex this replaced, matching is anchored — there is no search.
        assert_eq!(timecode_to_millis("start=0:00:01.00"), None);
        // Neither is it Unicode-aware, which used to make `parse_timecode` panic.
        assert_eq!(timecode_to_millis("\u{661}:02:03.04"), None);

        // Overflow is reported rather than panicking or wrapping.
        assert_eq!(timecode_to_millis("9999999999999999999999:00:00.00"), None);
        assert_eq!(timecode_to_millis("9223372036854775807:00:00.00"), None);

        assert_matches!(
            parse_timecode("nonsense"),
            Err(SubtitleParseError::InvalidTimecode(text))
        );
        assert_eq!(text, "nonsense");
    }

    #[test]
    fn extradata_id_must_be_numeric() {
        let mut extradata = Extradata::default();

        // A well-formed line is accepted.
        parse_extradata_line("Data: 1,key,evalue", &mut extradata).unwrap();
        assert_eq!(extradata.entries.len(), 1);

        // A `Data:` line whose id is not a number is a hard error, rather than being dropped.
        assert_matches!(
            parse_extradata_line("Data: \u{661},key,evalue", &mut extradata),
            Err(SubtitleParseError::InvalidExtradataId(id))
        );
        assert_eq!(id, "\u{661}");
        assert_matches!(
            parse_extradata_line("Data: abc,key,evalue", &mut extradata),
            Err(SubtitleParseError::InvalidExtradataId(_))
        );

        // Lines that are not extradata at all are still ignored silently.
        parse_extradata_line("", &mut extradata).unwrap();
        parse_extradata_line("; a comment", &mut extradata).unwrap();
        parse_extradata_line("Data: no commas here", &mut extradata).unwrap();
        assert_eq!(extradata.entries.len(), 1);
    }

    #[test]
    fn script_info() -> Result<(), SubtitleParseError> {
        let mut info = ScriptInfo::default();

        parse_script_info_line("Title: samaku test", &mut info)?;
        parse_script_info_line("ScriptType: v4.00+", &mut info)?;
        parse_script_info_line("WrapStyle: 1", &mut info)?;
        parse_script_info_line("ScaledBorderAndShadow: yes", &mut info)?;
        parse_script_info_line("YCbCr Matrix: TV.709", &mut info)?;
        parse_script_info_line("PlayResX: 1920", &mut info)?;
        parse_script_info_line("PlayResY: 1080", &mut info)?;
        parse_script_info_line("LayoutResX: 1280", &mut info)?;
        parse_script_info_line("LayoutResY: 720", &mut info)?;

        assert_eq!(info.playback_resolution.x, 1920);
        assert_eq!(info.playback_resolution.y, 1080);
        assert_matches!(info.layout_resolution, Some(layout_resolution));
        assert_eq!(layout_resolution.x, 1280);
        assert_eq!(layout_resolution.y, 720);
        assert_eq!(info.wrap_style, WrapStyle::EndOfLine);
        assert!(info.scaled_border_and_shadow);
        assert_matches!(info.ycbcr_matrix, YCbCrMatrix::Bt709Tv);
        assert_matches!(info.extra_info.get("Title"), Some(value));
        assert_eq!(value, "samaku test");

        Ok(())
    }

    #[test]
    fn aegi_metadata() {
        let mut aegi_metadata = HashMap::new();
        parse_aegi_metadata_line("Key: Value", &mut aegi_metadata);
        assert_matches!(aegi_metadata.get("Key"), Some(value));
        assert_eq!(value, "Value");
    }

    #[test]
    fn extradata_line() {
        let mut extradata = Extradata::new();
        parse_extradata_line("Data: 2,_aegi_perspective_ambient_plane,e249.07;213.54#7C2170.22;302.89#7C2209.38;1199.91#7C-158.29;1040.20", &mut extradata).unwrap();
        assert_eq!(extradata.next_id, ExtradataId(3));

        let entry = &extradata[ExtradataId(2)];
        assert_matches!(entry, &ExtradataEntry::Opaque { ref key, ref value });
        assert_eq!(key, "_aegi_perspective_ambient_plane");
        assert_eq!(
            value,
            b"249.07;213.54|2170.22;302.89|2209.38;1199.91|-158.29;1040.20"
        );
    }

    #[test]
    fn extradata_references() {
        assert_matches!(parse_extradata_references("{}a"), None);
        assert_matches!(parse_extradata_references("{=}a"), None);
        assert_matches!(parse_extradata_references("{1}a"), None);
        assert_matches!(parse_extradata_references("{=1}a"), Some((refs, after)));
        assert_eq!(refs.as_slice(), &[ExtradataId(1)]);
        assert_eq!(after, 4);
        assert_matches!(parse_extradata_references("{=1=2}a"), Some((refs, after)));
        assert_eq!(refs.as_slice(), &[ExtradataId(1), ExtradataId(2)]);
        assert_eq!(after, 6);
        assert_matches!(
            parse_extradata_references("{=1234567890}a"),
            Some((refs, after))
        );
        assert_eq!(refs.as_slice(), &[ExtradataId(1_234_567_890)]);
        assert_eq!(after, 13);
        assert_matches!(parse_extradata_references("{==1}a"), None);
        assert_matches!(parse_extradata_references("{=1=2"), None);
        assert_matches!(parse_extradata_references("{=1a}b"), None);
        assert_matches!(parse_extradata_references("{=1ä}b"), None);
    }

    #[test]
    fn warning() {
        let (file, warnings) = parse_blocking(&test_file("test_files/parse_warnings.ass"));
        assert_eq!(warnings.len(), 2);
        assert_matches!(&warnings[0], &Warning::StyleOnLine(14, _));
        assert_matches!(&warnings[1], &Warning::UnmatchedStyle(_));
        assert_eq!(file.styles.len(), 1); // There should still be a style...
        assert_eq!(file.styles[0].name, "Default"); // ...but it should be the default one
    }
}
