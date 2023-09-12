use core::slice;
use std::{
    mem::size_of,
    path::Path,
    sync::{
        atomic::{AtomicI64, Ordering},
        Arc, Mutex,
    },
};

use cpal::traits::{DeviceTrait, HostTrait};

use super::bindings::{bestsource, c_string};

const DEFAULT_SCRIPT: &str = include_str!("default_scripts/audio.py");

pub struct Audio {
    properties: bestsource::AudioProperties,
    current_pos: Arc<Mutex<i64>>,
    stream: cpal::Stream,
}

impl Audio {
    pub fn load<P: AsRef<Path>>(filename: P) -> Audio {
        let mut bas = bestsource::BestAudioSource::new(filename, -1, -1, 0, Path::new(""), 0.0);
        let properties = bas.get_audio_properties();

        println!("audio properties: {:?}", properties);

        // Find the cpal sample format that matches the audio properties
        let sample_format_opt: Option<cpal::SampleFormat> = if properties.is_float {
            const F32_SIZE: i32 = size_of::<f32>() as i32;
            const F64_SIZE: i32 = size_of::<f64>() as i32;
            match properties.bytes_per_sample {
                F32_SIZE => Some(cpal::SampleFormat::F32),
                F64_SIZE => Some(cpal::SampleFormat::F64),
                _ => None,
            }
        } else {
            const U8_SIZE: i32 = size_of::<u8>() as i32;
            const I16_SIZE: i32 = size_of::<i16>() as i32;
            const I32_SIZE: i32 = size_of::<i32>() as i32;
            match properties.bytes_per_sample {
                U8_SIZE => Some(cpal::SampleFormat::U8),
                I16_SIZE => Some(cpal::SampleFormat::I16),
                I32_SIZE => Some(cpal::SampleFormat::I32),
                _ => None,
            }
        };

        let sample_format =
            sample_format_opt.expect("Audio sample format not representable by cpal");

        let mut config_opt: Option<cpal::SupportedStreamConfig> = None;

        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .expect("No audio output device available");

        // Try to find a cpal playback config that matches the properties
        for supported_config in device
            .supported_output_configs()
            .expect("Error while querying audio output configurations")
        {
            if properties.channels == supported_config.channels().into()
                && properties.sample_rate >= supported_config.min_sample_rate().0 as i32
                && properties.sample_rate <= supported_config.max_sample_rate().0 as i32
                && sample_format == supported_config.sample_format()
            {
                config_opt =
                    Some(supported_config.with_sample_rate(cpal::SampleRate(
                        properties.sample_rate.try_into().unwrap(),
                    )))
            }
        }

        let config = config_opt.expect("Could not find a suitable system audio configuration that matches the loaded audio file");

        if sample_format != cpal::SampleFormat::F32 {
            todo!();
        }

        let current_pos = Arc::new(Mutex::new(0));
        let current_pos2 = Arc::clone(&current_pos);

        let stream = device
            .build_output_stream(
                &config.into(),
                move |data: &mut [f32], _| {
                    // Lock the mutex, so nothing tries to change the position
                    // between now and when we get the audio.
                    let mut pos = current_pos2.lock().unwrap();

                    // cpal expects packed audio. The buffer length refers to the
                    // number of samples (so frames * channels)
                    let num_samples = data.len() as i64;

                    // BS' parameters refer to the number of frames, so we
                    // need to divide by the number of channels
                    let num_frames = num_samples / properties.channels as i64;

                    // Transmute pointer to float
                    let data_u8 = unsafe {
                        slice::from_raw_parts_mut(
                            data.as_mut_ptr() as *mut u8,
                            data.len() * size_of::<f32>(),
                        )
                    };

                    bas.get_packed_audio(data_u8, *pos, num_frames);
                    println!("read {} frames", num_frames);
                    *pos += num_frames;
                },
                move |err| println!("Audio stream error: {}", err),
                None,
            )
            .expect("Failed to build audio stream");

        Audio {
            properties,
            current_pos,
            stream,
        }
    }
}
