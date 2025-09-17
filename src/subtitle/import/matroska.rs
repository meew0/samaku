use crate::subtitle;
use anyhow::Context as _;
use matroska_demuxer::{
    ContentCompAlgo, ContentEncoding, ContentEncodingValue, Frame, MatroskaFile, TrackType,
};
use std::borrow::Cow;
use std::fmt::Write;
use std::fs::File;
use std::path::Path;
use thiserror::Error;

pub(crate) fn open_and_read(path: &Path) -> anyhow::Result<String> {
    let file = File::open(path).context("Failed to open file")?;
    let matroska = read_first_subtitle_track_to_ass(&file)
        .context("Failed to read subtitles from matroska file")?;
    Ok(matroska)
}

/// Reads the first subtitle track from the given Matroska file into a string parseable e.g. by libass.
fn read_first_subtitle_track_to_ass(file: &File) -> anyhow::Result<String> {
    let mut matroska_file = MatroskaFile::open(file).map_err(LoadError::DemuxErrorLoad)?;
    let track = matroska_file
        .tracks()
        .iter()
        .find(|track| {
            track.track_type() == TrackType::Subtitle
                && (track.codec_id() == "S_TEXT/SSA" || track.codec_id() == "S_TEXT/ASS")
        })
        .ok_or(LoadError::NoSubtitleTrackFound)?;

    let track_number: u64 = track.track_number().into();

    let decode_fns = find_decode_fns(track.content_encodings())?;

    let codec_private = track
        .codec_private()
        .ok_or(LoadError::MissingCodecPrivate)?;
    let codec_private_decoded = (decode_fns.private)(codec_private)?;
    let codec_private_parsed =
        str::from_utf8(&codec_private_decoded).map_err(LoadError::CodecPrivateDecodeFailed)?;

    let mut result = codec_private_parsed.to_owned();

    let mut frame = Frame::default();
    while matroska_file
        .next_frame(&mut frame)
        .map_err(LoadError::DemuxErrorNextFrame)?
    {
        if frame.track == track_number {
            write_ass_line_from_matroska_frame(&mut result, &frame, decode_fns.block)?;
        }
    }

    Ok(result)
}

fn write_ass_line_from_matroska_frame<W: Write>(
    writer: &mut W,
    frame: &Frame,
    decode_fn: DecodeFn,
) -> anyhow::Result<()> {
    let duration = frame
        .duration
        .ok_or(LoadError::MissingDuration(frame.timestamp))?;
    let decoded = decode_fn(frame.data.as_slice())?;
    let content = str::from_utf8(&decoded)
        .map_err(|err| LoadError::FrameDecodeFailed(err, frame.timestamp))?;

    let (_index, tail) = content
        .split_once(',')
        .ok_or(LoadError::InvalidSubtitleFormat(frame.timestamp))?;
    let (layer, tail) = tail
        .split_once(',')
        .ok_or(LoadError::InvalidSubtitleFormat(frame.timestamp))?;

    write!(writer, "Dialogue: {layer},").map_err(LoadError::FormatError)?;

    let start_time: i64 = frame
        .timestamp
        .try_into()
        .map_err(|_| LoadError::TimingOverflow(frame.timestamp))?;
    let end_time = duration
        .try_into()
        .map_err(|_| LoadError::TimingOverflow(frame.timestamp))
        .and_then(|duration: i64| {
            duration
                .checked_add(start_time)
                .ok_or(LoadError::TimingOverflow(frame.timestamp))
        })?;

    subtitle::emit_timecode(writer, subtitle::StartTime(start_time))
        .map_err(LoadError::FormatError)?;
    writer.write_char(',').map_err(LoadError::FormatError)?;
    subtitle::emit_timecode(writer, subtitle::StartTime(end_time))
        .map_err(LoadError::FormatError)?;
    write!(writer, ",{tail}\r\n").map_err(LoadError::FormatError)?;

    Ok(())
}

type DecodeFn = fn(&[u8]) -> anyhow::Result<Cow<[u8]>>;

struct DecodeFnSet {
    block: DecodeFn,
    private: DecodeFn,
}

const SCOPE_BLOCK: u64 = 1;
const SCOPE_PRIVATE: u64 = 2;

