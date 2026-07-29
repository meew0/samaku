use std::sync::{
    Mutex,
    atomic::{AtomicI64, Ordering},
};

use crate::{media, subtitle};

/// Interiorly mutable, thread-safe playback position.
///
/// The position is stored in as a signed integer representing milliseconds.
/// Playback engines will update the authoritative state based on their own position
/// at the "native" resolution, and resynchronize their own positions when the
/// generation counter changes.
///
/// For the purposes of UI code, the position can always be read lock-free from the atomic `cached` value.
/// Locking the mutex is only required for writing to it, and for certain engine-facing
/// methods that need to read the generation counter as well.
pub struct Position {
    /// The authoritative state. Always guaranteed to be correct, but requires a lock to access.
    state: Mutex<State>,

    /// Cached value that can be read without a lock on the mutex. It may be very slightly out of date,
    /// but this inaccuracy should not matter for most code.
    cached_millis: AtomicI64,
}

struct State {
    /// Playback position, in milliseconds.
    millis: i64,

    /// Lower end of the range the position is clamped to, inclusive. (This will almost always be 0.)
    min_millis: i64,

    /// Upper end of the range the position is clamped to, inclusive;
    /// i.e. the end of the video or audio stream.
    max_millis: i64,

    /// Counter for changes to the position that occur outside of the playback engine (e.g. seeking).
    ///
    /// If the playback engine notices that this value has changed,
    /// it will know that it needs to re-synchronize its internal position.
    generation: u64,
}

/// The playback position together with the generation it belongs to,
/// for a playback engine to synchronize its own position against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Snapshot {
    pub millis: i64,
    pub generation: u64,
}

/// The outcome of [`Position::advance_to`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Advance {
    /// The advance was successfully applied to the playback position.
    Applied,

    /// The position was changed in the meantime, so the advance could not be applied
    /// and the engine needs to re-synchronize before its next block.
    Discarded,

    /// The advance ran into the end of the clamp range (i.e. the end of the audio or video),
    /// so playback should stop here. The engine also needs to re-synchronize before its next block.
    ReachedEnd,
}

impl Position {
    /// Returns the current playback position in milliseconds.
    pub fn millis(&self) -> i64 {
        self.cached_millis.load(Ordering::Relaxed)
    }

    /// Returns the current playback position as a time for subtitle purposes.
    pub fn subtitle_time(&self) -> subtitle::StartTime {
        subtitle::StartTime(self.millis())
    }

