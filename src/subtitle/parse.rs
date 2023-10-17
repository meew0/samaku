//! Functions for parsing `.ass` files. For parsing ASS override tags, see [`nde::tags::parse`]

use std::collections::HashMap;

use once_cell::sync::OnceCell;
use regex::Regex;
use smol::stream::StreamExt;
use thiserror::Error;

use crate::nde::tags::{Alignment, Colour, Transparency};
use crate::{nde, subtitle};

use super::{
    Angle, AssFile, Attachment, AttachmentType, BorderStyle, Duration, EventType, Extradata,
    ExtradataEntry, ExtradataId, JustifyMode, Margins, Scale, ScriptInfo, SideData, Sline,
    SlineTrack, StartTime, Style, YCbCrMatrix,
};

#[allow(clippy::too_many_lines)]
pub(super) async fn parse(
    mut input: smol::io::Lines<smol::io::BufReader<smol::fs::File>>,
) -> Result<AssFile, Error> {
    let mut state = ParseState::ScriptInfo;

    // Data of opaque/unknown sections
    let mut header = String::new();
    let mut section = String::new();
    let mut opaque_sections: HashMap<String, String> = HashMap::new();

    let mut current_attachment: Option<Attachment> = None;

    let mut style_lookup: HashMap<String, usize> = HashMap::new();
    let mut styles: Vec<Style> = vec![];
    let mut raw_slines_and_style_names: Vec<(Sline, String)> = vec![];
    let mut script_info = ScriptInfo::default();
    let mut extradata = Extradata::default();
    let mut aegi_metadata = HashMap::new();
    let mut attachments = vec![];

    while let Some(line_result) = input.next().await {
        let line_string = line_result.map_err(Error::IoError)?;
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
                section = String::new();
            }

            if line.eq_ignore_ascii_case("[v4 styles]") {
                return Err(Error::V4StylesFound);
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
                header.push_str(&line[1..(line.len() - 1)]);
            }

            continue;
        }

        match state {
            ParseState::Unknown => {
                section.push_str(&line_string);
            }
            ParseState::Styles => {
                if line.starts_with("Style:") {
                    let style = parse_style_line(line)?;
                    style_lookup.insert(style.name.clone(), styles.len());
                    styles.push(style);
                }
            }
            ParseState::Events => {
                if line.starts_with("Dialogue:") || line.starts_with("Comment:") {
                    raw_slines_and_style_names.push(parse_event_line(line)?);
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

    // Match event style names to styles, and construct sline track
    let mut slines: Vec<Sline> = vec![];
    for (mut raw_sline, style_name) in raw_slines_and_style_names {
        if let Some(style_index) = style_lookup.get(&style_name) {
            raw_sline.style_index = (*style_index).try_into().unwrap();
            slines.push(raw_sline);
        } else {
            return Err(Error::UnmatchedStyle(style_name));
        }
    }

    let subtitles = SlineTrack {
        slines,
        styles,
        extradata,
    };

    Ok(AssFile {
        script_info,
        subtitles,
        side_data: SideData {
            aegi_metadata,
            attachments,
            other_sections: opaque_sections,
        },
    })
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
pub enum Error {
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

    #[error("Style mentioned in event does not exist: {0}")]
    UnmatchedStyle(String),

    #[error("Invalid NDE filter format identifier: {0:?}")]
    InvalidNdeFilterFormat(Option<char>),

    #[error("Failed to decode base64 data for NDE filter: {0}")]
    NdeFilterBase64DecodeError(data_encoding::DecodeError),

    #[error("Failed to decompress NDE filter: {0}")]
    NdeFilterDecompressError(miniz_oxide::inflate::DecompressError),

    #[error("Failed to deserialise NDE filter: {0}")]
    NdeFilterDeserialiseError(String),
}

fn parse_style_line(line: &str) -> Result<Style, Error> {
    let Some((key, value)) = parse_kv_generic(line) else {
        return Err(Error::MalformedStyleLine);
    };

    if key != "Style" {
        return Err(Error::StyleLineInvalidKey);
    }

    let mut split = value.splitn(23, ',');

    let name = next_split_trim::<true>(&mut split)?.to_string();
    let font_name = next_split_trim::<true>(&mut split)?.to_string();
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

    let scale_x = next_split_f64(&mut split)?;
    let scale_y = next_split_f64(&mut split)?;

    let spacing = next_split_f64(&mut split)?;
    let angle = Angle(next_split_f64(&mut split)?);

    let border_style = BorderStyle::from(next_split_i32(&mut split)?);
    let border_width = next_split_f64(&mut split)?;
    let shadow_distance = next_split_f64(&mut split)?;
    let alignment =
        Alignment::try_from_an(next_split_i32(&mut split)?).ok_or(Error::InvalidAlignment)?;

    let margin_l = next_split_i32(&mut split)?;
    let margin_r = next_split_i32(&mut split)?;
    let margin_v = next_split_i32(&mut split)?;

    let encoding = next_split_i32(&mut split)?;

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

fn parse_event_line(line: &str) -> Result<(Sline, String), Error> {
    let (event_type, fields_str) = if let Some(fields_str) = line.strip_prefix("Dialogue: ") {
        (EventType::Dialogue, fields_str)
    } else if let Some(fields_str) = line.strip_prefix("Comment: ") {
        (EventType::Comment, fields_str)
    } else {
        return Err(Error::InvalidEventType(line.to_string()));
    };

    let mut split = fields_str.splitn(10, ',');

    // TODO: `Marked=`?
    // https://github.com/arch1t3cht/Aegisub/blob/d8c611d662480aea1fae6c438892b4327447765a/src/ass_dialogue.cpp#L106
    let layer = next_split_i32(&mut split)?;

    let start = parse_timecode(next_split_trim::<true>(&mut split)?)?;
    let end = parse_timecode(next_split_trim::<true>(&mut split)?)?;
    let style = next_split_trim::<true>(&mut split)?.to_string();
    let actor = next_split_trim::<true>(&mut split)?.to_string();

    let margin_l = next_split_i32(&mut split)?;
    let margin_r = next_split_i32(&mut split)?;
    let margin_v = next_split_i32(&mut split)?;

    let effect = next_split_trim::<true>(&mut split)?.to_string();

    // Aegisub only trims the event text at its end. We match that behaviour, because why not.
    let mut text = next_split_trim::<false>(&mut split)?;

    let mut extradata_ids: Vec<ExtradataId> = vec![];

    if text.starts_with("{=") {
        if let Some((new_extradata_ids, after)) = parse_extradata_references(text) {
            extradata_ids = new_extradata_ids;
            text = &text[after..];
        }
    }

    let new_sline = Sline {
        start: StartTime(start),
        duration: Duration(end - start),
        layer_index: layer,
        style_index: 0,
        margins: Margins {
            left: margin_l,
            right: margin_r,
            vertical: margin_v,
        },
        text: text.to_string(),
        actor,
        effect,
        event_type,
        extradata_ids,
    };

    Ok((new_sline, style))
}

fn parse_script_info_line(line: &str, script_info: &mut ScriptInfo) -> Result<(), Error> {
    if line.starts_with(';') {
        // Comment
        return Ok(());
    }

    if let Some(value) = line.strip_prefix("ScriptType:") {
        let version_str = value.trim().to_ascii_lowercase();
        if version_str != "v4.00+" {
            return Err(Error::UnsupportedScriptType);
        }
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
            .insert(key.to_string(), value.to_string());
    }

    Ok(())
}

fn parse_aegi_metadata_line(line: &str, aegi_metadata: &mut HashMap<String, String>) {
    if let Some((key, value)) = parse_kv_generic(line) {
        aegi_metadata.insert(key.to_string(), value.to_string());
    };
}

static EXTRADATA_REGEX: OnceCell<Regex> = OnceCell::new();

fn parse_extradata_line(line: &str, extradata: &mut Extradata) -> Result<(), Error> {
    let extradata_regex = EXTRADATA_REGEX
        .get_or_init(|| Regex::new("Data:[[:space:]]*(\\d+),([^,]+),(.)(.*)").unwrap());

    if let Some(captures) = extradata_regex.captures(line) {
        let id_str = captures.get(1).unwrap().as_str();
        let Ok(id_num) = id_str.parse::<u32>() else {
            println!("invalid extradata ID: {id_str}");
            return Ok(()); // ignore
        };

        let key = aegi_inline_string_decode(captures.get(2).unwrap().as_str());
        let value_type = captures.get(3).unwrap().as_str();
        let value_raw = captures.get(4).unwrap().as_str();

        let value = if value_type == "e" {
            aegi_inline_string_decode(value_raw)
        } else if value_type == "u" {
            todo!()
        } else {
            String::new()
        };

        extradata.next_id = extradata.next_id.max(ExtradataId(id_num + 1));
        extradata
            .entries
            .insert(ExtradataId(id_num), parse_extradata_entry(key, value)?);
    }

    Ok(())
}

fn parse_extradata_entry(key: String, value: String) -> Result<ExtradataEntry, Error> {
    if key == "_samaku_nde_filter" {
        let first_char = value.chars().next();
        if first_char == Some('1') {
            let base64 = &value.as_bytes()[1..];
            let decoded = data_encoding::BASE64
                .decode(base64)
                .map_err(Error::NdeFilterBase64DecodeError)?;
            let decompressed =
                miniz_oxide::inflate::decompress_to_vec_with_limit(decoded.as_slice(), 1_000_000)
                    .map_err(Error::NdeFilterDecompressError)?;
            let filter = ciborium::from_reader::<nde::Filter, _>(decompressed.as_slice())
                .map_err(|de_error| Error::NdeFilterDeserialiseError(format!("{de_error:?}")))?;

            Ok(ExtradataEntry::NdeFilter(filter))
        } else {
            Err(Error::InvalidNdeFilterFormat(first_char))
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
        filename: filename.to_string(),
        data: vec![],
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
    attachment.data.extend_from_slice(line.as_bytes());
}

fn parse_extradata_references(text: &str) -> Option<(Vec<ExtradataId>, usize)> {
    let mut res = vec![];
    let mut match_start: Option<usize> = None;

    for (i, char) in text.char_indices() {
        if i == 0 {
            if char == '{' {
                continue;
            }

            return None;
        }

        match char {
            '=' => {
                if let Some(match_start) = match_start.take() {
                    res.push(ExtradataId(text[match_start..i].parse::<u32>().unwrap()));
                } else if i != 1 {
                    // Double `=` are not allowed
                    return None;
                }
            }
            '0'..='9' => {
                if i == 1 {
                    // Needs a `=` before
                    return None;
                }

                match_start.get_or_insert(i);
            }
            '}' => {
                return if let Some(match_start) = match_start.take() {
                    res.push(ExtradataId(text[match_start..i].parse::<u32>().unwrap()));
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
) -> Result<&'a str, Error> {
    match split.next() {
        Some(str) => Ok(if TRIM_START {
            str.trim()
        } else {
            str.trim_end()
        }),
        None => Err(Error::TruncatedLine),
    }
}

fn next_split_i32(split: &mut std::str::SplitN<char>) -> Result<i32, Error> {
    next_split_trim::<true>(split)?
        .parse::<i32>()
        .map_err(Error::ParseIntError)
}

fn next_split_f64(split: &mut std::str::SplitN<char>) -> Result<f64, Error> {
    next_split_trim::<true>(split)?
        .parse::<f64>()
        .map_err(Error::ParseFloatError)
}

fn next_split_bool(split: &mut std::str::SplitN<char>) -> Result<bool, Error> {
    Ok(next_split_trim::<true>(split)?
        .parse::<u8>()
        .map_err(Error::ParseIntError)?
        != 0)
}

/// Parse a generic key/value line of the form `Key: Value`.
fn parse_kv_generic(line: &str) -> Option<(&str, &str)> {
    let Some(colon_pos) = line.find(':') else {
        // ignore lines without a colon
        return None;
    };

    let key = &line[0..colon_pos];
    let value = line[(colon_pos + 1)..].trim_start();
    Some((key, value))
}

static TIMECODE_REGEX: OnceCell<Regex> = OnceCell::new();

fn parse_timecode(timecode: &str) -> Result<i64, Error> {
    let timecode_regex =
        TIMECODE_REGEX.get_or_init(|| Regex::new("(\\d+):(\\d+):(\\d+).(\\d+)").unwrap());

    let Some(captures) = timecode_regex.captures(timecode) else {
        return Err(Error::InvalidTimecode(timecode.to_string()));
    };

    let hours = captures[1].parse::<i64>().unwrap();
    let minutes = captures[2].parse::<i64>().unwrap();
    let seconds = captures[3].parse::<i64>().unwrap();
    let centis = captures[4].parse::<i64>().unwrap();

    Ok(((hours * 60 + minutes) * 60 + seconds) * 1000 + centis * 10)
}

fn parse_packed_colour_and_transparency(
    packed_colour_hex: &str,
) -> Result<(Colour, Transparency), Error> {
    let prefix_stripped = packed_colour_hex
        .strip_prefix("&H")
        .or_else(|| packed_colour_hex.strip_prefix("&h"))
        .unwrap_or(packed_colour_hex);
    let suffix_stripped = prefix_stripped.strip_suffix('&').unwrap_or(prefix_stripped);
    let number = u32::from_str_radix(suffix_stripped, 16).map_err(Error::ParseIntError)?;

    Ok(subtitle::unpack_colour_and_transparency_tbgr(number))
}

fn aegi_inline_string_decode(input: &str) -> String {
    let input_byte_size = input.bytes().len();
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
            let represented_byte = u8::from_str_radix(&tag[1..], 16).unwrap_or(0);
            output.push(represented_byte as char);
            tag.clear();
        } else if tag.is_empty() {
            output.push(char);
        }
    }

    if !tag.is_empty() {
        output.push_str(&tag);
    }

    output
}

#[cfg(test)]
mod tests {
    use assert_matches2::assert_matches;
    use smol::io::AsyncBufReadExt;

    use crate::nde::tags::{HorizontalAlignment, VerticalAlignment, WrapStyle};
    use crate::test_utils::test_file;

    use super::*;

    #[test]
    fn sections_file() -> Result<(), Error> {
        let path = test_file("test_files/extra_sections.ass");

        let ass_file = smol::block_on(async {
            let lines = smol::io::BufReader::new(smol::fs::File::open(path).await.unwrap()).lines();
            parse(lines).await
        })?;

        assert_eq!(ass_file.script_info.playback_resolution.x, 1920);
        assert_eq!(ass_file.side_data.attachments.len(), 1);
        assert_matches!(
            ass_file.side_data.attachments[0].attachment_type,
            AttachmentType::Graphic
        );

        let sline5 = &ass_file.subtitles.slines[5];
        assert_matches!(
            ass_file.subtitles.extradata.nde_filter_for_sline(sline5),
            Some(filter)
        );
        assert_eq!(filter.graph.nodes.len(), 4);

        Ok(())
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
    fn style() -> Result<(), Error> {
        let style = parse_style_line("Style: Default,Arial,20,&H000000FF,&H00FFFFFF,&HFF000000,&H00000000,1,0,0,0,100,100,0,0,1,2,2,2,10,10,10,1")?;

        assert_eq!(style.name, "Default");
        assert_eq!(style.font_name, "Arial");
        assert!((style.font_size - 20.0).abs() < f64::EPSILON);
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
        assert!((style.scale.x - 100.0).abs() < f64::EPSILON);
        assert!((style.scale.y - 100.0).abs() < f64::EPSILON);
        assert!((style.spacing - 0.0).abs() < f64::EPSILON);
        assert_eq!(style.angle, Angle(0.0));
        assert_eq!(style.border_style, BorderStyle::Default);
        assert!((style.border_width - 2.0).abs() < f64::EPSILON);
        assert!((style.shadow_distance - 2.0).abs() < f64::EPSILON);
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
        assert_eq!(style.encoding, 1);

        Ok(())
    }

    #[test]
    fn event() -> Result<(), Error> {
        let (sline, style_name) = parse_event_line(
            r"Dialogue: 0,0:00:05.00,0:00:07.00,Default,,1,2,3,,{=8=10}{\fs100}asdhasjkldhsajk",
        )?;

        assert_eq!(style_name, "Default");
        assert_eq!(sline.layer_index, 0);
        assert_eq!(sline.start, StartTime(5000));
        assert_eq!(sline.duration, Duration(2000));
        assert_eq!(sline.margins.left, 1);
        assert_eq!(sline.margins.right, 2);
        assert_eq!(sline.margins.vertical, 3);
        assert_eq!(
            sline.extradata_ids.as_slice(),
            &[ExtradataId(8), ExtradataId(10)]
        );
        assert_eq!(sline.actor, "");
        assert_eq!(sline.effect, "");
        assert_eq!(sline.text, r"{\fs100}asdhasjkldhsajk");

        Ok(())
    }

    #[test]
    fn script_info() -> Result<(), Error> {
        let mut info = ScriptInfo::default();

        parse_script_info_line("Title: samaku test", &mut info)?;
        parse_script_info_line("ScriptType: v4.00+", &mut info)?;
        parse_script_info_line("WrapStyle: 1", &mut info)?;
        parse_script_info_line("ScaledBorderAndShadow: yes", &mut info)?;
        parse_script_info_line("YCbCr Matrix: TV.709", &mut info)?;
        parse_script_info_line("PlayResX: 1920", &mut info)?;
        parse_script_info_line("PlayResY: 1080", &mut info)?;

        assert_eq!(info.playback_resolution.x, 1920);
        assert_eq!(info.playback_resolution.y, 1080);
        assert_eq!(info.wrap_style, WrapStyle::EndOfLine);
        assert!(info.scaled_border_and_shadow);
        assert_matches!(info.ycbcr_matrix, YCbCrMatrix::Bt709Tv);
        assert_matches!(info.extra_info.get("Title"), Some(value));
        assert_eq!(value, "samaku test");

        Ok(())
    }

    #[test]
    fn aegi_metadata() {
        let mut a = HashMap::new();
        parse_aegi_metadata_line("Key: Value", &mut a);
        assert_matches!(a.get("Key"), Some(value));
        assert_eq!(value, "Value");
    }

    #[test]
    fn extradata_line() {
        let mut extradata = Extradata::new();
        parse_extradata_line("Data: 2,_aegi_perspective_ambient_plane,e249.07;213.54#7C2170.22;302.89#7C2209.38;1199.91#7C-158.29;1040.20", &mut extradata).unwrap();
        assert_eq!(extradata.next_id, ExtradataId(3));

        let entry = &extradata[ExtradataId(2)];
        assert_matches!(entry, ExtradataEntry::Opaque { key, value });
        assert_eq!(key, "_aegi_perspective_ambient_plane");
        assert_eq!(
            value,
            "249.07;213.54|2170.22;302.89|2209.38;1199.91|-158.29;1040.20"
        );
    }

    #[test]
    fn extradata_references() {
        assert_matches!(parse_extradata_references("{}a"), None);
        assert_matches!(parse_extradata_references("{=}a"), None);
        assert_matches!(parse_extradata_references("{1}a"), None);
        assert_matches!(parse_extradata_references("{=1}a"), Some((e, after)));
        assert_eq!(e.as_slice(), &[ExtradataId(1)]);
        assert_eq!(after, 4);
        assert_matches!(parse_extradata_references("{=1=2}a"), Some((e, after)));
        assert_eq!(e.as_slice(), &[ExtradataId(1), ExtradataId(2)]);
        assert_eq!(after, 6);
        assert_matches!(
            parse_extradata_references("{=1234567890}a"),
            Some((e, after))
        );
        assert_eq!(e.as_slice(), &[ExtradataId(1_234_567_890)]);
        assert_eq!(after, 13);
        assert_matches!(parse_extradata_references("{==1}a"), None);
        assert_matches!(parse_extradata_references("{=1=2"), None);
        assert_matches!(parse_extradata_references("{=1a}b"), None);
        assert_matches!(parse_extradata_references("{=1ä}b"), None);
    }
}
