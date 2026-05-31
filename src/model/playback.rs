use std::sync::{
    Mutex,
    atomic::{AtomicU32, AtomicU64, Ordering},
};

use crate::{media, model, subtitle};

/// Atomically interior-mutable playback position.
///
/// Represents the playback position in a thread-safe way while also allowing interior mutability with only an immutable reference.
/// The position is represented as a number of ticks with a variable base rate (specified as the `rate` field).
pub struct Position {
    /// Position in terms of `rate`. Always guaranteed to be correct,
    /// but requires a lock on the mutex to access.
    pub authoritative_position: Mutex<u64>,

    // TODO fix potential race condition here from position and rate being observed independently
    /// Last known position in terns of `rate`.
    pub position: AtomicU64,

    /// How many `n`'s per second there are.
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

    /// Returns the current non-authoritative position as milliseconds for subtitle purposes.
    ///
    /// # Panics
    /// Panics on overflow.
    pub fn subtitle_time(&self) -> subtitle::StartTime {
        subtitle::StartTime(
            self.position()
                .checked_mul(1000)
                .and_then(|result| <u64 as TryInto<i64>>::try_into(result).ok())
                .expect("playback position overflow")
                / i64::from(self.rate()),
        )
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

    /// Sets the playback position to the given value in ticks.
    ///
    /// # Panics
    /// Panics if the authoritative position lock is poisoned.
    pub fn set_ticks(&self, new_value: u64) {
        let mut lock = self.authoritative_position.lock().unwrap();
        *lock = new_value;
        drop(lock);
        self.position.store(new_value, Ordering::Relaxed);
    }

    /// Sets the playback position to the given event start time.
    ///
    /// # Panics
    /// Panics if the authoritative position lock is poisoned, or on overflow.
    pub fn set_to_event(&self, new_value: subtitle::StartTime) {
        if self.rate() == 0 {
            return;
        }

        let ticks: i64 = new_value
            .0
            .checked_mul(i64::from(self.rate()))
            .expect("ticks overflow")
            / 1000;
        self.set_ticks(ticks.try_into().unwrap_or(0));
    }

    /// Sets the playback position to the given frame.
    ///
    /// # Panics
    /// Panics if the authoritative position lock is poisoned, or on overflow.
    pub fn set_to_frame(&self, new_value: model::FrameNumber, frame_rate: media::FrameRate) {
        if self.rate() == 0 {
            return;
        }

        // We need to always round up, to cover the event starting on this frame.
        #[expect(
            clippy::cast_possible_truncation,
            reason = "outside the expected temporal bounds"
        )]
        let ms = (1000.0 * f64::from(new_value.0) / f64::from(frame_rate)).ceil() as i64;

        let ticks: i64 = ms
            .checked_mul(i64::from(self.rate()))
            .expect("ticks overflow")
            / 1000;
        self.set_ticks(ticks.try_into().unwrap_or(0));
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

#[cfg(test)]
mod tests {
    use super::*;

    fn make_position(position: u64, rate: u32) -> Position {
        Position {
            authoritative_position: Mutex::new(position),
            position: AtomicU64::new(position),
            rate: AtomicU32::new(rate),
        }
    }

    #[test]
    fn seconds_zero_rate() {
        // should equal zero
        assert!(make_position(1000, 0).seconds().abs() < 0.0001);
    }

    #[test]
    fn seconds_normal() {
        assert!((make_position(0, 1000).seconds()).abs() < 0.0001); // should equal zero
        assert!((make_position(1000, 1000).seconds() - 1.0).abs() < 0.0001);
        assert!((make_position(500, 1000).seconds() - 0.5).abs() < 0.0001);
        assert!((make_position(3000, 1000).seconds() - 3.0).abs() < 0.0001);
    }

    #[test]
    fn subtitle_time_basic() {
        assert_eq!(
            make_position(0, 1000).subtitle_time(),
            subtitle::StartTime(0)
        );
        assert_eq!(
            make_position(1000, 1000).subtitle_time(),
            subtitle::StartTime(1000)
        );
        assert_eq!(
            make_position(2500, 1000).subtitle_time(),
            subtitle::StartTime(2500)
        );
    }

    #[test]
    fn current_frame_24fps() {
        let frame_rate = media::FrameRate {
            numerator: 24,
            denominator: 1,
        };
        // 1 second → frame 24
        assert_eq!(
            make_position(1000, 1000).current_frame(frame_rate),
            model::FrameNumber(24)
        );
        // 0 ticks → frame 0
        assert_eq!(
            make_position(0, 1000).current_frame(frame_rate),
            model::FrameNumber(0)
        );
        // 500 ms → frame 12
        assert_eq!(
            make_position(500, 1000).current_frame(frame_rate),
            model::FrameNumber(12)
        );
        // Rounds down: one tick less than a full frame
        assert_eq!(
            make_position(41, 1000).current_frame(frame_rate),
            model::FrameNumber(0)
        );
        assert_eq!(
            make_position(42, 1000).current_frame(frame_rate),
            model::FrameNumber(1)
        );
    }

    #[test]
    fn add_ticks_basic() {
        let pos = make_position(0, 1000);
        pos.add_ticks(100);
        assert_eq!(pos.position(), 100);
        pos.add_ticks(50);
        assert_eq!(pos.position(), 150);
        pos.add_ticks(-100);
        assert_eq!(pos.position(), 50);
    }

    #[test]
    fn add_ticks_saturates_at_zero() {
        let pos = make_position(10, 1000);
        pos.add_ticks(-100);
        assert_eq!(pos.position(), 0);
    }

    #[test]
    fn set_to_event_basic() {
        let pos = make_position(0, 1000);
        pos.set_to_event(subtitle::StartTime(2000));
        assert_eq!(pos.position(), 2000);
        pos.set_to_event(subtitle::StartTime(0));
        assert_eq!(pos.position(), 0);
    }

    #[test]
    fn set_to_event_zero_rate() {
        let pos = make_position(500, 0);
        pos.set_to_event(subtitle::StartTime(2000));
        // Should not change position when rate is 0
        assert_eq!(pos.position(), 500);
    }

    #[test]
    fn set_to_event_negative_clamped_to_zero() {
        let pos = make_position(1000, 1000);
        pos.set_to_event(subtitle::StartTime(-5000));
        assert_eq!(pos.position(), 0);
    }

    #[test]
    fn set_to_frame() {
        let pos = make_position(0, 1000);
        let frame_rate = media::FrameRate {
            numerator: 24,
            denominator: 1,
        };

        pos.set_to_frame(model::FrameNumber(24), frame_rate);
        assert_eq!(pos.position(), 1000);

        let pos = make_position(0, 1000);
        let frame_rate = media::FrameRate {
            numerator: 24000,
            denominator: 1001,
        };

        pos.set_to_frame(model::FrameNumber(13), frame_rate);
        assert_eq!(pos.position(), 543);
    }
}
