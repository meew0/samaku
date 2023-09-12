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
        let internal: *mut c_void = unsafe {
            bs::BestAudioSource_new(
                super::path_to_cstring(source_file).as_ptr(),
                track,
                ajust_delay,
                threads,
                super::path_to_cstring(cache_path).as_ptr(),
                drc_scale,
            )
        };
        BestAudioSource { internal }
    }

    pub fn get_track(&self) -> i32 {
        unsafe { bs::BestAudioSource_GetTrack(self.internal) }
    }

    pub fn set_max_cache_size(&mut self, bytes: usize) {
        unsafe {
            bs::BestAudioSource_SetMaxCacheSize(self.internal, bytes);
        }
    }

    pub fn set_seek_pre_roll(&mut self, samples: i64) {
        unsafe {
            bs::BestAudioSource_SetSeekPreRoll(self.internal, samples);
        }
    }

    pub fn get_relative_start_time(&self, track: i32) -> f64 {
        unsafe { bs::BestAudioSource_GetRelativeStartTime(self.internal, track) }
    }

    // This is not declared as const in the c++ header file,
    // so I'm defining it as requiring a &mut self...
    pub fn get_exact_duration(&mut self) -> bool {
        unsafe { bs::BestAudioSource_GetExactDuration(self.internal) != 0 }
    }

    pub fn get_audio_properties(&self) -> AudioProperties {
        let bas_ap = unsafe { bs::BestAudioSource_GetAudioProperties(self.internal) };
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
        unsafe {
            bs::BestAudioSource_GetPackedAudio(self.internal, slice.as_mut_ptr(), start, count)
        };
    }
}

impl Drop for BestAudioSource {
    fn drop(&mut self) {
        unsafe { bs::BestAudioSource_delete(self.internal) }
    }
}