    /// Returns the current playback position as floating-point seconds.
    #[expect(
        clippy::cast_precision_loss,
        reason = "acceptable amount of precision loss"
    )]
    pub fn seconds(&self) -> f64 {
        self.millis() as f64 / 1000.0
    }

    /// Converts the playback position into a frame number (rounding down) using the given frame
    /// rate. Avoids floating point imprecisions where possible.
    pub fn current_frame(&self, frame_rate: &media::FrameRate) -> media::FrameNumber {
        frame_rate.frame_at_time(self.subtitle_time(), media::TimeMode::Exact)
    }

    /// Writes `new_millis` into the given locked state, clamped to the state's range, and
    /// publishes it to the lock-free snapshot.
    ///
    /// Returns whether the value had to be clamped down to the upper end of the range.
    fn store_locked(&self, state: &mut State, new_millis: i64) -> bool {
        let clamped = new_millis.clamp(state.min_millis, state.max_millis);
        state.millis = clamped;
        self.cached_millis.store(clamped, Ordering::Relaxed);
        clamped < new_millis
    }

    /// Sets the playback position to the given number of milliseconds, clamped to the current
    /// range, and marks the position as seeked.
    ///
    /// # Panics
    /// Panics if the state lock is poisoned.
    pub fn set_millis(&self, new_millis: i64) {
        let mut lock = self.state.lock().unwrap();
        self.store_locked(&mut lock, new_millis);
        lock.generation = lock.generation.wrapping_add(1);
    }

    /// Adds the given `delta` number of milliseconds to the playback position. May be negative.
    ///
    /// # Panics
    /// Panics if the state lock is poisoned.
    pub fn add_millis(&self, delta: i64) {
        let mut lock = self.state.lock().unwrap();
        let new_millis = lock.millis.saturating_add(delta);
        self.store_locked(&mut lock, new_millis);
        lock.generation = lock.generation.wrapping_add(1);
    }

    /// Adds the given `delta_seconds` to the playback position. May be negative.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "very unlikely to occur in practice"
    )]
    pub fn add_seconds(&self, delta_seconds: f64) {
        self.add_millis((delta_seconds * 1000.0).round() as i64);
    }

    /// Advances the playback position by the given number of frames, using the given frame rate.
    pub fn add_frames(&self, delta_frames: media::FrameDelta, frame_rate: &media::FrameRate) {
        let target_frame = self.current_frame(frame_rate) + delta_frames;
        self.set_to_frame(target_frame, frame_rate);
    }

    /// Sets the playback position to the given event start time.
    pub fn set_to_event(&self, new_value: subtitle::StartTime) {
        self.set_millis(new_value.0);
    }

    /// Sets the playback position to the start of the given frame.
    pub fn set_to_frame(&self, new_value: media::FrameNumber, frame_rate: &media::FrameRate) {
        self.set_millis(
            frame_rate
                .time_at_frame(new_value, media::TimeMode::Exact)
                .0,
        );
    }

    /// Sets the range, in milliseconds, the playback position is clamped to. Both ends are
    /// inclusive.
    ///
    /// The current position will be instantly clamped into the new range and marked as seeked.
    ///
    /// # Panics
    /// Panics if the state lock is poisoned.
    pub fn set_bounds(&self, min_millis: i64, max_millis: i64) {
        let mut lock = self.state.lock().unwrap();
        lock.min_millis = min_millis;
        lock.max_millis = max_millis.max(min_millis);
        let current_millis = lock.millis;
        self.store_locked(&mut lock, current_millis);
        lock.generation = lock.generation.wrapping_add(1);
    }

    /// Returns the authoritative playback position together with its generation, for a playback
    /// engine to synchronise its own cursor against.
    ///
    /// # Panics
    /// Panics if the state lock is poisoned.
    pub fn snapshot(&self) -> Snapshot {
        let lock = self.state.lock().unwrap();
        Snapshot {
            millis: lock.millis,
            generation: lock.generation,
        }
    }

    /// Applies a position update from a playback engine, without marking the position as seeked.
    ///
    /// `expected_generation` is the generation the engine last synchronized against. If the
    /// position has been seeked since then, the engine's advance was computed from an
    /// outdated position, so it is discarded and the method will return `Advance::Discarded`.
    ///
    /// # Panics
    /// Panics if the state lock is poisoned.
    pub fn advance_to(&self, new_millis: i64, expected_generation: u64) -> Advance {
        dbg!(new_millis);

        let mut lock = self.state.lock().unwrap();

        if lock.generation != expected_generation {
            return Advance::Discarded;
        }

        if self.store_locked(&mut lock, new_millis) {
            // We ran into the end of the clamp range. Bump the generation, so the engine
            // re-synchronises instead of letting its own cursor run away past the end.
            lock.generation = lock.generation.wrapping_add(1);
            Advance::ReachedEnd
        } else {
            Advance::Applied
        }
    }
}

