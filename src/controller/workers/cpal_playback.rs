use std::{
    mem::size_of,
    sync::{atomic::Ordering, Arc, Mutex},
};

use crate::{media, message, model};

pub fn spawn(
    tx_out: super::GlobalSender,
    global_state: &model::GlobalState,
) -> Option<super::Worker<cpal::Stream, ()>> {
    use cpal::traits::{DeviceTrait, HostTrait};

    // currently unused
    let (tx_in, _rx_in) = std::sync::mpsc::channel::<()>();

    let playback_state = Arc::clone(&global_state.playback_state);
    let audio_mutex = Arc::clone(&global_state.audio);

    let audio_properties = {
        let audio_lock = audio_mutex.lock().unwrap();
        if let Some(audio) = audio_lock.as_ref() {
            audio.properties
        } else {
            return None;
        }
    };

    // Find the cpal sample format that matches the audio properties
    let sample_format_opt: Option<cpal::SampleFormat> = if audio_properties.is_float {
        const F32_SIZE: i32 = size_of::<f32>() as i32;
        const F64_SIZE: i32 = size_of::<f64>() as i32;
        match audio_properties.bytes_per_sample {
            F32_SIZE => Some(cpal::SampleFormat::F32),
            F64_SIZE => Some(cpal::SampleFormat::F64),
            _ => None,
        }
    } else {
        const U8_SIZE: i32 = size_of::<u8>() as i32;
        const I16_SIZE: i32 = size_of::<i16>() as i32;
        const I32_SIZE: i32 = size_of::<i32>() as i32;
        match audio_properties.bytes_per_sample {
            U8_SIZE => Some(cpal::SampleFormat::U8),
            I16_SIZE => Some(cpal::SampleFormat::I16),
            I32_SIZE => Some(cpal::SampleFormat::I32),
            _ => None,
        }
    };

    let sample_format = sample_format_opt.expect("Audio sample format not representable by cpal");

    let mut config_opt: Option<cpal::SupportedStreamConfig> = None;

    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .expect("No audio output device available");

    // Try to find a cpal playback config that matches the audio_properties
    for supported_config in device
        .supported_output_configs()
        .expect("Error while querying audio output configurations")
    {
        if audio_properties.channels == supported_config.channels().into()
            && audio_properties.sample_rate >= supported_config.min_sample_rate().0 as i32
            && audio_properties.sample_rate <= supported_config.max_sample_rate().0 as i32
            && sample_format == supported_config.sample_format()
        {
            config_opt = Some(supported_config.with_sample_rate(cpal::SampleRate(
                audio_properties.sample_rate.try_into().unwrap(),
            )))
        }
    }

    let config = config_opt.expect(
        "Could not find a suitable system audio configuration that matches the loaded audio file",
    );

    if sample_format != cpal::SampleFormat::F32 {
        todo!();
    }

    playback_state
        .rate
        .store(audio_properties.sample_rate as u32, Ordering::Relaxed);

    let stream = device
        .build_output_stream(
            &config.into(),
            move |data: &mut [f32], _| {
                data_callback::<f32>(data, &audio_mutex, &playback_state);
                tx_out
                    .unbounded_send(message::Message::Worker(
                        message::WorkerMessage::VideoDecoder(
                            message::VideoDecoderMessage::PlaybackStep,
                        ),
                    ))
                    .expect("Error while emitting PlaybackStep");
            },
            move |err| println!("Audio stream error: {}", err),
            None,
        )
        .expect("Failed to build audio stream");

    Some(super::Worker {
        worker_type: super::Type::CpalPlayback,
        handle: stream,
        message_in: tx_in,
    })
}

fn data_callback<T>(
    data: &mut [T],
    audio_mutex: &Arc<Mutex<Option<media::Audio>>>,
    state: &Arc<model::playback::PlaybackState>,
) {
    // Lock the audio mutex, so nothing else tries to access the audio data at the moment.
    let mut audio_lock = audio_mutex.lock().unwrap();

    if let Some(audio) = audio_lock.as_mut() {
        // Lock the position mutex, so nothing tries to change the position
        // between now and when we get the audio.
        let mut auth_pos = state.authoritative_position.lock().unwrap();

        // cpal expects packed audio. The buffer length refers to the
        // number of samples (so frames * channels)
        let num_samples = data.len() as u64;

        // BS' parameters refer to the number of frames, so we
        // need to divide by the number of channels
        let num_frames = num_samples / audio.properties.channels as u64;

        // Get the actual data
        audio.fill_buffer_packed(data, *auth_pos as i64, num_frames as i64);

        println!("read {} frames", num_frames);
        *auth_pos += num_frames;
        state.position.store(*auth_pos, Ordering::Relaxed);
    }
}
