//! Barebones Matroska parser that can read subtitle tracks and attachments.
//!
//! Uses two passes, first a metadata pass that reads track infos and the attachments,
//! then a content pass that only reads the data belonging to subtitle tracks.
//! This avoids unnecessary memory allocation for video data or the like.

use super::{Attachment, Contents};
use crate::subtitle;
use std::borrow::Cow;
use std::fmt::Write as _;
use std::path::Path;
use thiserror::Error;

mod ebml;
mod id;

/// `TrackType` value denoting a subtitle track.
const TRACK_TYPE_SUBTITLE: u64 = 17;

/// `ContentEncodingType` value denoting compression rather than encryption.
const ENCODING_TYPE_COMPRESSION: u64 = 0;

/// `ContentCompAlgo` value denoting zlib.
const COMP_ALGO_ZLIB: u64 = 0;

/// `ContentEncodingScope` bit covering block data.
const SCOPE_BLOCK: u64 = 1;

/// `ContentEncodingScope` bit covering the codec private data.
const SCOPE_PRIVATE: u64 = 2;

/// The default `ContentEncodingScope`, which covers only block data.
const DEFAULT_ENCODING_SCOPE: u64 = SCOPE_BLOCK;

/// The default `TimestampScale`, in nanoseconds per tick. Practically every muxer uses this, which
/// makes one tick equal one millisecond.
const DEFAULT_TIMESTAMP_SCALE: u64 = 1_000_000;

const NANOS_PER_MILLISECOND: i128 = 1_000_000;

/// The largest number of bytes a track number inside a block can occupy.
const MAX_TRACK_NUMBER_LENGTH: u64 = 8;

pub(crate) async fn open_and_read(path: &Path) -> anyhow::Result<Contents> {
    let file = smol::fs::File::open(path).await?;
    read(file).await
}

/// Reads subtitles and attachments from the given Matroska file,
/// or other type of async stream representing Matroska data.
async fn read<R: smol::io::AsyncRead + smol::io::AsyncSeek + Unpin>(
    source: R,
) -> anyhow::Result<Contents> {
    let mut reader = ebml::Reader::new(source);

    read_ebml_header(&mut reader).await?;
    let segment_end = enter_segment(&mut reader).await?;

    let metadata = read_metadata(&mut reader, segment_end).await?;
    let track = metadata.track.ok_or(LoadError::NoSubtitleTrack)?;

    let codec_private = (track.decode.private)(&track.codec_private)?;
    let mut ass = String::from_utf8(codec_private.into_owned())
        .map_err(|error| LoadError::CodecPrivateNotUtf8(error.utf8_error()))?;

    if let Some(cluster_start) = metadata.first_cluster {
        reader.seek_to(cluster_start).await?;
        read_events(
            &mut reader,
            segment_end,
            metadata.timestamp_scale,
            &track,
            &mut ass,
        )
        .await?;
    }

    Ok(Contents {
        text: ass,
        attachments: metadata.attachments,
    })
}

/// Reads and validates the EBML header that every Matroska file starts with.
async fn read_ebml_header<R: smol::io::AsyncRead + smol::io::AsyncSeek + Unpin>(
    reader: &mut ebml::Reader<R>,
) -> Result<(), LoadError> {
    let header = reader.next_header().await?.ok_or(LoadError::NotMatroska)?;
    if header.id != id::EBML {
        return Err(LoadError::NotMatroska);
    }

    let body = reader.read_vec_size(header.size).await?;
    let mut cursor = ebml::Cursor::new(&body);
    let mut doc_type: Option<String> = None;
    while let Some(element) = cursor.next_element()? {
        if element.id == id::DOC_TYPE {
            doc_type = Some(ebml::read_string(element.body)?);
        }
    }

    match doc_type {
        // WebM cannot carry ASS subtitles or attachments, so there is nothing to gain by accepting it.
        Some(ref found) if found == "matroska" => Ok(()),
        Some(found) => Err(LoadError::UnsupportedDocType(found)),
        None => Err(LoadError::NotMatroska),
    }
}

