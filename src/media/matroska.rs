use crate::subtitle;
use matroska_demuxer::{Frame, MatroskaFile, TrackType};
use std::fmt::Write;
use std::fs::File;
use thiserror::Error;

pub fn read(file: &File) -> Result<String, LoadError> {
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

    if track.content_encodings().is_some() {
        return Err(LoadError::ContentEncodingNotImplemented);
    }

    let codec_private = track
        .codec_private()
        .ok_or(LoadError::MissingCodecPrivate)?;
    let codec_private_parsed =
        str::from_utf8(codec_private).map_err(LoadError::CodecPrivateDecodeFailed)?;

    let mut result = codec_private_parsed.to_owned();

    let mut frame = Frame::default();
    while matroska_file
        .next_frame(&mut frame)
        .map_err(LoadError::DemuxErrorNextFrame)?
    {
        if frame.track == track_number {
            write_ass_line_from_matroska_frame(&mut result, &frame)?;
        }
    }

    Ok(result)
}

fn write_ass_line_from_matroska_frame<W: Write>(
    writer: &mut W,
    frame: &Frame,
) -> Result<(), LoadError> {
    let duration = frame
        .duration
        .ok_or(LoadError::MissingDuration(frame.timestamp))?;
    let content = str::from_utf8(frame.data.as_slice())
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

#[derive(Error, Debug)]
pub enum LoadError {
    #[error("Demux error on initial load: {0:?}")]
    DemuxErrorLoad(matroska_demuxer::DemuxError),

    #[error("Demux error while reading next frame: {0:?}")]
    DemuxErrorNextFrame(matroska_demuxer::DemuxError),

    #[error("Matroska file contains no subtitle track")]
    NoSubtitleTrackFound,

    #[error("Content encoding specified, this is not yet implemented")]
    ContentEncodingNotImplemented,

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
    fn subtitle_from_mkv() -> Result<(), LoadError> {
        let file =
            &File::open(crate::test_utils::test_file("test_files/cube_sub_ass.mkv")).unwrap();
        let ass_data = read(file)?;

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
}
