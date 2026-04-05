use anyhow::Context as _;

use super::bindings::ffms2;
pub use ffms2::AudioProperties as Properties;
pub(crate) use ffms2::init;

pub struct Audio {
    source: ffms2::AudioSource,
    pub properties: Properties,
}

impl Audio {
    pub fn load<P: AsRef<std::path::Path>>(filename: P) -> anyhow::Result<Self> {
        let mut indexer = ffms2::Indexer::new(filename.as_ref()).context("creating indexer")?;
        indexer.set_track_type_index_settings(ffms2::TrackType::Audio, 1);
        let mut index = indexer
            .do_indexing(ffms2::IndexErrorHandling::Abort)
            .context("indexing")?;

        let first_audio_track = index
            .first_track_of_type(ffms2::TrackType::Audio)
            .context("finding first audio track")?;
        let source = ffms2::AudioSource::new(
            filename.as_ref(),
            first_audio_track,
            &index,
            ffms2::AudioDelayMode::FirstVideoTrack,
        )
        .context("creating audio source")?;

        let properties = source.properties.clone();
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
        self.source
            .get_audio(start_frame as usize, count_frames as usize, data)
            .unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_music_mp3() {
        init();

        let audio = Audio::load(crate::test_utils::test_file("test_files/music.mp3")).unwrap();

        assert_eq!(audio.properties.channels, 2);
        assert_eq!(audio.properties.sample_rate, 44100);
        assert_eq!(audio.properties.sample_format, cpal::SampleFormat::F32);
    }

    #[test]
    fn read_audio_frames() {
        init();

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
