use std::{
    mem::size_of,
    sync::{atomic::Ordering, Arc, Mutex},
    thread,
};

use crate::{media, message, model};

#[derive(Debug, Clone)]
pub enum Message {
    TryRestart,

    // These are NOT for playing and pausing video/audio playback
    // at the application level (use global_state.playback_state.playing),
    // but for the stream level
    Play,
    Pause,
}

pub fn spawn(
    tx_out: super::GlobalSender,
    shared_state: &crate::SharedState,
) -> super::Worker<self::Message> {
    use cpal::traits::{DeviceTrait, HostTrait};

    let (tx_in, rx_in) = std::sync::mpsc::channel::<self::Message>();

    let playback_state = Arc::clone(&shared_state.playback_state);
    let audio_mutex = Arc::clone(&shared_state.audio);

    let handle = thread::Builder::new().name("samaku_cpal_playback".to_string()).spawn(move || {
        use cpal::traits::StreamTrait;
        let mut stream_opt: Option<cpal::Stream> = None;

        loop {
            match rx_in.recv() {
                Ok(message) => match message {
                    self::Message::TryRestart => {
                        // This drops the existing stream, which is supposedly guaranteed to
                        // close it (https://github.com/RustAudio/cpal/issues/652)
                        stream_opt = None;

                        let audio_properties = {
                            let audio_lock = audio_mutex.lock().unwrap();
                            if let Some(audio) = audio_lock.as_ref() {
                                audio.properties
                            } else {
                                continue;
                            }
                        };

                        // Find the cpal sample format that matches the audio properties
                        let sample_format_opt: Option<cpal::SampleFormat> =
                            if audio_properties.is_float {
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

                        let sample_format = sample_format_opt
                            .expect("Audio sample format not representable by cpal");

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
                                && audio_properties.sample_rate
                                >= supported_config.min_sample_rate().0 as i32
                                && audio_properties.sample_rate
                                <= supported_config.max_sample_rate().0 as i32
                                && sample_format == supported_config.sample_format()
                            {
                                config_opt =
                                    Some(supported_config.with_sample_rate(cpal::SampleRate(
                                        audio_properties.sample_rate.try_into().unwrap(),
                                    )))
                            }
                        }

                        let config = config_opt.expect(
                            "Could not find a suitable system audio configuration that matches the loaded audio file",
                        );

                        playback_state
                            .rate
                            .store(audio_properties.sample_rate as u32, Ordering::Relaxed);

                        let audio_mutex2 = Arc::clone(&audio_mutex);
                        let playback_state2 = Arc::clone(&playback_state);
                        let tx_out2 = tx_out.clone();

                        if let Some(stream) = match sample_format {
                            cpal::SampleFormat::F32 => Some(build_stream::<f32>(
                                &device,
                                &config.into(),
                                audio_mutex2,
                                playback_state2,
                                tx_out2,
                            )),
                            cpal::SampleFormat::F64 => Some(build_stream::<f64>(
                                &device,
                                &config.into(),
                                audio_mutex2,
                                playback_state2,
                                tx_out2,
                            )),
                            cpal::SampleFormat::U8 => Some(build_stream::<u8>(
                                &device,
                                &config.into(),
                                audio_mutex2,
                                playback_state2,
                                tx_out2,
                            )),
                            cpal::SampleFormat::I16 => Some(build_stream::<i16>(
                                &device,
                                &config.into(),
                                audio_mutex2,
                                playback_state2,
                                tx_out2,
                            )),
                            cpal::SampleFormat::I32 => Some(build_stream::<i32>(
                                &device,
                                &config.into(),
                                audio_mutex2,
                                playback_state2,
                                tx_out2,
                            )),
                            other => {
                                println!("Unsupported sample format for playback: {}", other);
                                None
                            }
                        } {
                            stream.play().expect("Failed to play audio stream");
                            stream_opt = Some(stream);
                        }
                    }
                    self::Message::Play => {
                        if let Some(ref stream) = stream_opt {
                            stream.play().expect("Failed to play audio stream");
                        }
                    }
                    self::Message::Pause => {
                        if let Some(ref stream) = stream_opt {
                            stream.pause().expect("Failed to pause audio stream");
                        }
                    }
                },
                Err(_) => return,
            }
        }
    }).unwrap();

    super::Worker {
        worker_type: super::Type::CpalPlayback,
        _handle: handle,
        message_in: tx_in,
    }
}

fn build_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    audio_mutex: Arc<Mutex<Option<media::Audio>>>,
    playback_state: Arc<model::playback::PlaybackState>,
    tx_out: super::GlobalSender,
) -> cpal::Stream
where
    T: cpal::SizedSample + Default,
{
    use cpal::traits::DeviceTrait;

    device
        .build_output_stream(
            config,
            move |data: &mut [T], _| {
                data_callback::<T>(data, &audio_mutex, &playback_state, &tx_out);
            },
            move |err| println!("Audio stream error: {}", err),
            None,
        )
        .expect("Failed to build audio stream")
}

fn data_callback<T>(
    data: &mut [T],
    audio_mutex: &Arc<Mutex<Option<media::Audio>>>,
    playback_state: &Arc<model::playback::PlaybackState>,
    tx_out: &super::GlobalSender,
) where
    T: Default,
{
    // Lock the audio mutex, so nothing else tries to access the audio data at the moment.
    let mut audio_lock = audio_mutex.lock().unwrap();

    // If playback is paused, zero the array and return
    if !playback_state.playing.load(Ordering::Relaxed) {
        for i in data.iter_mut() {
            *i = Default::default();
        }
        return;
    }

    if let Some(audio) = audio_lock.as_mut() {
        // Lock the position mutex, so nothing tries to change the position
        // between now and when we get the audio.
        let mut auth_pos = playback_state.authoritative_position.lock().unwrap();

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
        playback_state.position.store(*auth_pos, Ordering::Relaxed);

        tx_out
            .unbounded_send(message::Message::PlaybackStep)
            .expect("Error while emitting PlaybackStep");
    }
}