/// Skips forward to the next segment and returns the offset at which it ends.
async fn enter_segment<R: smol::io::AsyncRead + smol::io::AsyncSeek + Unpin>(
    reader: &mut ebml::Reader<R>,
) -> Result<u64, LoadError> {
    loop {
        let header = reader.next_header().await?.ok_or(LoadError::NoSegment)?;
        if header.id == id::SEGMENT {
            // A segment of unknown size runs to the end of the file.
            // This is detected in the code below by the reader running out of input.
            return Ok(match header.size {
                ebml::Size::Known(size) => reader.position().saturating_add(size),
                ebml::Size::Unknown => u64::MAX,
            });
        }

        reader
            .skip(require_size(&header, "top-level element")?)
            .await?;
    }
}

/// All the data collected by the metadata pass.
struct Metadata {
    timestamp_scale: u64,
    track: Option<SubtitleTrack>,
    attachments: Vec<Attachment>,

    /// Where the first cluster's header starts, so the event pass can return to it.
    first_cluster: Option<u64>,
}

async fn read_metadata<R: smol::io::AsyncRead + smol::io::AsyncSeek + Unpin>(
    reader: &mut ebml::Reader<R>,
    segment_end: u64,
) -> Result<Metadata, LoadError> {
    let mut metadata = Metadata {
        timestamp_scale: DEFAULT_TIMESTAMP_SCALE,
        track: None,
        attachments: vec![],
        first_cluster: None,
    };

    while reader.position() < segment_end {
        let Some(header) = reader.next_header().await? else {
            break;
        };
        let size = header.size;

        match header.id {
            id::INFO => {
                let body = reader.read_vec_size(size).await?;
                metadata.timestamp_scale = read_timestamp_scale(&body)?;
            }
            id::TRACKS => {
                let body = reader.read_vec_size(size).await?;
                metadata.track = find_subtitle_track(&body)?;
            }
            id::ATTACHMENTS => {
                let body = reader.read_vec_size(size).await?;
                metadata.attachments = read_attachments(&body)?;
            }
            id::CLUSTER => {
                metadata.first_cluster.get_or_insert(header.header_start);
                reader.skip_size(size).await?;
            }
            _ => reader.skip_size(size).await?,
        }
    }

    Ok(metadata)
}

/// Walk over the clusters and turn the subtitle track's block groups into ASS events.
async fn read_events<R: smol::io::AsyncRead + smol::io::AsyncSeek + Unpin>(
    reader: &mut ebml::Reader<R>,
    segment_end: u64,
    timestamp_scale: u64,
    track: &SubtitleTrack,
    output: &mut String,
) -> Result<(), LoadError> {
    let mut cluster_timestamp: u64 = 0;

    while reader.position() < segment_end {
        let Some(header) = reader.next_header().await? else {
            break;
        };

        match header.id {
            // Descend one level down (the next iteration reads this cluster's first child)
            id::CLUSTER => {}
            id::TIMESTAMP => {
                let body = reader.read_vec_size(header.size).await?;
                cluster_timestamp = ebml::read_uint(&body)?;
            }
            id::BLOCK_GROUP => {
                let body = reader
                    .read_vec_size(header.size)
                    .await?;
                if let Some(event) = read_block_group(&body, track)? {
                    write_event(output, &event, cluster_timestamp, timestamp_scale, track)?;
                }
            }
            id::SIMPLE_BLOCK => {
                let size = require_size(&header, "SimpleBlock")?;
                skip_simple_block(reader, size, track).await?;
            }
            _ => reader.skip_size(header.size).await?,
        }
    }

    Ok(())
}

/// Steps over a simple block, while checking that it does not belong to the subtitle track.
///
/// Simple blocks carry no duration, so they cannot describe a subtitle event.
/// If a subtitle track uses them anyway for some reason, an error is returned.
async fn skip_simple_block<R: smol::io::AsyncRead + smol::io::AsyncSeek + Unpin>(
    reader: &mut ebml::Reader<R>,
    size: u64,
    track: &SubtitleTrack,
) -> Result<(), LoadError> {
    let peek_length = size.min(MAX_TRACK_NUMBER_LENGTH);
    let peeked = reader.read_vec(peek_length).await?;
    let (block_track, _length) = ebml::decode_vint(&peeked)?;

    if block_track == track.number {
        return Err(LoadError::SimpleBlockSubtitles);
    }

    reader.skip(size - peek_length).await?;
    Ok(())
}

