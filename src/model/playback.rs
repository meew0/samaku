use std::sync::{
    atomic::{AtomicU32, AtomicU64, Ordering},
    Mutex,
};

use crate::{media, model};

pub struct Position {
    // Position in terms of `rate`. Always guaranteed to be correct,
    // but requires a lock on the mutex to access.
    pub authoritative_position: Mutex<u64>,

    // Last known position in terns of `rate`.
    pub position: AtomicU64,

    // How many `n`'s per second there are.
    pub rate: AtomicU32,
}

impl Position {
    pub fn position(&self) -> u64 {
        self.position.load(Ordering::Relaxed)
    }

    pub fn rate(&self) -> u32 {
        self.rate.load(Ordering::Relaxed)
    }

    /// Returns the current non-authoritative position as floating-point seconds.
    /// May be imprecise for very large positions or rates.
    #[expect(
        clippy::cast_precision_loss,
        reason = "acceptable amount of precision loss"
    )]
    pub fn seconds(&self) -> f64 {
        if self.rate() == 0 {
            return 0.0;
        }
        self.position() as f64 / f64::from(self.rate())
    }

    /// Converts the playback position into a frame number (rounding down) using the given frame
    /// rate. Avoids floating point imprecisions where possible.
    ///
    /// # Panics
    /// Panics if the frame number does not fit into a 32-bit signed integer.
    pub fn current_frame(&self, frame_rate: media::FrameRate) -> model::FrameNumber {
        let numerator = self.position() * frame_rate.numerator;
        let denominator = frame_rate.denominator * u64::from(self.rate());

        model::FrameNumber(
            (numerator / denominator)
                .try_into()
                .expect("frame number overflow"),
        )
    }

    /// Adds the given `delta` number of ticks to the playback state. May be negative.
    ///
    /// # Panics
    /// Panics if the authoritative position lock is poisoned.
    pub fn add_ticks(&self, delta: i64) {
        let mut lock = self.authoritative_position.lock().unwrap();
        let new_value = lock.saturating_add_signed(delta);
        *lock = new_value;
        drop(lock);
        self.position.store(new_value, Ordering::Relaxed);
    }

    pub fn add_seconds(&self, delta_seconds: f64) {
        if self.rate() == 0 {
            return;
        }
        #[expect(
            clippy::cast_possible_truncation,
            reason = "very unlikely to occur in practice"
        )]
        let delta_ticks: i64 = (delta_seconds * f64::from(self.rate())).round() as i64;
        self.add_ticks(delta_ticks);
    }

    pub fn add_frames(&self, delta_frames: model::FrameDelta, frame_rate: media::FrameRate) {
        self.add_seconds(f64::from(delta_frames.0) / f64::from(frame_rate));
    }
}

impl Default for Position {
    fn default() -> Self {
        Self {
            authoritative_position: Mutex::new(0),
            position: 0.into(),
            rate: 1000.into(), // if nothing is loaded, use milliseconds for position
        }
    }
}
