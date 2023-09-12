use std::sync::{
    atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
    Mutex,
};

use crate::media;

pub struct PlaybackState {
    // Position in terms of `rate`. Always guaranteed to be correct,
    // but requires a lock on the mutex to access.
    pub authoritative_position: Mutex<u64>,

    // Last known position in terns of `rate`.
    pub position: AtomicU64,

    // How many `n`'s per second there are.
    pub rate: AtomicU32,

    pub playing: AtomicBool,
}

impl PlaybackState {
    pub fn position(&self) -> u64 {
        self.position.load(Ordering::Relaxed)
    }

    pub fn rate(&self) -> u32 {
        self.rate.load(Ordering::Relaxed)
    }

    pub fn seconds(&self) -> f64 {
        if self.rate() == 0 {
            return 0.0;
        }
        self.position() as f64 / self.rate() as f64
    }

    pub fn current_frame(&self, frame_rate: media::FrameRate) -> i32 {
        (self.seconds() * f64::from(frame_rate)).floor() as i32
    }

    // These do not require a mutable reference as the struct
    // ensures unique mutability by itself
    pub fn add_ticks(&self, delta: i64) {
        let mut lock = self.authoritative_position.lock().unwrap();
        let new_value = lock.saturating_add_signed(delta);
        *lock = new_value;
        self.position.store(new_value, Ordering::Relaxed);
    }

    pub fn add_seconds(&self, delta_seconds: f64) {
        if self.rate() == 0 {
            return;
        }
        let delta_ticks: i64 = (delta_seconds * self.rate() as f64).round() as i64;
        self.add_ticks(delta_ticks);
    }

    pub fn add_frames(&self, delta_frames: i32, frame_rate: media::FrameRate) {
        self.add_seconds(delta_frames as f64 / f64::from(frame_rate));
    }
}

impl Default for PlaybackState {
    fn default() -> Self {
        Self {
            authoritative_position: Mutex::new(0),
            position: 0.into(),
            rate: 1000.into(), // if nothing is loaded, use milliseconds for position
            playing: false.into(),
        }
    }
}