fn read_timestamp_scale(body: &[u8]) -> Result<u64, LoadError> {
    let mut cursor = ebml::Cursor::new(body);
    while let Some(element) = cursor.next_element()? {
        if element.id == id::TIMESTAMP_SCALE {
            let scale = ebml::read_uint(element.body)?;
            return if scale == 0 {
                Err(LoadError::ZeroTimestampScale)
            } else {
                Ok(scale)
            };
        }
    }

    Ok(DEFAULT_TIMESTAMP_SCALE)
}

/// The subtitle track we decided to read, including the metadata necessary to interpret the blocks it contains.
struct SubtitleTrack {
    number: u64,
    codec_private: Vec<u8>,
    decode: DecodeFns,
}

fn find_subtitle_track(body: &[u8]) -> Result<Option<SubtitleTrack>, LoadError> {
    let mut cursor = ebml::Cursor::new(body);
    while let Some(element) = cursor.next_element()? {
        if element.id == id::TRACK_ENTRY
            && let Some(track) = read_track_entry(element.body)?
        {
            return Ok(Some(track));
        }
    }

    Ok(None)
}

/// Reads one track entry. If it is a supported subtitle track, we return it, otherwise, return `None`.
fn read_track_entry(body: &[u8]) -> Result<Option<SubtitleTrack>, LoadError> {
    let mut cursor = ebml::Cursor::new(body);
    let mut number: Option<u64> = None;
    let mut track_type: Option<u64> = None;
    let mut codec_id: Option<String> = None;
    let mut codec_private: Option<Vec<u8>> = None;
    let mut encodings: Option<&[u8]> = None;

    while let Some(element) = cursor.next_element()? {
        match element.id {
            id::TRACK_NUMBER => number = Some(ebml::read_uint(element.body)?),
            id::TRACK_TYPE => track_type = Some(ebml::read_uint(element.body)?),
            id::CODEC_ID => codec_id = Some(ebml::read_string(element.body)?),
            id::CODEC_PRIVATE => codec_private = Some(element.body.to_vec()),
            id::CONTENT_ENCODINGS => encodings = Some(element.body),
            _ => {}
        }
    }

    if track_type != Some(TRACK_TYPE_SUBTITLE) {
        return Ok(None);
    }
    let is_ass = codec_id
        .as_deref()
        .is_some_and(|codec| matches!(codec, "S_TEXT/ASS" | "S_TEXT/SSA"));
    if !is_ass {
        return Ok(None);
    }

    // Read the encodings only after we ensure the track is actually a supported subtitle track,
    // so that an exotically compressed video does not unnecessarily error out the whole import.
    let decode = match encodings {
        Some(encoding_data) => read_content_encodings(encoding_data)?,
        None => DecodeFns::identity(),
    };

    Ok(Some(SubtitleTrack {
        number: number.ok_or(LoadError::MissingTrackNumber)?,
        codec_private: codec_private.ok_or(LoadError::MissingCodecPrivate)?,
        decode,
    }))
}

fn read_attachments(body: &[u8]) -> Result<Vec<Attachment>, LoadError> {
    let mut cursor = ebml::Cursor::new(body);
    let mut attachments: Vec<Attachment> = vec![];

    while let Some(element) = cursor.next_element()? {
        if element.id == id::ATTACHED_FILE {
            attachments.push(read_attached_file(element.body)?);
        }
    }

    Ok(attachments)
}

