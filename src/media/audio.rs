use anyhow::Context as _;

#[derive(Debug, Clone)]
pub struct Properties {
    pub channels: u16,
    pub sample_rate: u32,
    pub sample_format: cpal::SampleFormat,
}

pub struct Audio {
    source: ffms2::audio::AudioSource,
    pub properties: Properties,
}

impl Audio {
    pub fn load<P: AsRef<std::path::Path>>(filename: P) -> anyhow::Result<Self> {
        let indexer = ffms2::index::Indexer::new(filename.as_ref())
            .map_err(ffms2_map_error)
            .context("creating indexer")?;
        indexer.TrackTypeIndexSettings(ffms2::track::TrackType::TYPE_AUDIO, 1);
        let index = indexer
            .DoIndexing2(ffms2::IndexErrorHandling::IEH_ABORT)
            .map_err(ffms2_map_error)
            .context("indexing")?;

        println!(
            "delay_first_video_track = {:?}",
            ffms2::audio::AudioDelay::DELAY_FIRST_VIDEO_TRACK as isize
        );

        let source = ffms2::audio::AudioSource::new(
            filename.as_ref(),
            index
                .FirstTrackOfType(ffms2::track::TrackType::TYPE_AUDIO)
                .map_err(ffms2_map_error)
                .context("finding first audio track")?,
            &index,
            // DELAY_FIRST_VIDEO_TRACK
            // TODO report this to ffms2-rs, their enum `AudioDelay` doesn't match up with the values in ffms2.
            -1,
        )
        .map_err(ffms2_map_error)
        .context("creating audio source")?;
        let internal_properties = source.GetAudioProperties();

        println!("sample rate: {}", internal_properties.SampleRate);

        let properties = Properties {
            channels: internal_properties.Channels.try_into()?,
            sample_rate: internal_properties.SampleRate.try_into()?,
            sample_format: if internal_properties.SampleFormat
                == ffms2::SampleFormat::FMT_S16 as i32
            {
                cpal::SampleFormat::I16
            } else if internal_properties.SampleFormat == ffms2::SampleFormat::FMT_S32 as i32 {
                cpal::SampleFormat::I32
            } else if internal_properties.SampleFormat == ffms2::SampleFormat::FMT_U8 as i32 {
                cpal::SampleFormat::U8
            } else if internal_properties.SampleFormat == ffms2::SampleFormat::FMT_FLT as i32 {
                cpal::SampleFormat::F32
            } else if internal_properties.SampleFormat == ffms2::SampleFormat::FMT_DBL as i32 {
                cpal::SampleFormat::F64
            } else {
                anyhow::bail!(
                    "invalid sample format: {:?}",
                    internal_properties.SampleFormat
                );
            },
        };

        Ok(Self { source, properties })
    }

    /// Fills the given data with `count_frames` frames, starting from `start_frame`.
    ///
    /// # Panics
    /// Panics on overflow, or when FFMS2 fails to retrieve audio data.
    pub fn fill_buffer_packed<T>(&mut self, data: &mut [T], start_frame: u64, count_frames: u64)
    where
        T: Copy,
    {
        #[expect(clippy::cast_possible_truncation, reason = "64 bit only")]
        let vec: Vec<T> = self
            .source
            .GetAudio(start_frame as usize, count_frames as usize)
            .unwrap();

        // TODO replace this with a method that doesn't allocate and copy a buffer
        // (needs PR/fork to ffms2-rs since their only GetAudio method returns an allocated vec)
        data.copy_from_slice(vec.as_slice());
    }
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "needed to conveniently use the function without a closure"
)]
fn ffms2_map_error(err: ffms2::Error) -> anyhow::Error {
    anyhow::anyhow!("{err:?}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_music_mp3() {
        let audio = Audio::load(crate::test_utils::test_file("test_files/music.mp3")).unwrap();

        assert_eq!(audio.properties.channels, 2);
        assert_eq!(audio.properties.sample_rate, 44100);
        assert_eq!(audio.properties.sample_format, cpal::SampleFormat::F32);
    }

    #[test]
    fn read_audio_frames() {
        let mut audio = Audio::load(crate::test_utils::test_file("test_files/music.mp3")).unwrap();

        // Read 1024 frames starting from frame 1000 (packed: channels * frames samples)
        let count_frames: u64 = 1024;
        let channels = usize::from(audio.properties.channels);
        #[expect(clippy::cast_possible_truncation, reason = "64 bit only")]
        let mut buf = vec![0.0_f32; channels * count_frames as usize];
        audio.fill_buffer_packed(&mut buf, 1000, count_frames);

        // The decoded audio should contain some non-zero samples
        assert!(buf.iter().any(|sample| *sample != 0.0));
        // All samples should be in a valid float range for audio
        assert!(buf.iter().all(|sample| sample.is_finite()));
    }
}
