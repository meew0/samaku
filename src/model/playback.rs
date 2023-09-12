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
}

impl Default for PlaybackState {
    fn default() -> Self {
        Self {
            authoritative_position: Mutex::new(0),
            position: 0.into(),
            rate: 0.into(),
            playing: false.into(),
        }
    }
}