fn read_attached_file(body: &[u8]) -> Result<Attachment, LoadError> {
    let mut cursor = ebml::Cursor::new(body);
    let mut name: Option<String> = None;
    let mut mime_type: Option<String> = None;
    let mut data: Option<Vec<u8>> = None;

    while let Some(element) = cursor.next_element()? {
        match element.id {
            id::FILE_NAME => name = Some(ebml::read_string(element.body)?),
            id::FILE_MIME_TYPE => mime_type = Some(ebml::read_string(element.body)?),
            id::FILE_DATA => data = Some(element.body.to_vec()),
            _ => {}
        }
    }

    // Ensure we have all the required data
    Ok(Attachment {
        name: name.ok_or(LoadError::IncompleteAttachment("FileName"))?,
        mime_type: mime_type.ok_or(LoadError::IncompleteAttachment("FileMimeType"))?,
        data: data.ok_or(LoadError::IncompleteAttachment("FileData"))?,
    })
}

/// A subtitle event as it appears inside a block group, with the data borrowing from the buffered block.
#[derive(Debug)]
struct BlockEvent<'a> {
    relative_timestamp: i16,
    duration_ticks: u64,
    data: &'a [u8],
}

/// Reads a block group, returning `Some` only if it holds a block belonging to the given `track`.
fn read_block_group<'a>(
    body: &'a [u8],
    track: &SubtitleTrack,
) -> Result<Option<BlockEvent<'a>>, LoadError> {
    let mut cursor = ebml::Cursor::new(body);
    let mut block: Option<&'a [u8]> = None;
    let mut duration: Option<u64> = None;

    while let Some(element) = cursor.next_element()? {
        match element.id {
            id::BLOCK => block = Some(element.body),
            id::BLOCK_DURATION => duration = Some(ebml::read_uint(element.body)?),
            _ => {}
        }
    }

    let Some(block_data) = block else {
        return Ok(None);
    };

    let (block_track, header_length) = ebml::decode_vint(block_data)?;
    if block_track != track.number {
        return Ok(None);
    }

    let rest = block_data
        .get(header_length..)
        .ok_or(LoadError::TruncatedBlock)?;
    let timestamp_bytes: [u8; 2] = rest
        .get(..2)
        .ok_or(LoadError::TruncatedBlock)?
        .try_into()
        .expect("slice of length 2 should convert into an array of length 2");
    let &flags = rest.get(2).ok_or(LoadError::TruncatedBlock)?;

    // “Lacing” is a Matroska feature that packs several frames into one block.
    // It exists to amortise block overhead for very small audio frames
    // and is not meaningfully applicable to subtitles, which need one duration per event,
    // so we simply reject the block in this case.
    if (flags & 0x06) >> 1_i32 != 0 {
        return Err(LoadError::LacedSubtitleBlock);
    }

    Ok(Some(BlockEvent {
        relative_timestamp: i16::from_be_bytes(timestamp_bytes),
        duration_ticks: duration.ok_or(LoadError::MissingBlockDuration)?,
        data: rest.get(3..).ok_or(LoadError::TruncatedBlock)?,
    }))
}

/// Converts a block into an ASS `Dialogue` line and appends it to `output`.
///
/// A subtitle block's payload is the tail of a dialogue line starting at the read order field:
/// `ReadOrder,Layer,Style,Name,MarginL,MarginR,MarginV,Effect,Text`. The read order is dropped and
/// the timings, which are stored in the container rather than the payload, are spliced back in,
/// to result in a line that can be parsed as an event by e.g. libass.
fn write_event(
    output: &mut String,
    event: &BlockEvent,
    cluster_timestamp: u64,
    timestamp_scale: u64,
    track: &SubtitleTrack,
) -> Result<(), LoadError> {
    let decoded = (track.decode.block)(event.data)?;
    let content = str::from_utf8(&decoded).map_err(LoadError::BlockNotUtf8)?;

    let cluster_ticks = i64::try_from(cluster_timestamp).map_err(LoadError::TimestampOutOfRange)?;
    let start_ticks = cluster_ticks
        .checked_add(i64::from(event.relative_timestamp))
        .ok_or(LoadError::TimestampOverflow)?;
    let duration_ticks =
        i64::try_from(event.duration_ticks).map_err(LoadError::TimestampOutOfRange)?;
    let end_ticks = start_ticks
        .checked_add(duration_ticks)
        .ok_or(LoadError::TimestampOverflow)?;

    let start = subtitle::StartTime(ticks_to_millis(start_ticks, timestamp_scale)?);
    let end = subtitle::StartTime(ticks_to_millis(end_ticks, timestamp_scale)?);

    let (_read_order, after_order) = content.split_once(',').ok_or(LoadError::MalformedEvent)?;
    let (layer, rest) = after_order
        .split_once(',')
        .ok_or(LoadError::MalformedEvent)?;

    write!(output, "Dialogue: {layer},").map_err(LoadError::Format)?;
    subtitle::emit_timecode(output, start).map_err(LoadError::Format)?;
    output.write_char(',').map_err(LoadError::Format)?;
    subtitle::emit_timecode(output, end).map_err(LoadError::Format)?;
    write!(output, ",{rest}\r\n").map_err(LoadError::Format)?;

    Ok(())
}