impl Default for Position {
    fn default() -> Self {
        Self {
            state: Mutex::new(State {
                millis: 0,
                min_millis: 0,
                max_millis: 0,
                generation: 0,
            }),
            cached_millis: AtomicI64::new(0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_float_eq::assert_float_absolute_eq;

    fn make_position(millis: i64) -> Position {
        Position {
            state: Mutex::new(State {
                millis,
                min_millis: 0,
                max_millis: i64::MAX,
                generation: 0,
            }),
            cached_millis: AtomicI64::new(millis),
        }
    }

    fn make_bounded_position(millis: i64, max_millis: i64) -> Position {
        let position = make_position(millis);
        position.set_bounds(0, max_millis);
        position
    }

    #[test]
    fn seconds_normal() {
        assert_float_absolute_eq!(make_position(0).seconds(), 0.0, 0.0001);
        assert_float_absolute_eq!(make_position(1000).seconds(), 1.0, 0.0001);
        assert_float_absolute_eq!(make_position(500).seconds(), 0.5, 0.0001);
        assert_float_absolute_eq!(make_position(3000).seconds(), 3.0, 0.0001);
    }

    #[test]
    fn subtitle_time_basic() {
        assert_eq!(make_position(0).subtitle_time(), subtitle::StartTime(0));
        assert_eq!(
            make_position(1000).subtitle_time(),
            subtitle::StartTime(1000)
        );
        assert_eq!(
            make_position(2500).subtitle_time(),
            subtitle::StartTime(2500)
        );
    }

    #[test]
    fn current_frame_24fps() {
        let frame_rate = media::FrameRate::f24();

        // 1 second → frame 24
        assert_eq!(
            make_position(1000).current_frame(&frame_rate),
            media::FrameNumber(24)
        );
        // 0 ms → frame 0
        assert_eq!(
            make_position(0).current_frame(&frame_rate),
            media::FrameNumber(0)
        );
        // 500 ms → frame 12
        assert_eq!(
            make_position(500).current_frame(&frame_rate),
            media::FrameNumber(12)
        );
        // Rounds down: one millisecond less than a full frame
        assert_eq!(
            make_position(41).current_frame(&frame_rate),
            media::FrameNumber(0)
        );
        assert_eq!(
            make_position(42).current_frame(&frame_rate),
            media::FrameNumber(1)
        );
    }

    #[test]
    fn add_millis_basic() {
        let position = make_position(0);
        position.add_millis(100);
        assert_eq!(position.millis(), 100);
        position.add_millis(50);
        assert_eq!(position.millis(), 150);
        position.add_millis(-100);
        assert_eq!(position.millis(), 50);
    }

    #[test]
    fn add_millis_clamp() {
        let position = make_bounded_position(10, 5000);
        position.add_millis(-100);
        assert_eq!(position.millis(), 0);

        position.add_millis(10000);
        assert_eq!(position.millis(), 5000);
    }

    #[test]
    fn set_to_event_basic() {
        let position = make_position(0);
        position.set_to_event(subtitle::StartTime(2000));
        assert_eq!(position.millis(), 2000);
        position.set_to_event(subtitle::StartTime(0));
        assert_eq!(position.millis(), 0);
    }

    #[test]
    fn set_to_event_clamp() {
        let position = make_bounded_position(1000, 5000);
        position.set_to_event(subtitle::StartTime(-5000));
        assert_eq!(position.millis(), 0);
        position.set_to_event(subtitle::StartTime(10000));
        assert_eq!(position.millis(), 5000);
    }

    #[test]
    fn set_to_frame() {
        let position = make_position(0);
        let frame_rate = media::FrameRate::f24();

        position.set_to_frame(media::FrameNumber(24), &frame_rate);
        assert_eq!(position.millis(), 1000);

        let position = make_position(0);
        let frame_rate = media::FrameRate::cfr(24000, 1001).unwrap();

        position.set_to_frame(media::FrameNumber(13), &frame_rate);
        assert_eq!(position.millis(), 542);
    }

    #[test]
    fn set_bounds() {
        let position = make_position(10000);
        assert_eq!(position.millis(), 10000);

        position.set_bounds(0, 5000);
        assert_eq!(position.millis(), 5000);
        assert_eq!(position.snapshot().millis, 5000);

        position.set_bounds(8000, 10000);
        assert_eq!(position.millis(), 8000);
    }

    #[test]
    fn seek_bumps_generation() {
        let position = make_position(0);
        let before = position.snapshot().generation;
        position.set_millis(1000);
        assert_ne!(position.snapshot().generation, before);
    }

    #[test]
    fn advance_to() {
        let position = make_position(0);
        let snapshot = position.snapshot();

        assert_eq!(
            position.advance_to(1000, snapshot.generation),
            Advance::Applied
        );
        assert_eq!(position.millis(), 1000);

        // The engine's cursor is still valid, so it should not be forced to re-synchronise.
        assert_eq!(position.snapshot().generation, snapshot.generation);

        // Test discarding an outdated advance
        let position = make_position(0);
        let snapshot = position.snapshot();

        // Something seeks while the engine is preparing its data...
        position.set_millis(30000);

        // ...so the advance the engine computed from the old position must not clobber it.
        assert_eq!(
            position.advance_to(1000, snapshot.generation),
            Advance::Discarded
        );
        assert_eq!(position.millis(), 30000);

        // Test reaching the end of the video
        let position = make_bounded_position(4900, 5000);
        let snapshot = position.snapshot();

        assert_eq!(
            position.advance_to(6000, snapshot.generation),
            Advance::ReachedEnd
        );
        assert_eq!(position.millis(), 5000);

        // The engine must re-synchronise, so that its own cursor cannot run away past the end.
        assert_ne!(position.snapshot().generation, snapshot.generation);
    }
}
