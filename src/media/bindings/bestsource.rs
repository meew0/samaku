#![allow(
    dead_code,
    reason = "implements more of what bestsource does for now than is currently used in samaku"
)]

use std::ffi::c_void;

use bestsource_sys as bs;

#[derive(Debug, Clone, Copy)]
pub struct AudioProperties {
    pub format: AudioFormat,
    pub sample_rate: u32,
    pub channels: u32,
    pub channel_layout: u64,
    pub num_samples: i64,
    pub start_time: f64,
}

#[derive(Debug, Clone, Copy)]
pub struct AudioFormat {
    pub float: bool,
    pub bits: usize,
    pub bytes_per_sample: usize,
}

pub(crate) enum CacheMode {
    Disable = 0,
    Auto = 1,
    AlwaysWrite = 2,
}

pub(crate) struct BestAudioSource {
    internal: *mut c_void,
}

unsafe impl Send for BestAudioSource {}

impl BestAudioSource {
    #![allow(
        clippy::too_many_arguments,
        reason = "closely wraps native code with similar amount of arguments"
    )]
    pub(crate) fn new<P1: AsRef<std::path::Path>, P2: AsRef<std::path::Path>>(
        source_file: P1,
        track: i32,
        ajust_delay: i32,
        threads: i32,
        cache_mode: CacheMode,
        cache_path: P2,
        drc_scale: f64,
    ) -> BestAudioSource {
        let source_file_c = super::path_to_cstring(source_file);
        let cache_path_c = super::path_to_cstring(cache_path);

        let w = unsafe {
            bs::BestAudioSource_new(
                source_file_c.as_ptr(),
                track,
                ajust_delay,
                threads,
                cache_mode as i32,
                cache_path_c.as_ptr(),
                drc_scale,
                None, // TODO progress
            )
        };

        assert!(w.error <= 0, "error while constructing BestAudioSource");
        assert!(
            !w.value.is_null(),
            "got null pointer from BestAudioSource constructor"
        );

        BestAudioSource { internal: w.value }
    }

    pub(crate) fn get_track(&self) -> i32 {
        let w = unsafe { bs::BestAudioSource_GetTrack(self.internal) };
        assert!(w.error <= 0, "error in BestAudioSource::GetTrack");
        w.value
    }

    pub(crate) fn set_max_cache_size(&mut self, bytes: usize) {
        let err = unsafe { bs::BestAudioSource_SetMaxCacheSize(self.internal, bytes) };
        assert!(err <= 0, "error in BestAudioSource::SetMaxCacheSize");
    }

    pub(crate) fn set_seek_pre_roll(&mut self, samples: i64) {
        let err = unsafe { bs::BestAudioSource_SetSeekPreRoll(self.internal, samples) };
        assert!(err <= 0, "error in BestAudioSource::SetSeekPreRoll");
    }

    pub(crate) fn get_relative_start_time(&self, track: i32) -> f64 {
        let w = unsafe { bs::BestAudioSource_GetRelativeStartTime(self.internal, track) };
        assert!(
            w.error <= 0,
            "error in BestAudioSource::GetRelativeStartTime"
        );
        w.value
    }

    pub(crate) fn get_audio_properties(&self) -> AudioProperties {
        let bas_ap = unsafe { bs::BestAudioSource_GetAudioProperties(self.internal) };
        assert!(
            bas_ap.error <= 0,
            "error in BestAudioSource::GetAudioProperties"
        );

        #[expect(clippy::cast_sign_loss, reason = "converting from native code")]
        AudioProperties {
            format: AudioFormat {
                float: bas_ap.AF.Float != 0,
                bits: bas_ap.AF.Bits as usize,
                bytes_per_sample: bas_ap.AF.BytesPerSample as usize,
            },
            sample_rate: bas_ap.SampleRate as u32,
            channels: bas_ap.Channels as u32,
            channel_layout: bas_ap.ChannelLayout,
            num_samples: bas_ap.NumSamples,
            start_time: bas_ap.StartTime,
        }
    }

    // TODO: get_planar_audio

    pub(crate) fn get_packed_audio(&mut self, slice: &mut [u8], start: i64, count: i64) {
        let err = unsafe {
            bs::BestAudioSource_GetPackedAudio(self.internal, slice.as_mut_ptr(), start, count)
        };
        assert!(err <= 0, "error in BestAudioSource::GetPackedAudio");
    }
}

impl Drop for BestAudioSource {
    fn drop(&mut self) {
        let err = unsafe { bs::BestAudioSource_delete(self.internal) };
        assert!(
            err <= 0 || std::thread::panicking(),
            "error while freeing BestAudioSource"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_properties_and_decoding() {
        let music_path = crate::test_utils::test_file("test_files/music.mp3");

        let mut bas = BestAudioSource::new(
            music_path,
            -1,
            -1,
            0,
            CacheMode::Disable,
            std::path::Path::new(""),
            0.0,
        );
        let properties = bas.get_audio_properties();

        assert_eq!(properties.sample_rate, 44100);
        assert_eq!(properties.channels, 2);

        let mut slice = vec![0_u8; 256 * properties.format.bytes_per_sample];
        bas.get_packed_audio(&mut slice, 88200, 128);
        assert_ne!(
            slice,
            vec![0_u8; 256 * properties.format.bytes_per_sample],
            "there should be some audio data"
        );
    }
}