/// Converts a timestamp from container ticks into milliseconds.
///
/// `TimestampScale` gives the number of nanoseconds per tick.
/// Almost all Matroska files use ticks that are exactly one millisecond long,
/// but there is not much more effort involved in supporting exotic files
/// that use a different duration.
fn ticks_to_millis(ticks: i64, timestamp_scale: u64) -> Result<i64, LoadError> {
    let nanos = i128::from(ticks)
        .checked_mul(i128::from(timestamp_scale))
        .ok_or(LoadError::TimestampOverflow)?;
    i64::try_from(nanos / NANOS_PER_MILLISECOND).map_err(LoadError::TimestampOutOfRange)
}

fn require_size(header: &ebml::ElementHeader, what: &'static str) -> Result<u64, LoadError> {
    match header.size {
        ebml::Size::Known(value) => Ok(value),
        ebml::Size::Unknown => Err(LoadError::UnknownSize(what)),
    }
}

type DecodeFn = fn(&[u8]) -> Result<Cow<'_, [u8]>, LoadError>;

/// How to undo the content encoding applied to a track's block data and codec private data.
/// The two are separate because `ContentEncodingScope` may cover one without the other.
#[derive(Debug, Clone, Copy)]
struct DecodeFns {
    block: DecodeFn,
    private: DecodeFn,
}

impl DecodeFns {
    const fn identity() -> Self {
        Self {
            block: decode_identity,
            private: decode_identity,
        }
    }
}

fn read_content_encodings(body: &[u8]) -> Result<DecodeFns, LoadError> {
    let mut cursor = ebml::Cursor::new(body);
    let mut count: usize = 0;
    let mut result = DecodeFns::identity();

    while let Some(element) = cursor.next_element()? {
        if element.id == id::CONTENT_ENCODING {
            count += 1;
            result = read_content_encoding(element.body)?;
        }
    }

    if count == 1 {
        Ok(result)
    } else {
        Err(LoadError::UnsupportedEncodingCount(count))
    }
}

fn read_content_encoding(body: &[u8]) -> Result<DecodeFns, LoadError> {
    let mut cursor = ebml::Cursor::new(body);
    let mut scope = DEFAULT_ENCODING_SCOPE;
    let mut encoding_type = ENCODING_TYPE_COMPRESSION;
    let mut algo = COMP_ALGO_ZLIB;

    while let Some(element) = cursor.next_element()? {
        match element.id {
            id::CONTENT_ENCODING_SCOPE => scope = ebml::read_uint(element.body)?,
            id::CONTENT_ENCODING_TYPE => encoding_type = ebml::read_uint(element.body)?,
            id::CONTENT_COMPRESSION => algo = read_content_compression(element.body)?,
            _ => {}
        }
    }

    if encoding_type != ENCODING_TYPE_COMPRESSION {
        return Err(LoadError::EncryptedTrack);
    }
    if algo != COMP_ALGO_ZLIB {
        return Err(LoadError::UnsupportedCompression(algo));
    }

    Ok(DecodeFns {
        block: select_decoder(scope, SCOPE_BLOCK),
        private: select_decoder(scope, SCOPE_PRIVATE),
    })
}

