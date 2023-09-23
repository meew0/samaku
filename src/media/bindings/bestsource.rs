#![allow(dead_code)]

use std::ffi::c_void;

use bestsource_sys as bs;

#[derive(Debug, Clone, Copy)]
pub struct AudioProperties {
    pub is_float: bool,
    pub bytes_per_sample: i32,
    pub bits_per_sample: i32,
    pub sample_rate: i32,
    pub channels: i32,
    pub channel_layout: u64,
    pub num_samples: i64,
    pub start_time: f64,
}

pub struct BestAudioSource {
    internal: *mut c_void,
}

unsafe impl Send for BestAudioSource {}

impl BestAudioSource {
    pub fn new<P1: AsRef<std::path::Path>, P2: AsRef<std::path::Path>>(
        source_file: P1,
        track: i32,
        ajust_delay: i32,
        threads: i32,
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
                cache_path_c.as_ptr(),
                drc_scale,
            )
        };

        if w.error > 0 {
            panic!("error while constructing BestAudioSource");
        }

        if w.value.is_null() {
            panic!("got null pointer from BestAudioSource constructor");
        }

        BestAudioSource { internal: w.value }
    }

    pub fn get_track(&self) -> i32 {
        let w = unsafe { bs::BestAudioSource_GetTrack(self.internal) };
        if w.error > 0 {
            panic!("error in BestAudioSource::GetTrack");
        }
        w.value
    }

    pub fn set_max_cache_size(&mut self, bytes: usize) {
        let err = unsafe { bs::BestAudioSource_SetMaxCacheSize(self.internal, bytes) };
        if err > 0 {
            panic!("error in BestAudioSource::SetMaxCacheSize");
        }
    }

    pub fn set_seek_pre_roll(&mut self, samples: i64) {
        let err = unsafe { bs::BestAudioSource_SetSeekPreRoll(self.internal, samples) };
        if err > 0 {
            panic!("error in BestAudioSource::SetSeekPreRoll");
        }
    }

    pub fn get_relative_start_time(&self, track: i32) -> f64 {
        let w = unsafe { bs::BestAudioSource_GetRelativeStartTime(self.internal, track) };
        if w.error > 0 {
            panic!("error in BestAudioSource::GetRelativeStartTime");
        }
        w.value
    }

    // This is not declared as const in the c++ header file,
    // so I'm defining it as requiring a &mut self...
    pub fn get_exact_duration(&mut self) -> bool {
        let w = unsafe { bs::BestAudioSource_GetExactDuration(self.internal) };
        if w.error > 0 {
            panic!("error in BestAudioSource::GetExactDuration");
        }
        w.value != 0
    }

    pub fn get_audio_properties(&self) -> AudioProperties {
        let bas_ap = unsafe { bs::BestAudioSource_GetAudioProperties(self.internal) };
        if bas_ap.error > 0 {
            panic!("error in BestAudioSource::GetAudioProperties");
        }
        AudioProperties {
            is_float: bas_ap.IsFloat != 0,
            bytes_per_sample: bas_ap.BytesPerSample,
            bits_per_sample: bas_ap.BitsPerSample,
            sample_rate: bas_ap.SampleRate,
            channels: bas_ap.Channels,
            channel_layout: bas_ap.ChannelLayout,
            num_samples: bas_ap.NumSamples,
            start_time: bas_ap.StartTime,
        }
    }

    // TODO: get_planar_audio

    pub fn get_packed_audio(&mut self, slice: &mut [u8], start: i64, count: i64) {
        let err = unsafe {
            bs::BestAudioSource_GetPackedAudio(self.internal, slice.as_mut_ptr(), start, count)
        };
        if err > 0 {
            panic!("error in BestAudioSource::GetPackedAudio");
        }
    }
}

impl Drop for BestAudioSource {
    fn drop(&mut self) {
        let err = unsafe { bs::BestAudioSource_delete(self.internal) };
        if err > 0 && !std::thread::panicking() {
            panic!("error while freeing BestAudioSource");
        }
    }
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::path::Path;

    use super::*;

    #[test]
    fn audio_properties_and_decoding() {
        let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
        let music_path = Path::new(&manifest_dir).join("test_files/music.mp3");
        if !music_path.exists() {
            panic!("Could not find test data (test_files/music.mp3)! Perhaps some relative-path problem?");
        }

        let mut bas = BestAudioSource::new(music_path, -1, -1, 0, Path::new(""), 0.0);
        let properties = bas.get_audio_properties();

        assert_eq!(properties.sample_rate, 44100);
        assert_eq!(properties.channels, 2);

        let mut slice = vec![0u8; 256 * properties.bytes_per_sample as usize];
        bas.get_packed_audio(&mut slice, 88200, 128);
        assert_ne!(
            slice,
            vec![0u8; 256 * properties.bytes_per_sample as usize],
            "there should be some audio data"
        );
    }
}
