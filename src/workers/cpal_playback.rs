use std::{
    mem::size_of,
    sync::{atomic, Arc, Mutex},
    thread,
};

use crate::{media, message, model};

#[derive(Debug, Clone)]
pub enum MessageIn {
    TryRestart,
    Play,
    Pause,
}

pub fn spawn(
    tx_out: super::GlobalSender,
    shared_state: &crate::SharedState,
) -> super::Worker<MessageIn> {
    use cpal::traits::{DeviceTrait, HostTrait};

    let (tx_in, rx_in) = std::sync::mpsc::channel::<MessageIn>();

    let playing = Arc::new(atomic::AtomicBool::new(false));
    let playback_position = Arc::clone(&shared_state.playback_position);
    let audio_mutex = Arc::clone(&shared_state.audio);

    let handle = thread::Builder::new().name("samaku_cpal_playback".to_owned()).spawn(move || {
        use cpal::traits::StreamTrait;
        let mut stream_opt: Option<cpal::Stream> = None;

        loop {
            match rx_in.recv() {
                Ok(message) => match message {
                    MessageIn::TryRestart => {
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
                        let sample_format = sample_format_for_audio_properties(&audio_properties);

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
                            if audio_properties.channels == u32::from(supported_config.channels())
                                && audio_properties.sample_rate
                                >= supported_config.min_sample_rate().0
                                && audio_properties.sample_rate
                                <= supported_config.max_sample_rate().0
                                && sample_format == supported_config.sample_format()
                            {
                                config_opt =
                                    Some(supported_config.with_sample_rate(cpal::SampleRate(
                                        audio_properties.sample_rate,
                                    )));
                            }
                        }

                        let config = config_opt.expect(
                            "Could not find a suitable system audio configuration that matches the loaded audio file",
                        );

                        playback_position
                            .rate
                            .store(audio_properties.sample_rate, atomic::Ordering::Relaxed);

                        if let Some(stream) = try_build_stream(sample_format, &device, config, Arc::clone(&audio_mutex), Arc::clone(&playing), Arc::clone(&playback_position), tx_out.clone()) {
                            stream_opt = Some(stream);
                        }
                    }
                    MessageIn::Play => {
                        if let Some(ref stream) = stream_opt {
                            playing.store(true, atomic::Ordering::Relaxed);
                            tx_out.unbounded_send(message::Message::Playing(true)).expect("Failed to send playing message");
                            stream.play().expect("Failed to play audio stream");
                        }
                    }
                    MessageIn::Pause => {
                        if let Some(ref stream) = stream_opt {
                            playing.store(false, atomic::Ordering::Relaxed);
                            tx_out.unbounded_send(message::Message::Playing(false)).expect("Failed to send pausing message");
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

fn sample_format_for_audio_properties(
    audio_properties: &media::AudioProperties,
) -> cpal::SampleFormat {
    let sample_format_opt: Option<cpal::SampleFormat> = if audio_properties.format.float {
        const F32_SIZE: usize = size_of::<f32>();
        const F64_SIZE: usize = size_of::<f64>();
        match audio_properties.format.bytes_per_sample {
            F32_SIZE => Some(cpal::SampleFormat::F32),
            F64_SIZE => Some(cpal::SampleFormat::F64),
            _ => None,
        }
    } else {
        const U8_SIZE: usize = size_of::<u8>();
        const I16_SIZE: usize = size_of::<i16>();
        const I32_SIZE: usize = size_of::<i32>();
        match audio_properties.format.bytes_per_sample {
            U8_SIZE => Some(cpal::SampleFormat::U8),
            I16_SIZE => Some(cpal::SampleFormat::I16),
            I32_SIZE => Some(cpal::SampleFormat::I32),
            _ => None,
        }
    };

    sample_format_opt.expect("Audio sample format not representable by cpal")
}

fn try_build_stream(
    sample_format: cpal::SampleFormat,
    device: &cpal::Device,
    config: cpal::SupportedStreamConfig,
    audio_mutex: Arc<Mutex<Option<media::Audio>>>,
    playing: Arc<atomic::AtomicBool>,
    playback_position: Arc<model::playback::Position>,
    tx_out: super::GlobalSender,
) -> Option<cpal::Stream> {
    match sample_format {
        cpal::SampleFormat::F32 => Some(build_stream::<f32>(
            device,
            &config.into(),
            audio_mutex,
            playing,
            playback_position,
            tx_out,
        )),
        cpal::SampleFormat::F64 => Some(build_stream::<f64>(
            device,
            &config.into(),
            audio_mutex,
            playing,
            playback_position,
            tx_out,
        )),
        cpal::SampleFormat::U8 => Some(build_stream::<u8>(
            device,
            &config.into(),
            audio_mutex,
            playing,
            playback_position,
            tx_out,
        )),
        cpal::SampleFormat::I16 => Some(build_stream::<i16>(
            device,
            &config.into(),
            audio_mutex,
            playing,
            playback_position,
            tx_out,
        )),
        cpal::SampleFormat::I32 => Some(build_stream::<i32>(
            device,
            &config.into(),
            audio_mutex,
            playing,
            playback_position,
            tx_out,
        )),
        other => {
            println!("Unsupported sample format for playback: {other}");
            None
        }
    }
}

fn build_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    audio_mutex: Arc<Mutex<Option<media::Audio>>>,
    playing: Arc<atomic::AtomicBool>,
    playback_position: Arc<model::playback::Position>,
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
                data_callback::<T>(data, &audio_mutex, &playing, &playback_position, &tx_out);
            },
            move |err| println!("Audio stream error: {err}"),
            None,
        )
        .expect("Failed to build audio stream")
}

fn data_callback<T>(
    data: &mut [T],
    audio_mutex: &Arc<Mutex<Option<media::Audio>>>,
    playing: &Arc<atomic::AtomicBool>,
    playback_position: &Arc<model::playback::Position>,
    tx_out: &super::GlobalSender,
) where
    T: Default,
{
    // Lock the audio mutex, so nothing else tries to access the audio data at the moment.
    let mut audio_lock = audio_mutex.lock().unwrap();

    // If playback is paused, zero the array and return
    if !playing.load(atomic::Ordering::Relaxed) {
        for i in &mut *data {
            *i = Default::default();
        }
        return;
    }

    if let Some(audio) = audio_lock.as_mut() {
        // Lock the position mutex, so nothing tries to change the position
        // between now and when we get the audio.
        let mut auth_pos = playback_position.authoritative_position.lock().unwrap();

        // cpal expects packed audio. The buffer length refers to the
        // number of samples (so frames * channels)
        let num_samples = data.len() as u64;

        // BS' parameters refer to the number of frames, so we
        // need to divide by the number of channels
        let num_frames = num_samples / u64::from(audio.properties.channels);

        // Get the actual data
        audio.fill_buffer_packed(data, *auth_pos, num_frames);

        *auth_pos += num_frames;
        playback_position
            .position
            .store(*auth_pos, atomic::Ordering::Relaxed);

        drop(auth_pos);

        tx_out
            .unbounded_send(message::Message::PlaybackStep)
            .expect("Error while emitting PlaybackStep");
    }
}