fn read_content_compression(body: &[u8]) -> Result<u64, LoadError> {
    let mut cursor = ebml::Cursor::new(body);
    while let Some(element) = cursor.next_element()? {
        if element.id == id::CONTENT_COMP_ALGO {
            return Ok(ebml::read_uint(element.body)?);
        }
    }

    Ok(COMP_ALGO_ZLIB)
}

/// Picks the decoder for one scope bit: data outside the encoding's scope is left alone.
fn select_decoder(scope: u64, bit: u64) -> DecodeFn {
    if scope & bit == 0 {
        decode_identity
    } else {
        decode_zlib
    }
}

#[expect(
    clippy::unnecessary_wraps,
    reason = "needs to match the `DecodeFn` signature"
)]
fn decode_identity(data: &'_ [u8]) -> Result<Cow<'_, [u8]>, LoadError> {
    Ok(Cow::Borrowed(data))
}

fn decode_zlib(data: &'_ [u8]) -> Result<Cow<'_, [u8]>, LoadError> {
    let decompressed =
        miniz_oxide::inflate::decompress_to_vec_zlib(data).map_err(LoadError::Decompress)?;
    Ok(Cow::Owned(decompressed))
}

#[derive(Error, Debug)]
pub(crate) enum LoadError {
    #[error("Error while reading EBML structure: {0}")]
    Ebml(#[from] ebml::ParseError),

    #[error("File does not start with an EBML header")]
    NotMatroska,

    #[error("Unsupported EBML document type: {0}")]
    UnsupportedDocType(String),

    #[error("File contains no segment")]
    NoSegment,

    #[error("{0} declared an unknown size, which is not supported")]
    UnknownSize(&'static str),

    #[error("File contains no ASS or SSA subtitle track")]
    NoSubtitleTrack,

    #[error("Subtitle track has no TrackNumber")]
    MissingTrackNumber,

    #[error("Subtitle track has no CodecPrivate header")]
    MissingCodecPrivate,

    #[error("Attachment is missing its {0} element")]
    IncompleteAttachment(&'static str),

    #[error("Invalid UTF-8 in CodecPrivate: {0:?}")]
    CodecPrivateNotUtf8(std::str::Utf8Error),

    #[error("Invalid UTF-8 in subtitle block: {0:?}")]
    BlockNotUtf8(std::str::Utf8Error),

    #[error("Subtitle block is truncated")]
    TruncatedBlock,

    #[error("Subtitle block has no BlockDuration")]
    MissingBlockDuration,

    #[error("Laced subtitle blocks are not supported")]
    LacedSubtitleBlock,

    #[error("Subtitle track uses SimpleBlocks, which cannot carry a duration")]
    SimpleBlockSubtitles,

    #[error("Subtitle block does not have the expected ASS field layout")]
    MalformedEvent,

    #[error("TimestampScale is zero")]
    ZeroTimestampScale,

    #[error("Timestamp does not fit into the supported range: {0:?}")]
    TimestampOutOfRange(std::num::TryFromIntError),

    #[error("Timestamp arithmetic overflowed")]
    TimestampOverflow,

    #[error("Unsupported number of content encodings: {0} (expected exactly 1)")]
    UnsupportedEncodingCount(usize),

    #[error("Encrypted tracks are not supported")]
    EncryptedTrack,

    #[error("Unsupported compression algorithm: {0}")]
    UnsupportedCompression(u64),

    #[error("Error during zlib decompression: {0:?}")]
    Decompress(miniz_oxide::inflate::DecompressError),

    #[error("Failed to format subtitle line: {0:?}")]
    Format(std::fmt::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::test_file;
    use crate::{media, resources};
    use assert_matches2::assert_matches;

    fn read_file(name: &str) -> anyhow::Result<Contents> {
        smol::block_on(async {
            let data = smol::fs::read(test_file(format!("test_files/{name}"))).await?;
            read(smol::io::Cursor::new(data)).await
        })
    }

    #[test]
    fn subtitles_from_mkv() {
        media::subtitle::set_libass_test_callback();

        let contents = read_file("cube_sub_ass.mkv").unwrap();
        let ass = &contents.text;

        assert!(ass.contains("Style: Style 2,Arial,48,&H00FFFFFF,&H000000FF,&H00000000,&H7F000000,-1,0,0,0,100,100,0,0,1,3,2,2,10,10,10,1"));
        assert!(ass.contains(
            "Dialogue: 0,0:00:00.00,0:00:02.00,Default,,0,0,0,,Sphinx of black quartz, judge my vow"
        ));
        assert!(ass.contains("色は匂えど散りぬるを"));

        // The events have to survive a round trip through libass, which is what actually consumes
        // the string we build.
        let track = media::subtitle::OpaqueTrack::parse(ass);
        assert_eq!(track.script_info().playback_resolution.x, 320);
        let events = track.to_event_track();
        assert_eq!(events.nth(1).1.start, subtitle::StartTime(1000));
        assert_eq!(events.nth(1).1.text, "色は匂えど散りぬるを");
        assert_eq!(track.styles()[events.nth(1).1.style_index].name, "Style 2");
    }

    #[test]
    fn subtitles_from_compressed_mks() {
        // Test the zlib decoding path
        let contents = read_file("style_colours_zlib.mks").unwrap();
        let ass = &contents.text;

        assert!(ass.contains("Video File: ?dummy:24000/1001:40000:1920:1080:47:163:254:"));
        assert!(ass.contains("Dialogue: 0,0:00:00.00,0:00:02.00,Default,,0,0,0,,Default style"));
    }

    #[test]
    fn attachments() {
        let contents = read_file("cube_attachment.mkv").unwrap();

        assert_eq!(contents.attachments.len(), 1);
        let attachment = contents.attachments.first().unwrap();
        assert_eq!(attachment.name, "Barlow-Regular.ttf");
        assert_eq!(attachment.mime_type, "font/ttf");

        // The `cube_attachment.mkv` file contains the Barlow file as an attachment.
        // Check that the bytes are exactly identical.
        assert_eq!(attachment.data.as_slice(), resources::BARLOW_REGULAR);
    }

    #[test]
    fn subtitles_and_attachments_from_same_file() {
        // Attachments sit between the tracks and the clusters in this file, so finding both
        // proves the metadata pass steps over elements it does not care about and that the event
        // pass correctly returns to the first cluster afterwards.
        let contents = read_file("cube_attachment.mkv").unwrap();

        assert_eq!(contents.attachments.len(), 1);
        assert!(contents.text.contains("[Script Info]"));

        // This file interleaves the subtitle track with AV1 video, so the event pass has to step
        // over simple blocks belonging to another track to find it. The line below is byte-for-byte
        // what `mkvextract` produces for the same file.
        assert!(contents.text.contains(
            "Dialogue: 0,0:00:00.00,0:00:02.00,Default,,0,0,0,,Sphinx of black quartz, judge my vow"
        ));

        let track = media::subtitle::OpaqueTrack::parse(&contents.text);
        assert_eq!(track.to_event_track().len(), 1);
    }

    #[test]
    fn video_only_file() {
        assert_matches!(
            read_file("cube_h264.mkv")
                .unwrap_err()
                .downcast::<LoadError>()
                .unwrap(),
            LoadError::NoSubtitleTrack
        );
    }

    #[test]
    fn not_matroska() {
        smol::block_on(async {
            let data = b"[Script Info]\r\nTitle: not a matroska file\r\n".to_vec();
            let result = read(smol::io::Cursor::new(data)).await;
            assert_matches!(
                result.unwrap_err().downcast::<LoadError>().unwrap(),
                LoadError::NotMatroska
            );
        });
    }

    #[test]
    fn timestamp_scale() {
        // One tick is one millisecond at the default scale.
        assert_eq!(
            ticks_to_millis(1000, DEFAULT_TIMESTAMP_SCALE).unwrap(),
            1000
        );
        assert_eq!(ticks_to_millis(0, DEFAULT_TIMESTAMP_SCALE).unwrap(), 0);
        assert_eq!(ticks_to_millis(-5, DEFAULT_TIMESTAMP_SCALE).unwrap(), -5);

        // A file using a finer scale must not come out sped up: at 100 ns per tick, 10000 ticks
        // are one millisecond.
        assert_eq!(ticks_to_millis(10_000, 100).unwrap(), 1);
        assert_eq!(ticks_to_millis(1_234_000, 100).unwrap(), 123);

        // ...and a coarser one must not come out slowed down.
        assert_eq!(ticks_to_millis(2, 1_000_000_000).unwrap(), 2000);
    }

    #[test]
    fn laced() {
        let track = SubtitleTrack {
            number: 1,
            codec_private: vec![],
            decode: DecodeFns::identity(),
        };

        // A block group holding a block for track 1 with the Xiph lacing bits set, plus a
        // duration so that lacing is the only thing wrong with it.
        let body = [
            0xA1, 0x86, 0x81, 0x00, 0x00, 0x02, 0x01, b'x', // Block
            0x9B, 0x81, 0x64, // BlockDuration = 100
        ];

        assert_matches!(
            read_block_group(&body, &track),
            Err(LoadError::LacedSubtitleBlock)
        );
    }

    #[test]
    fn block_other_track() {
        let track = SubtitleTrack {
            number: 2,
            codec_private: vec![],
            decode: DecodeFns::identity(),
        };

        // Same shape as above, but for track 1 and without lacing.
        let body = [
            0xA1, 0x86, 0x81, 0x00, 0x00, 0x00, 0x01, b'x', // Block
            0x9B, 0x81, 0x64, // BlockDuration = 100
        ];

        assert_matches!(read_block_group(&body, &track), Ok(None));
    }

    #[test]
    fn block_missing_duration() {
        let track = SubtitleTrack {
            number: 1,
            codec_private: vec![],
            decode: DecodeFns::identity(),
        };

        let body = [0xA1, 0x86, 0x81, 0x00, 0x00, 0x00, 0x01, b'x'];

        assert_matches!(
            read_block_group(&body, &track),
            Err(LoadError::MissingBlockDuration)
        );
    }

    /// Whether a decoder actually inflates, checked by behaviour rather than by comparing function
    /// pointers. The identity decoder hands the compressed bytes straight back, so the two are
    /// unambiguously distinguishable.
    fn inflates(decode: DecodeFn) -> bool {
        let compressed = miniz_oxide::deflate::compress_to_vec_zlib(b"hello", 6);
        decode(&compressed).is_ok_and(|decoded| decoded.as_ref() == b"hello")
    }

    #[test]
    fn content_encoding_variants() {
        // No ContentEncodings at all leaves both scopes untouched.
        let identity = DecodeFns::identity();
        assert!(!inflates(identity.block));
        assert!(!inflates(identity.private));

        // A single zlib compression with the default scope covers blocks but not the codec
        // private data — getting this backwards would corrupt the ASS header.
        let default_scope = [0x62, 0x40, 0x83, 0x50, 0x34, 0x80];
        let decode = read_content_encodings(&default_scope).unwrap();
        assert!(inflates(decode.block));
        assert!(!inflates(decode.private));

        // Scope 3 covers both.
        let both_scopes = [0x62, 0x40, 0x87, 0x50, 0x32, 0x81, 0x03, 0x50, 0x34, 0x80];
        let decode = read_content_encodings(&both_scopes).unwrap();
        assert!(inflates(decode.block));
        assert!(inflates(decode.private));

        // Encryption (ContentEncodingType = 1) is refused rather than silently mangled.
        let encrypted = [0x62, 0x40, 0x84, 0x50, 0x33, 0x81, 0x01];
        assert_matches!(
            read_content_encodings(&encrypted),
            Err(LoadError::EncryptedTrack)
        );

        // Multiple stacked encodings are out of scope; we would have to apply them in order.
        let two_encodings = [
            0x62, 0x40, 0x83, 0x50, 0x34, 0x80, 0x62, 0x40, 0x83, 0x50, 0x34, 0x80,
        ];
        assert_matches!(
            read_content_encodings(&two_encodings),
            Err(LoadError::UnsupportedEncodingCount(2))
        );
    }
}
