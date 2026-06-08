use crate::{media, message, model};
use std::{
    sync::{Arc, Mutex, atomic},
    thread,
};

use anyhow::Context as _;

#[derive(Debug, Clone)]
pub(super) enum MessageIn {
    TryRestart,
    Play,
    Pause,
}

pub(super) fn spawn(
    tx_out: super::GlobalSender,
    shared_state: &crate::SharedState,
) -> super::Worker<MessageIn> {
    let (tx_in, rx_in) = std::sync::mpsc::channel::<MessageIn>();

    let playing = Arc::new(atomic::AtomicBool::new(false));
    let playback_position = Arc::clone(&shared_state.playback_position);
    let audio_mutex = Arc::clone(&shared_state.audio);

    let handle = thread::Builder::new()
        .name("samaku_cpal_playback".to_owned())
        .spawn(move || {
            use cpal::traits::StreamTrait as _;
            let mut stream_opt: Option<cpal::Stream> = None;

            loop {
                match rx_in.recv() {
                    Ok(message) => match message {
                        MessageIn::TryRestart => {
                            // This drops the existing stream, which is supposedly guaranteed to
                            // close it (https://github.com/RustAudio/cpal/issues/652)
                            stream_opt = None;

                            let audio_properties = {
                                let audio_lock =
                                    audio_mutex.lock().expect("Audio mutex lock poisoned");
                                if let Some(audio) = audio_lock.as_ref() {
                                    audio.properties.clone()
                                } else {
                                    continue;
                                }
                            };

                            match cpal_find_config(&audio_properties) {
                                Ok((device, config)) => {
                                    playback_position.rate.store(
                                        audio_properties.sample_rate,
                                        atomic::Ordering::Relaxed,
                                    );

                                    if let Some(stream) = try_build_stream(
                                        audio_properties.sample_format,
                                        &device,
                                        config,
                                        Arc::clone(&audio_mutex),
                                        Arc::clone(&playing),
                                        Arc::clone(&playback_position),
                                        tx_out.clone(),
                                    ) {
                                        stream_opt = Some(stream);
                                    }
                                }
                                Err(err) => {
                                    tx_out.error(err, "Failed to open audio stream");
                                }
                            }
                        }
                        MessageIn::Play => {
                            if let Some(ref stream) = stream_opt {
                                playing.store(true, atomic::Ordering::Relaxed);
                                tx_out.send(message::Message::UpdatePlaybackStateRepresentation(
                                    true,
                                ));
                                stream.play().expect("Failed to play audio stream");
                            }
                        }
                        MessageIn::Pause => {
                            if let Some(ref stream) = stream_opt {
                                playing.store(false, atomic::Ordering::Relaxed);
                                tx_out.send(message::Message::UpdatePlaybackStateRepresentation(
                                    false,
                                ));
                                stream.pause().expect("Failed to pause audio stream");
                            }
                        }
                    },
                    Err(_) => return,
                }
            }
        })
        .unwrap();

    super::Worker {
        worker_type: super::Type::CpalPlayback,
        _handle: handle,
        message_in: tx_in,
    }
}

fn cpal_find_config(
    audio_properties: &media::AudioProperties,
) -> anyhow::Result<(cpal::Device, cpal::SupportedStreamConfig)> {
    use cpal::traits::DeviceTrait as _;
    use cpal::traits::HostTrait as _;

    // Find the cpal sample format that matches the audio properties
    let sample_format = audio_properties.sample_format;

    let mut config_opt: Option<cpal::SupportedStreamConfig> = None;

    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .context("No audio output device available")?;

    // Try to find a cpal playback config that matches the audio_properties
    for supported_config in device
        .supported_output_configs()
        .context("Error while querying audio output configurations")?
    {
        if audio_properties.channels == supported_config.channels()
            && audio_properties.sample_rate >= supported_config.min_sample_rate()
            && audio_properties.sample_rate <= supported_config.max_sample_rate()
            && sample_format == supported_config.sample_format()
        {
            config_opt = Some(supported_config.with_sample_rate(audio_properties.sample_rate));
            break;
        }
    }

    let config = config_opt.ok_or_else(|| anyhow::anyhow!(
        "Could not find a suitable system audio configuration that matches the loaded audio file",
    ))?;

    Ok((device, config))
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
    let stream_config = config.into();

    match sample_format {
        cpal::SampleFormat::F32 => Some(build_stream::<f32>(
            device,
            stream_config,
            audio_mutex,
            playing,
            playback_position,
            tx_out,
        )),
        cpal::SampleFormat::F64 => Some(build_stream::<f64>(
            device,
            stream_config,
            audio_mutex,
            playing,
            playback_position,
            tx_out,
        )),
        cpal::SampleFormat::U8 => Some(build_stream::<u8>(
            device,
            stream_config,
            audio_mutex,
            playing,
            playback_position,
            tx_out,
        )),
        cpal::SampleFormat::I16 => Some(build_stream::<i16>(
            device,
            stream_config,
            audio_mutex,
            playing,
            playback_position,
            tx_out,
        )),
        cpal::SampleFormat::I32 => Some(build_stream::<i32>(
            device,
            stream_config,
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
    config: cpal::StreamConfig,
    audio_mutex: Arc<Mutex<Option<media::Audio>>>,
    playing: Arc<atomic::AtomicBool>,
    playback_position: Arc<model::playback::Position>,
    tx_out: super::GlobalSender,
) -> cpal::Stream
where
    T: cpal::SizedSample + Default,
{
    use cpal::traits::DeviceTrait as _;

    let tx_out_err = tx_out.clone();

    device
        .build_output_stream(
            config,
            move |data: &mut [T], _| {
                data_callback::<T>(data, &audio_mutex, &playing, &playback_position, &tx_out);
            },
            move |err| {
                tx_out_err.send(message::Message::Toast(model::toast::Toast::error_title(
                    "Audio stream error",
                    &err.into(),
                )));
            },
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
    T: cpal::SizedSample + Default,
{
    // Lock the audio mutex, so nothing else tries to access the audio data at the moment.
    let mut audio_lock = audio_mutex.lock().unwrap();

    // If playback is paused, zero the array and return
    if !playing.load(atomic::Ordering::Relaxed) {
        zero(data);
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
        if let Err(err) = audio.fill_buffer_packed(data, *auth_pos, num_frames) {
            // If an error occurred while getting audio data,
            // show it as a toast and pause playback.
            tx_out.send(message::Message::Toast(model::toast::Toast::error_title(
                "Audio playback error",
                &err,
            )));
            tx_out.send(message::Message::SetPlayback(false));
            zero(data);
            return;
        }

        *auth_pos += num_frames;
        playback_position
            .position
            .store(*auth_pos, atomic::Ordering::Relaxed);

        drop(auth_pos);

        tx_out.send(message::Message::PlaybackStep);
    }
}

fn zero<T: Default>(data: &mut [T]) {
    for i in &mut *data {
        *i = Default::default();
    }
}