fn find_decode_fns(content_encodings: Option<&[ContentEncoding]>) -> anyhow::Result<DecodeFnSet> {
    match content_encodings {
        None => Ok(DecodeFnSet {
            block: decode_identity,
            private: decode_identity,
        }),
        Some(content_encodings) => {
            let num_encodings = content_encodings.len();
            if num_encodings != 1 {
                anyhow::bail!("Unsupported number of content encodings: {num_encodings} != 1");
            }

            let content_encoding = &content_encodings[0];
            let encoding_value = content_encoding.encoding();
            let decode_fn = match encoding_value {
                ContentEncodingValue::Unknown | ContentEncodingValue::Encryption(_) => {
                    anyhow::bail!("Invalid content encoding value: {encoding_value:?}");
                }
                ContentEncodingValue::Compression(compression) => {
                    if compression.algo() != ContentCompAlgo::Zlib {
                        anyhow::bail!(
                            "Unsupported compression algorithm: {:?}",
                            compression.algo()
                        );
                    }

                    decode_zlib
                }
            };

            // Make sure we only return the decode function for the elements covered by the content encoding, specified in the “scope” value
            let scope = content_encoding.scope();
            let set = DecodeFnSet {
                block: scope_select(scope, SCOPE_BLOCK, decode_fn, decode_identity),
                private: scope_select(scope, SCOPE_PRIVATE, decode_fn, decode_identity),
            };

            Ok(set)
        }
    }
}

fn scope_select<T>(scope: u64, query: u64, yes: T, no: T) -> T {
    if scope & query == 0 { no } else { yes }
}

#[expect(
    clippy::unnecessary_wraps,
    reason = "needs to match a common function signature"
)]
fn decode_identity(data: &'_ [u8]) -> anyhow::Result<Cow<'_, [u8]>> {
    Ok(Cow::Borrowed(data))
}

fn decode_zlib(data: &'_ [u8]) -> anyhow::Result<Cow<'_, [u8]>> {
    let decompressed = miniz_oxide::inflate::decompress_to_vec_zlib(data)
        .map_err(DecompressError::DecompressError)?;
    Ok(Cow::Owned(decompressed))
}

#[derive(Error, Debug)]
pub(crate) enum DecompressError {
    #[error("Error during zlib decompression: {0:?}")]
    DecompressError(miniz_oxide::inflate::DecompressError),
}

#[derive(Error, Debug)]
pub(crate) enum LoadError {
    #[error("Demux error on initial load: {0:?}")]
    DemuxErrorLoad(matroska_demuxer::DemuxError),

    #[error("Demux error while reading next frame: {0:?}")]
    DemuxErrorNextFrame(matroska_demuxer::DemuxError),

    #[error("Matroska file contains no subtitle track")]
    NoSubtitleTrackFound,

    #[error("Subtitle track has no CodecPrivate header")]
    MissingCodecPrivate,

    #[error("Invalid UTF-8 in CodecPrivate: {0:?}")]
    CodecPrivateDecodeFailed(std::str::Utf8Error),

    #[error("Invalid UTF-8 in frame: {0:?} at {1} ms")]
    FrameDecodeFailed(std::str::Utf8Error, u64),

    #[error("Failed to format subtitle line: {0:?}")]
    FormatError(std::fmt::Error),

    #[error("Invalid subtitle format at {0} ms")]
    InvalidSubtitleFormat(u64),

    #[error("Subtitle frame missing duration at {0} ms")]
    MissingDuration(u64),

    #[error("Subtitle timing overflow at {0} ms")]
    TimingOverflow(u64),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media;

    #[test]
    fn subtitle_from_mkv() -> anyhow::Result<()> {
        let file = &File::open(crate::test_utils::test_file("test_files/cube_sub_ass.mkv"))?;
        let ass_data = read_first_subtitle_track_to_ass(file)?;

        assert!(ass_data.contains("Style: Style 2,Arial,48,&H00FFFFFF,&H000000FF,&H00000000,&H7F000000,-1,0,0,0,100,100,0,0,1,3,2,2,10,10,10,1"));
        assert!(ass_data.contains(
            "Dialogue: 0,0:00:00.00,0:00:02.00,Default,,0,0,0,,Sphinx of black quartz, judge my vow"
        ));
        assert!(ass_data.contains("色は匂えど散りぬるを"));

        let track = media::subtitle::OpaqueTrack::parse(&ass_data);
        assert_eq!(track.script_info().playback_resolution.x, 320);
        let events = track.to_event_track();
        assert_eq!(events.as_slice()[1].start, subtitle::StartTime(1000));
        assert_eq!(events.as_slice()[1].text, "色は匂えど散りぬるを");
        assert_eq!(
            track.styles()[events.as_slice()[1].style_index].name,
            "Style 2"
        );

        Ok(())
    }

    #[test]
    fn subtitle_from_mks_compressed() -> anyhow::Result<()> {
        let file = &File::open(crate::test_utils::test_file(
            "test_files/style_colours_zlib.mks",
        ))?;
        let ass_data = read_first_subtitle_track_to_ass(file)?;

        assert!(ass_data.contains("Video File: ?dummy:24000/1001:40000:1920:1080:47:163:254:"));
        assert!(
            ass_data.contains("Dialogue: 0,0:00:00.00,0:00:02.00,Default,,0,0,0,,Default style")
        );

        Ok(())
    }
}
