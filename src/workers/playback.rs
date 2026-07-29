use crate::{media, message, model};
use anyhow::Context as _;
use std::{
    sync::{Arc, Mutex, atomic, mpsc},
    thread,
};

#[derive(Debug, Clone)]
pub(super) enum MessageIn {
    TryRestart,
    Play,
    Pause,
}

const DIRECT_PLAY_INTERVAL: std::time::Duration = std::time::Duration::from_millis(10);

#[expect(
    clippy::too_many_lines,
    reason = "uncoupling all this code is kind of difficult and not so high priority"
)] // TODO uncouple
pub(super) fn spawn(
    tx_out: super::GlobalSender,
    shared_state: &crate::SharedState,
) -> super::Worker<MessageIn> {
    let (tx_in, rx_in) = mpsc::channel::<MessageIn>();

    let playing = Arc::new(atomic::AtomicBool::new(false));
    let playback_position = Arc::clone(&shared_state.playback_position);
    let audio_mutex = Arc::clone(&shared_state.audio);

    let handle = thread::Builder::new()
        .name("samaku_playback".to_owned())
        .spawn(move || {
            use cpal::traits::StreamTrait as _;
            let mut stream_opt: Option<cpal::Stream> = None;

            loop {
                let message = if stream_opt.is_none() && playing.load(atomic::Ordering::Relaxed) {
                    // If we don't have a stream but playing is set, we ourselves should be responsible for playback timing.
                    // But before that, we need to check if there is an event we need to handle first.
                    let mut start_time = std::time::Instant::now();
                    let mut start_position = playback_position.snapshot();
                    'playback: loop {
                        let last_position = playback_position.snapshot();
                        match rx_in.recv_timeout(DIRECT_PLAY_INTERVAL) {
                            Ok(message) => break 'playback message,
                            Err(mpsc::RecvTimeoutError::Timeout) => {
                                // There's no event, so advance playback timing.
                                let millis_since_start =
                                    i64::try_from(start_time.elapsed().as_millis())
                                        .expect("playback time overflow");
                                let current_position = start_position.millis + millis_since_start;

                                let advance_result = playback_position
                                    .advance_to(current_position, last_position.generation);
                                match advance_result {
                                    model::playback::Advance::Applied => {
                                        // nothing to do
                                    }
                                    model::playback::Advance::Discarded => {
                                        start_time = std::time::Instant::now();
                                        start_position = playback_position.snapshot();
                                    }
                                    model::playback::Advance::ReachedEnd => {
                                        tx_out.send(message::Message::SetPlayback(false));
                                    }
                                }
                                tx_out.send(message::Message::PlaybackStep);
                            }
                            Err(mpsc::RecvTimeoutError::Disconnected) => {
                                panic!("playback worker channel closed")
                            }
                        }
                    }
                } else {
                    match rx_in.recv() {
                        Ok(message) => message,
                        Err(mpsc::RecvError) => panic!("playback worker channel closed"),
                    }
                };

                match message {
                    MessageIn::TryRestart => {
                        // This drops the existing stream, which is supposedly guaranteed to
                        // close it (https://github.com/RustAudio/cpal/issues/652)
                        stream_opt = None;

                        let audio_properties = {
                            let audio_lock = audio_mutex.lock().expect("Audio mutex lock poisoned");
                            if let Some(audio) = audio_lock.as_ref() {
                                audio.properties.clone()
                            } else {
                                continue;
                            }
                        };

                        match cpal_find_config(&audio_properties) {
                            Ok((device, config)) => {
                                if let Some(stream) = try_build_stream(
                                    audio_properties.sample_format,
                                    &device,
                                    config,
                                    Arc::clone(&audio_mutex),
                                    Arc::clone(&playing),
                                    Arc::clone(&playback_position),
                                    tx_out.clone(),
                                ) {
                                    if playing.load(atomic::Ordering::Relaxed) {
                                        stream.play().expect("Failed to play audio stream");
                                    }
                                    stream_opt = Some(stream);
                                }
                            }
                            Err(err) => {
                                tx_out.error(err, "Failed to open audio stream");
                            }
                        }
                    }
                    MessageIn::Play => {
                        playing.store(true, atomic::Ordering::Relaxed);
                        tx_out.send(message::Message::UpdatePlaybackStateRepresentation(true));

                        if let Some(ref stream) = stream_opt {
                            stream.play().expect("Failed to play audio stream");
                        }
                    }
                    MessageIn::Pause => {
                        playing.store(false, atomic::Ordering::Relaxed);
                        tx_out.send(message::Message::UpdatePlaybackStateRepresentation(false));

                        if let Some(ref stream) = stream_opt {
                            stream.pause().expect("Failed to pause audio stream");
                        }
                    }
                }
            }
        })
        .unwrap();

    super::Worker {
        worker_type: super::Type::Playback,
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

/// This stream's own cursor into the audio data, at sample resolution.
///
/// The playback position is only accurate to a millisecond, which is not enough to prevent
/// accumulating rounding errors over time. So we keep track of the exact sample we
/// are at ourselves, and only re-synchronize from the playback position when something else
/// changed that (e.g. seeking).
struct Cursor {
    /// Index of the next sample frame to be played.
    sample: u64,

    /// The generation of the playback position this cursor was last synchronised against, or
    /// `None` if it has not been synchronised at all yet.
    generation: Option<u64>,
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

    let mut cursor = Cursor {
        sample: 0,
        generation: None,
    };

    device
        .build_output_stream(
            config,
            move |data: &mut [T], _| {
                data_callback::<T>(
                    data,
                    &mut cursor,
                    &audio_mutex,
                    &playing,
                    &playback_position,
                    &tx_out,
                );
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
    cursor: &mut Cursor,
    audio_mutex: &Mutex<Option<media::Audio>>,
    playing: &atomic::AtomicBool,
    playback_position: &model::playback::Position,
    tx_out: &super::GlobalSender,
) where
    T: cpal::SizedSample + Default,
{
    // If playback is paused, zero the array and return.
    // This needs to be done before taking any locks, so a paused stream cannot be held up by anything.
    if !playing.load(atomic::Ordering::Relaxed) {
        zero(data);
        return;
    }

    // Lock the audio mutex, so nothing else tries to access the audio data at the moment.
    let mut audio_lock = audio_mutex.lock().unwrap();
    let Some(audio) = audio_lock.as_mut() else {
        zero(data);
        return;
    };

    let sample_rate = audio.properties.sample_rate;
    let channels = audio.properties.channels;

    // Check whether the position was changed outside of our control
    // (e.g. by seeking), and if so, update our own cursor.
    let snapshot = playback_position.snapshot();
    if cursor.generation != Some(snapshot.generation) {
        cursor.generation = Some(snapshot.generation);
        cursor.sample = millis_to_samples(snapshot.millis, sample_rate);
    }

    // cpal expects packed audio. The buffer length refers to the
    // number of samples (so frames * channels)
    let num_samples = data.len() as u64;

    // ffms2's parameters refer to the number of frames, so we
    // need to divide by the number of channels
    let num_frames = num_samples / u64::from(channels);

    // Get the actual data. Note that we deliberately do not hold the playback position lock while
    // doing this. If a seek occurs in the meantime, we will notice below in `advance_to`.
    let fill_result = audio.fill_buffer_packed(data, cursor.sample, num_frames);
    drop(audio_lock);

    if let Err(err) = fill_result {
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

    cursor.sample += num_frames;

    let advance = playback_position.advance_to(
        samples_to_millis(cursor.sample, sample_rate),
        snapshot.generation,
    );

    if matches!(advance, model::playback::Advance::ReachedEnd) {
        // We ran into the end of the playable range, so stop here.
        // The next callback will re-synchronize our cursor against the clamped position.
        tx_out.send(message::Message::SetPlayback(false));
    }

    tx_out.send(message::Message::PlaybackStep);
}

/// Converts a number of milliseconds into the number of sample frames it corresponds to at the
/// given sample rate.
fn millis_to_samples(millis: i64, sample_rate: u32) -> u64 {
    let non_negative: u64 = millis.max(0).try_into().unwrap_or(0);
    non_negative * u64::from(sample_rate) / 1000
}

/// Converts a number of sample frames at the given sample rate into the number of milliseconds
/// they correspond to, rounding down.
fn samples_to_millis(samples: u64, sample_rate: u32) -> i64 {
    if sample_rate == 0 {
        return 0;
    }

    (samples * 1000 / u64::from(sample_rate))
        .try_into()
        .unwrap_or(i64::MAX)
}

fn zero<T: Default>(data: &mut [T]) {
    for i in &mut *data {
        *i = Default::default();
    }
}
