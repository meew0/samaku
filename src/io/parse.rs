use std::collections::HashMap;

use once_cell::sync::OnceCell;
use regex::Regex;
use smol::stream::StreamExt;
use thiserror::Error;

use crate::subtitle;

use super::{AssFile, Attachment, AttachmentType, Extradata, ExtradataEntry, SideData};

/// Parse the given stream of lines into an [`AssFile`].
///
/// # Errors
/// Errors when the stream returns an IO error, or when an unrecoverable parse error is encountered.
/// The parser is quite tolerant, so this should not happen often.
pub async fn parse(
    mut input: smol::io::Lines<smol::io::BufReader<smol::fs::File>>,
) -> Result<super::AssFile, Error> {
    let mut state = ParseState::ScriptInfo;

    let mut current_attachment: Option<Attachment> = None;

    let mut script_info = subtitle::ScriptInfo::default();
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
            if line.eq_ignore_ascii_case("[v4 styles]") {
                state = ParseState::Styles(0);
            } else if line.eq_ignore_ascii_case("[v4+ styles]") {
                state = ParseState::Styles(1);
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
            }

            continue;
        }

        match state {
            ParseState::Unknown => todo!(),
            ParseState::Styles(_) => todo!(),
            ParseState::Events => todo!(),
            ParseState::ScriptInfo => {
                parse_script_info_line(line, &mut script_info)?;
            }
            ParseState::AegiMetadata => {
                parse_aegi_metadata_line(line, &mut aegi_metadata);
            }
            ParseState::Extradata => {
                parse_extradata_line(line, &mut extradata);
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

    Ok(AssFile {
        subtitles: subtitle::SlineTrack::default(),
        side_data: SideData {
            script_info,
            extradata,
            aegi_metadata,
            attachments,
            other_sections: HashMap::default(),
        },
    })
}

enum ParseState {
    Unknown,
    Styles(u8),
    Events,
    ScriptInfo,
    AegiMetadata,
    Extradata,
    Graphics,
    Fonts,
}

#[derive(Error, Debug)]
pub enum Error {
    #[error("IO error: {0}")]
    IoError(smol::io::Error),

    #[error("Script type must be v4.00+, all other versions are unsupported")]
    UnsupportedScriptType,
}

fn parse_script_info_line(line: &str, script_info: &mut subtitle::ScriptInfo) -> Result<(), Error> {
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
            "TV.601" => subtitle::ass::YCbCrMatrix::Bt601Tv,
            "PC.601" => subtitle::ass::YCbCrMatrix::Bt601Pc,
            "TV.709" => subtitle::ass::YCbCrMatrix::Bt709Tv,
            "PC.709" => subtitle::ass::YCbCrMatrix::Bt709Pc,
            "TV.FCC" => subtitle::ass::YCbCrMatrix::FccTv,
            "PC.FCC" => subtitle::ass::YCbCrMatrix::FccPc,
            "TV.240M" => subtitle::ass::YCbCrMatrix::Smtpe240MTv,
            "PC.240M" => subtitle::ass::YCbCrMatrix::Smtpe240MPc,
            _ => subtitle::ass::YCbCrMatrix::None,
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

fn parse_extradata_line(line: &str, extradata: &mut Extradata) {
    let extradata_regex = EXTRADATA_REGEX
        .get_or_init(|| Regex::new("Data:[[:space:]]*(\\d+),([^,]+),(.)(.*)").unwrap());

    if let Some(captures) = extradata_regex.captures(line) {
        let id_str = captures.get(1).unwrap().as_str();
        let Ok(id) = id_str.parse::<u32>() else {
            println!("invalid extradata ID: {id_str}");
            return; // ignore
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

        extradata.next_id = extradata.next_id.max(id + 1);
        extradata.entries.insert(id, ExtradataEntry { key, value });
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

    use super::*;

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
    fn script_info() -> Result<(), Error> {
        let mut info = subtitle::ScriptInfo::default();

        parse_script_info_line("Title: samaku test", &mut info)?;
        parse_script_info_line("ScriptType: v4.00+", &mut info)?;
        parse_script_info_line("WrapStyle: 1", &mut info)?;
        parse_script_info_line("ScaledBorderAndShadow: yes", &mut info)?;
        parse_script_info_line("YCbCr Matrix: TV.709", &mut info)?;
        parse_script_info_line("PlayResX: 1920", &mut info)?;
        parse_script_info_line("PlayResY: 1080", &mut info)?;

        assert_eq!(info.playback_resolution.x, 1920);
        assert_eq!(info.playback_resolution.y, 1080);
        assert_eq!(info.wrap_style, subtitle::WrapStyle::EndOfLine);
        assert!(info.scaled_border_and_shadow);
        assert_matches!(info.ycbcr_matrix, subtitle::ass::YCbCrMatrix::Bt709Tv);
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
        parse_extradata_line("Data: 2,_aegi_perspective_ambient_plane,e249.07;213.54#7C2170.22;302.89#7C2209.38;1199.91#7C-158.29;1040.20", &mut extradata);
        assert_eq!(extradata.next_id, 3);

        let entry = extradata.entries.get(&2_u32).unwrap();
        assert_eq!(entry.key, "_aegi_perspective_ambient_plane");
        assert_eq!(
            entry.value,
            "249.07;213.54|2170.22;302.89|2209.38;1199.91|-158.29;1040.20"
        );
    }
}
