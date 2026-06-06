use crate::{media, subtitle};
use std::cmp::Ordering;
use std::ops::{Add, AddAssign, Sub, SubAssign};
use std::sync::LazyLock;

/// Identifies a video frame by number.
#[derive(
    Debug, Clone, Copy, PartialOrd, Ord, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct Number(pub i32);

/// A difference in counted video frames.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Delta(pub i32);

impl Number {
    #[must_use]
    pub fn step(self, direction: super::motion::Direction) -> Self {
        match direction {
            super::motion::Direction::Forward => Self(self.0 + 1),
            super::motion::Direction::Backward => Self(self.0 - 1),
        }
    }
}

impl std::fmt::Display for Number {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Add<Delta> for Number {
    type Output = Number;

    fn add(self, rhs: Delta) -> Self::Output {
        Number(self.0 + rhs.0)
    }
}

impl AddAssign<Delta> for Number {
    fn add_assign(&mut self, rhs: Delta) {
        self.0 += rhs.0;
    }
}

impl Sub<Delta> for Number {
    type Output = Number;

    fn sub(self, rhs: Delta) -> Self::Output {
        Number(self.0 - rhs.0)
    }
}

impl SubAssign<Delta> for Number {
    fn sub_assign(&mut self, rhs: Delta) {
        self.0 -= rhs.0;
    }
}

impl Sub<Number> for Number {
    type Output = Delta;

    fn sub(self, rhs: Number) -> Self::Output {
        Delta(self.0 - rhs.0)
    }
}

/// Frame-time conversion interpretation mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimeMode {
    /// Use the actual frame times. With 1 FPS video, frame 0 is `[0, 999]` ms.
    Exact,

    /// Start-of-event rule: an event is first visible on the first frame whose
    /// start time is *less than or equal to* the event's start time.
    /// With 1 FPS, frame 0 is `[-999, 0]` ms.
    Start,

    /// End-of-event rule: an event is last visible on the last frame whose
    /// start time is `<` the event's end time.
    /// Note that it is interpreted as the frame *on which* the event is last visible (inclusive).
    /// With 1 FPS, frame 0 is `[1, 1000]` ms.
    EndInclusive,
}

/// Errors that can arise while building a [`Framerate`] from timecodes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FramerateError {
    /// Fewer than two timecodes were supplied.
    TooFewTimecodes,

    /// Timecodes were not monotonically non-decreasing.
    NotSorted,

    /// All timecodes were identical (zero total duration).
    AllIdentical,

    /// A CFR numerator/denominator (or FPS) was non-positive or out of range.
    InvalidTimebase,
}

impl std::fmt::Display for FramerateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let string = match *self {
            FramerateError::TooFewTimecodes => {
                "must have at least two timecodes to do anything useful"
            }
            FramerateError::NotSorted => "timecodes are out of order",
            FramerateError::AllIdentical => "timecodes are all identical",
            FramerateError::InvalidTimebase => {
                "framerate numerator and denominator must both be positive and in range"
            }
        };
        f.write_str(string)
    }
}

impl std::error::Error for FramerateError {}

/// One linear piece of the timecode function.
#[derive(Clone, Copy, Debug)]
struct Segment {
    first_frame: i64,
    last_frame: i64,

    /// Exact time of `first_frame`, in milliseconds.
    start_time: i64,

    /// Average ms-per-frame over the piece, scaled by `STEP_SCALE`.
    step_scaled: i64,

    /// Residual bound (in frames) of the linear model over this piece.
    eps: i64,
}

impl Segment {
    /// Return the predicted frame for time `ms` within this segment,
    /// by inverting the linear model.
    fn predict_frame(&self, ms: i64) -> i64 {
        let dt = ms - self.start_time;
        self.first_frame + (dt * STEP_SCALE) / self.step_scaled
    }
}

/// Fixed-point scale for `step_scaled`. Large enough that the per-frame
/// duration (at most ~1e3 ms for >=1 FPS, but we don't assume a lower bound) is
/// represented with ample precision, small enough that `step_scaled * frames`
/// cannot overflow `i64` for any plausible video. With a 1e6 scale, a frame
/// index up to ~9.2e6 (over 100 hours at 24 fps) multiplied by a ~1e9 step
/// stays well within `i64`.
const STEP_SCALE: i64 = 1_000_000;

/// Maximum residual a segment is allowed to accumulate before it is split.
/// Smaller -> more segments, tighter local windows; larger -> fewer segments,
/// wider windows. The optimum is content-dependent; this bounds the per-lookup
/// correction work regardless of how non-linear a run is.
const MAX_EPS: i64 = 8;

/// Immutable VFR framerate backed by a piecewise-linear timecode index.
///
/// The in-range frame lookup is backed by a
/// piecewise-linear index over the timecodes rather than a binary search over
/// the full table. A real VFR timecode stream is piecewise-CFR: it is an
/// (almost) exactly linear function of frame index within each constant-rate
/// run. We segment the table once at construction into a small number of
/// linear pieces, each carrying a residual bound `eps`, so a lookup is:
///
///   1. binary-search the (tiny) segment list by time      -> O(log s)
///   2. predict the frame from the segment's linear model
///   3. correct within a `[pred - eps, pred + eps]` window  -> O(log eps)
///
/// against the real timecodes, which remain ground truth. The model only ever
/// *narrows* the search window; it never decides the answer on its own, so the
/// result is bit-for-bit identical to a binary search over the full table.
///
/// The interface is inspired by Aegisub's `agi::vfr::Framerate`, although the
/// implementation is more or less custom.
#[derive(Clone, Debug)]
pub struct Framerate {
    /// Exact start time, in milliseconds, of every frame.
    timecodes: Vec<i64>,

    /// Piecewise-linear segments, ordered by time.
    /// Guaranteed to be non-empty once built.
    segments: Vec<Segment>,

    /// Average framerate numerator.
    numerator: i64,

    /// Average framerate denominator.
    denominator: i64,
}

const DEFAULT_DENOMINATOR: i64 = 1_000_000_000;

impl Framerate {
    /// Build a framerate from a `Vec` of per-frame start times in
    /// milliseconds. Frame 0 is shifted to time 0 (matching Aegisub's
    /// `normalize_timecodes`). The input must be monotonically non-decreasing
    /// and contain at least two distinct values.
    ///
    /// # Panics
    /// Panics on certain overflow conditions.
    pub fn from_timecodes(mut timecodes: Vec<i64>) -> Result<Self, FramerateError> {
        if timecodes.len() <= 1 {
            return Err(FramerateError::TooFewTimecodes);
        }
        if !timecodes.windows(2).all(|w| {
            assert_eq!(w.len(), 2, "elide bounds checks");
            w[0] <= w[1]
        }) {
            return Err(FramerateError::NotSorted);
        }
        if timecodes.first() == timecodes.last() {
            return Err(FramerateError::AllIdentical);
        }

        // Normalize so frame 0 is at t = 0.
        let front = timecodes[0];
        if front != 0 {
            for timecode in &mut timecodes {
                *timecode -= front;
            }
        }

        let n: i64 = timecodes
            .len()
            .try_into()
            .expect("timecode length overflow");
        let last_ms = *timecodes.last().unwrap();
        let denominator = DEFAULT_DENOMINATOR;
        // numerator = (frames - 1) * denominator * 1000 / last_ms, computed in
        // i128 to avoid overflow (the product is ~1e15..1e16 before division).
        let numerator: i64 = {
            let num = i128::from(n - 1) * i128::from(denominator) * 1000;
            (num / i128::from(last_ms))
                .try_into()
                .expect("FPS numerator overflow")
        };

        let segments = Self::build_segments(&timecodes);

        Ok(Framerate {
            timecodes,
            segments,
            numerator,
            denominator,
        })
    }

    /// Same as `from_timecodes`, but takes an iterator instead.
    pub fn from_timecodes_iter<I>(iter: I) -> Result<Self, FramerateError>
    where
        I: IntoIterator<Item = i64>,
    {
        let timecodes: Vec<i64> = iter.into_iter().collect();
        Self::from_timecodes(timecodes)
    }

    /// Greedy piecewise-linear segmentation in a single **O(n)** pass using the
    /// shrinking-cone method (the FITing-tree / Swing-filter family).
    ///
    /// We model frame index as a linear function of elapsed time within a
    /// segment: `frame ≈ first_frame + (t - t0) * inv_slope`, and require every
    /// frame in the segment to be within `±MAX_EPS` of that line. For each new
    /// frame we form the admissible *inverse-slope* interval that keeps it (and,
    /// by the running intersection, all prior frames) within the bound, and
    /// intersect it into the running cone. When the cone becomes empty the
    /// segment is closed at the previous frame and a new one started.
    ///
    /// This avoids any quadratic interior re-scan: each frame is touched O(1)
    /// times while growing, plus one linear pass per closed segment to compute a
    /// tight residual.
    fn build_segments(timecodes: &[i64]) -> Vec<Segment> {
        let num_frames = timecodes.len();
        let mut segments = Vec::new();
        if num_frames == 0 {
            return segments;
        }

        let mut segment_start = 0_usize;
        while segment_start < num_frames {
            let segment_start_time = timecodes[segment_start];
            #[expect(
                clippy::cast_possible_wrap,
                reason = "num_frames is guaranteed to fit into i64"
            )]
            let segment_start_frame = segment_start as i64;

            // Admissible inverse-slope cone (frames-per-ms, scaled by
            // STEP_SCALE). Prediction is frame = f0 + dt*inv/STEP_SCALE, dt >= 0.
            // For frame k with dt = t[k]-t0 > 0, requiring
            //   |f0 + dt*inv/STEP_SCALE - k| <= eps
            // gives inv in [ (k-f0-eps)*STEP_SCALE/dt , (k-f0+eps)*STEP_SCALE/dt ].
            let mut inv_lo = i64::MIN;
            let mut inv_hi = i64::MAX;
            let mut segment_end = segment_start;

            let mut frame = segment_start + 1;
            while frame < num_frames {
                let dt = timecodes[frame] - segment_start_time;
                if dt <= 0 {
                    // Zero-duration frame: imposes no usable slope
                    // constraint. Fold it in and let the local correction resolve
                    // the tie at lookup time.
                    segment_end = frame;
                    frame += 1;
                    continue;
                }

                #[expect(
                    clippy::cast_possible_wrap,
                    reason = "num_frames is guaranteed to fit into i64"
                )]
                let df = (frame as i64) - segment_start_frame;

                let cand_lo = ((df - MAX_EPS) * STEP_SCALE).div_euclid(dt);
                let cand_hi = ((df + MAX_EPS) * STEP_SCALE).div_euclid(dt);
                let new_lo = inv_lo.max(cand_lo);
                let new_hi = inv_hi.min(cand_hi);
                if new_lo > new_hi {
                    break; // cone empty: cannot extend
                }
                inv_lo = new_lo;
                inv_hi = new_hi;
                segment_end = frame;
                frame += 1;
            }

            // Representative inverse slope: prefer the endpoint fit (best average
            // over the run) clamped into the cone; fall back to the cone middle.
            let inv_scaled = if segment_end > segment_start {
                let dt = timecodes[segment_end] - segment_start_time;
                #[expect(
                    clippy::cast_possible_wrap,
                    reason = "num_frames is guaranteed to fit into i64"
                )]
                let df = (segment_end as i64) - segment_start_frame;
                let endpoint = if dt > 0 {
                    (df * STEP_SCALE) / dt
                } else {
                    let lo = if inv_lo == i64::MIN { 1 } else { inv_lo };
                    let hi = if inv_hi == i64::MAX { lo } else { inv_hi };
                    i64::midpoint(lo, hi)
                };
                let lo = if inv_lo == i64::MIN { endpoint } else { inv_lo };
                let hi = if inv_hi == i64::MAX { endpoint } else { inv_hi };
                endpoint.clamp(lo.min(hi), lo.max(hi))
            } else if segment_start + 1 < num_frames {
                // Single-frame segment: borrow slope from the next frame.
                let dt = (timecodes[segment_start + 1] - segment_start_time).max(1);
                STEP_SCALE / dt
            } else {
                1
            };

            // Convert inverse slope back to a ms/frame step (scaled) so that
            // predict_frame's `f0 + dt*STEP_SCALE/step_scaled` reproduces
            // `f0 + dt*inv/STEP_SCALE` exactly: step_scaled = STEP_SCALE^2 / inv.
            let inv_eff = inv_scaled.max(1);
            let step_scaled = ((STEP_SCALE * STEP_SCALE) / inv_eff).max(1);

            // Tighten `eps` to the true worst residual under the chosen slope.
            let mut eps = 0_i64;
            for (residual_frame, frame_time) in
                timecodes[segment_start..=segment_end].iter().enumerate()
            {
                let dt = frame_time - segment_start_time;
                let pred = segment_start_frame + (dt * STEP_SCALE) / step_scaled;
                #[expect(
                    clippy::cast_possible_wrap,
                    reason = "num_frames is guaranteed to fit into i64"
                )]
                let residual = (pred - residual_frame as i64).abs();
                if residual > eps {
                    eps = residual;
                }
            }

            #[expect(
                clippy::cast_possible_wrap,
                reason = "num_frames is guaranteed to fit into i64"
            )]
            segments.push(Segment {
                first_frame: segment_start_frame,
                last_frame: segment_end as i64,
                start_time: segment_start_time,
                step_scaled,
                eps,
            });

            segment_start = segment_end + 1;
        }

        segments
    }

    /// Build a constant-framerate (CFR) framerate from a rational timebase.
    ///
    /// `numerator / denominator` is the frames-per-second.
    /// The timecode table holds only a single entry, so every lookup falls
    /// through to the analytic average-framerate path (there is no per-frame
    /// table to search). A single trivial segment is added so the segment-index
    /// invariants hold uniformly.
    ///
    /// Returns an error if either component is non-positive.
    ///
    /// # Panics
    /// Panics on certain overflow conditions.
    pub fn cfr(numerator: i64, denominator: i64) -> Result<Self, FramerateError> {
        if numerator <= 0 || denominator <= 0 {
            return Err(FramerateError::InvalidTimebase);
        }

        let timecodes = vec![0_i64];

        // One degenerate segment covering frame 0 at t0 = 0, with the CFR slope.
        // step (ms/frame) = 1000 * denominator / numerator; scaled by STEP_SCALE.
        let step_scaled: i64 = ((1000_i128 * i128::from(denominator) * i128::from(STEP_SCALE))
            / i128::from(numerator))
        .max(1)
        .try_into()
        .expect("step_scaled overflow");

        let segments = vec![Segment {
            first_frame: 0,
            last_frame: 0,
            start_time: 0,
            step_scaled,
            eps: 0,
        }];

        Ok(Framerate {
            timecodes,
            segments,
            numerator,
            denominator,
        })
    }

    /// Convenience CFR constructor from a floating-point FPS.
    pub fn cfr_fps(fps: f64) -> Result<Self, FramerateError> {
        if !(fps.is_finite() && fps > 0.0 && fps <= 1000.0) {
            return Err(FramerateError::InvalidTimebase);
        }
        let denominator = DEFAULT_DENOMINATOR;
        #[expect(clippy::cast_precision_loss, reason = "unavoidable")]
        #[expect(clippy::cast_possible_truncation, reason = "unavoidable")]
        let numerator = (fps * denominator as f64) as i64;
        Framerate::cfr(numerator, denominator)
    }

    /// 24 frames per second.
    #[expect(
        clippy::missing_panics_doc,
        reason = "will never panic because constants are specified"
    )]
    #[must_use]
    pub fn f24() -> Self {
        Self::cfr(DEFAULT_DENOMINATOR * 24, DEFAULT_DENOMINATOR).unwrap()
    }

    #[must_use]
    pub fn numerator(&self) -> i64 {
        self.numerator
    }

    #[must_use]
    pub fn denominator(&self) -> i64 {
        self.denominator
    }

    /// Average frames-per-second.
    #[must_use]
    pub fn fps(&self) -> f64 {
        #[expect(clippy::cast_precision_loss, reason = "unavoidable")]
        let fps = self.numerator as f64 / self.denominator as f64;
        fps
    }

    /// Number of linear segments in the index (diagnostic / test aid).
    #[must_use]
    pub fn segment_count(&self) -> usize {
        self.segments.len()
    }

    /// Number of frames in the timecode table.
    #[must_use]
    pub fn len(&self) -> usize {
        self.timecodes.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.timecodes.is_empty()
    }

    /// Frame visible at time `ms` (milliseconds) under the given [`TimeMode`] interpretation.
    #[must_use]
    pub fn frame_at_time(&self, time: subtitle::StartTime, mode: TimeMode) -> Number {
        let ms = time.0;

        let frame = match mode {
            TimeMode::Start => self.frame_at_time_exact(ms - 1) + 1,
            TimeMode::EndInclusive => self.frame_at_time_exact(ms - 1),
            TimeMode::Exact => self.frame_at_time_exact(ms),
        };

        Number(frame)
    }

    /// EXACT-mode frame lookup: find the last frame whose start time
    /// is less than or equal to the given time, in milliseconds.
    fn frame_at_time_exact(&self, ms: i64) -> i32 {
        #[expect(
            clippy::cast_possible_wrap,
            reason = "num_frames is guaranteed to fit into i64"
        )]
        let last_idx = (self.timecodes.len() - 1) as i64;
        let last_frame_ms = *self.timecodes.last().unwrap();

        // Extrapolate before the start, assuming average framerate.
        if ms < 0 {
            let extrapolated =
                (i128::from(ms) * i128::from(self.numerator) / i128::from(self.denominator) - 999)
                    / 1000;
            return extrapolated
                .try_into()
                .expect("extrapolated frame overflow");
        }

        // Extrapolate beyond the end, assuming average framerate.
        if ms > last_frame_ms {
            let denominator = i128::from(self.denominator);
            let self_numerator = i128::from(self.numerator);
            let last_fs = i128::from(last_idx) * denominator * 1000;
            let frame_numerator =
                (i128::from(ms) + 1) * self_numerator - last_fs - self_numerator / 2
                    + (1000 * denominator - 1);
            let frame = frame_numerator / (1000 * denominator) + (self.timecodes.len() as i128 - 2);
            return frame.try_into().expect("extrapolated frame overflow");
        }

        // If we are in range:
        // locate the segment whose time span contains `ms`,
        // predict the frame by inverting the linear model,
        // then correct within the `eps` window against the real timecodes.
        let segment = self.locate_segment(ms);
        let predicted = segment.predict_frame(ms);

        // Clamp the search window to the segment's frame span (the answer is
        // guaranteed to lie within [first_frame, last_frame] for an in-segment
        // time, and within ±eps of pred).
        let lower = (predicted - segment.eps).max(segment.first_frame);
        let upper = (predicted + segment.eps).min(segment.last_frame);

        #[expect(clippy::cast_sign_loss, reason = "guaranteed to not be negative")]
        #[expect(clippy::cast_possible_truncation, reason = "64 bit only")]
        let exact = self
            .correct_exact(ms, lower as usize, upper as usize)
            .try_into()
            .expect("frame overflow");
        exact
    }

    /// Binary search the segment list for the piece whose time range contains
    /// `ms`. Segments are contiguous in time, so we find the last segment whose
    /// `t0 <= ms`.
    fn locate_segment(&self, ms: i64) -> &Segment {
        // Find the first index where t0 > ms.
        // The segment before that one contains `ms`.
        let partition_point = self
            .segments
            .partition_point(|segment| segment.start_time <= ms);
        let index = partition_point.saturating_sub(1);
        &self.segments[index]
    }

    /// Between `lower` and `upper` (both inclusive),
    /// return the largest frame index `i` with `timecodes[i] <= ms`.
    /// The window is guaranteed (by the eps bound) to
    /// bracket the answer for an in-range `ms`. Uses binary search over the
    /// short window; for the common tiny eps this is a handful of steps.
    fn correct_exact(&self, ms: i64, mut lower: usize, mut upper: usize) -> i64 {
        let timecodes = &self.timecodes;

        // Defensive expansion in case the model window is off by the boundary:
        // these loops run at most a couple of times given the eps guarantee.
        while lower > 0 && timecodes[lower] > ms {
            lower -= 1;
        }
        while upper < timecodes.len() - 1 && timecodes[upper + 1] <= ms {
            upper += 1;
        }

        // Binary search for the boundary.
        while lower < upper {
            // Bias the midpoint upward so we converge on the last <= ms.
            let mid = (lower + upper).div_ceil(2);

            match timecodes[mid].cmp(&ms) {
                Ordering::Less | Ordering::Equal => lower = mid,
                Ordering::Greater => upper = mid - 1,
            }
        }

        #[expect(
            clippy::cast_possible_wrap,
            reason = "num_frames is guaranteed to fit into i64"
        )]
        let lower_i64 = lower as i64;
        lower_i64
    }

    /// Time (ms) at the start/within the range of a frame, under the mode.
    #[must_use]
    pub fn time_at_frame(&self, frame: Number, mode: TimeMode) -> subtitle::StartTime {
        let ms = match mode {
            TimeMode::Start => {
                let prev = self.time_at_frame_exact(frame.0 - 1);
                let cur = self.time_at_frame_exact(frame.0);
                // +1 so two frames 1 ms apart round up, matching Aegisub.
                prev + (cur - prev + 1) / 2
            }
            TimeMode::EndInclusive => {
                let cur = self.time_at_frame_exact(frame.0);
                let next = self.time_at_frame_exact(frame.0 + 1);
                cur + (next - cur + 1) / 2
            }
            TimeMode::Exact => self.time_at_frame_exact(frame.0),
        };

        subtitle::StartTime(ms)
    }

    /// Retrieve the given frame's time, or extrapolate if necessary.
    fn time_at_frame_exact(&self, frame: i32) -> i64 {
        // If the timecodes is empty, or if the frame is negative,
        // simply extrapolate from the FPS values.
        if self.timecodes.is_empty() || frame < 0 {
            let cfr_ms = i128::from(frame) * i128::from(self.denominator) * 1000
                / i128::from(self.numerator);
            return cfr_ms.try_into().expect("cfr_ms overflow");
        }

        #[expect(
            clippy::cast_possible_wrap,
            reason = "num_frames is guaranteed to fit into i64"
        )]
        let n = self.timecodes.len() as i64;

        // If the frame is beyond the end of the timecodes, extrapolate as well,
        // with the more complex logic required.
        if i64::from(frame) >= n {
            let frames_past_end = i128::from(frame) - i128::from(n) + 1;
            let den = i128::from(self.denominator);
            let num = i128::from(self.numerator);
            let last_fs = i128::from(n - 1) * den * 1000;
            let cfr_ms = (frames_past_end * 1000 * den + last_fs + num / 2) / num;
            return cfr_ms.try_into().expect("cfr_ms overflow");
        }

        // Otherwise, simply return the frame's time
        #[expect(clippy::cast_sign_loss, reason = "guaranteed to not be negative")]
        let frame_ms = self.timecodes[frame as usize];
        frame_ms
    }

    pub fn iter_from(&self, frame: Number) -> impl Iterator<Item = (Number, subtitle::StartTime)> {
        FrameIterator {
            frame_rate: self,
            current: frame,
        }
    }
}

struct FrameIterator<'a> {
    frame_rate: &'a Framerate,
    current: Number,
}

impl Iterator for FrameIterator<'_> {
    type Item = (Number, subtitle::StartTime);

    fn next(&mut self) -> Option<Self::Item> {
        self.current += Delta(1);
        Some((
            self.current,
            self.frame_rate.time_at_frame(self.current, TimeMode::Exact),
        ))
    }
}

pub static UNLOADED_FRAMERATE: LazyLock<media::FrameRate> = LazyLock::new(media::FrameRate::f24);

#[expect(
    clippy::cast_possible_truncation,
    reason = "test numbers are within bounds"
)]
pub mod util {
    /// Simpler reference implementation for `frame_at_exact`:
    /// a straight binary search over the full table.
    ///
    /// # Panics
    /// Panics on overflow.
    #[must_use]
    pub fn ref_frame_at_exact(timecodes: &[i64], numerator: i64, denominator: i64, ms: i64) -> i32 {
        let last_frame_ms = *timecodes.last().unwrap();
        let last_idx: i64 = (timecodes.len() - 1).try_into().unwrap();
        if ms < 0 {
            return ((i128::from(ms) * i128::from(numerator) / i128::from(denominator) - 999)
                / 1000)
                .try_into()
                .unwrap();
        }
        if ms > last_frame_ms {
            let den = i128::from(denominator);
            let num = i128::from(numerator);
            let last_fs = i128::from(last_idx) * den * 1000;
            let numer = (i128::from(ms) + 1) * num - last_fs - num / 2 + (1000 * den - 1);
            return (numer / (1000 * den) + (timecodes.len() as i128 - 2))
                .try_into()
                .unwrap();
        }
        // last i with tc[i] <= ms
        let idx = timecodes.partition_point(|&time| time <= ms);
        (idx - 1).try_into().unwrap()
    }

    /// Build a CFR timecode table at `fps` for `frames` frames.
    #[must_use]
    pub fn cfr_timecodes(fps: f64, frames: usize) -> Vec<i64> {
        let mut out = Vec::with_capacity(frames);
        let mut time = 0.0_f64;
        for _ in 0..frames {
            out.push((time + 0.5) as i64);
            time += 1000.0 / fps;
        }
        out
    }

    /// Build a piecewise-CFR table by alternating runs of various
    /// framerates and lengths.
    #[must_use]
    pub fn vfr_timecodes(runs: &[(f64, usize)]) -> Vec<i64> {
        let mut out = Vec::new();
        let mut time = 0.0_f64;
        for &(fps, count) in runs {
            for _ in 0..count {
                out.push((time + 0.5) as i64);
                time += 1000.0 / fps;
            }
        }
        out
    }
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "test numbers are within bounds"
)]
#[expect(clippy::cast_sign_loss, reason = "test numbers are within bounds")]
#[expect(clippy::cast_possible_wrap, reason = "64 bit only")]
#[cfg(test)]
mod tests {
    use super::util::*;
    use super::*;

    #[test]
    fn frame_timing_24() {
        let frame_rate = Framerate::f24();

        // Equivalent behavior to the old CFR-only `FrameRate`:
        // `ass_time_to_frame` ^= `frame_at_time(..., Exact)`
        // `ass_time_to_frame_after` ^= `frame_at_time(..., EndInclusive) + Delta(1)`
        assert_eq!(
            frame_rate.frame_at_time(subtitle::StartTime(0), TimeMode::Exact),
            Number(0)
        );
        assert_eq!(
            frame_rate.frame_at_time(subtitle::StartTime(0), TimeMode::EndInclusive) + Delta(1),
            Number(0)
        );

        assert_eq!(
            frame_rate.frame_at_time(subtitle::StartTime(1), TimeMode::Exact),
            Number(0)
        );
        assert_eq!(
            frame_rate.frame_at_time(subtitle::StartTime(1), TimeMode::EndInclusive) + Delta(1),
            Number(1)
        );

        assert_eq!(
            frame_rate.frame_at_time(subtitle::StartTime(999), TimeMode::Exact),
            Number(23)
        );
        assert_eq!(
            frame_rate.frame_at_time(subtitle::StartTime(999), TimeMode::EndInclusive) + Delta(1),
            Number(24)
        );

        assert_eq!(
            frame_rate.frame_at_time(subtitle::StartTime(1000), TimeMode::Exact),
            Number(24)
        );
        assert_eq!(
            frame_rate.frame_at_time(subtitle::StartTime(1000), TimeMode::EndInclusive) + Delta(1),
            Number(24)
        );
    }

    #[test]
    fn frame_timing_23_976() {
        let frame_rate = Framerate::cfr(24000, 1001).unwrap();

        assert_eq!(
            frame_rate.frame_at_time(subtitle::StartTime(0), TimeMode::Exact),
            Number(0)
        );
        assert_eq!(
            frame_rate.frame_at_time(subtitle::StartTime(0), TimeMode::EndInclusive) + Delta(1),
            Number(0)
        );

        assert_eq!(
            frame_rate.frame_at_time(subtitle::StartTime(1), TimeMode::Exact),
            Number(0)
        );
        assert_eq!(
            frame_rate.frame_at_time(subtitle::StartTime(1), TimeMode::EndInclusive) + Delta(1),
            Number(1)
        );

        assert_eq!(
            frame_rate.frame_at_time(subtitle::StartTime(1000), TimeMode::Exact),
            Number(23)
        );
        assert_eq!(
            frame_rate.frame_at_time(subtitle::StartTime(1000), TimeMode::EndInclusive) + Delta(1),
            Number(24)
        );
    }

    #[test]
    fn rejects_degenerate_input() {
        assert_eq!(
            Framerate::from_timecodes(vec![5_i64]).unwrap_err(),
            FramerateError::TooFewTimecodes
        );
        assert_eq!(
            Framerate::from_timecodes(vec![0_i64, 100, 50]).unwrap_err(),
            FramerateError::NotSorted
        );
        assert_eq!(
            Framerate::from_timecodes(vec![10_i64, 10, 10]).unwrap_err(),
            FramerateError::AllIdentical
        );
    }

    #[test]
    fn normalizes_to_zero() {
        // A table that doesn't start at 0 should be shifted.
        let fr = Framerate::from_timecodes(vec![100_i64, 142, 184, 226]).unwrap();
        assert_eq!(
            fr.time_at_frame(Number(0), TimeMode::Exact),
            subtitle::StartTime(0)
        );
        assert_eq!(
            fr.time_at_frame(Number(1), TimeMode::Exact),
            subtitle::StartTime(42)
        );
    }

    #[test]
    fn exact_roundtrip_cfr_24fps() {
        let timecodes = cfr_timecodes(24000.0 / 1001.0, 5000);
        let framerate = Framerate::from_timecodes_iter(timecodes.iter().copied()).unwrap();

        // Frame -> time -> frame must be the identity for every in-range frame.
        for (frame_index, timecode) in timecodes.iter().enumerate() {
            let frame = Number(frame_index as i32);
            let time = framerate.time_at_frame(frame, TimeMode::Exact);
            assert_eq!(time.0, *timecode, "time_at_frame({frame})");
            assert_eq!(
                framerate.frame_at_time(time, TimeMode::Exact),
                frame,
                "frame_at_time roundtrip at frame {frame}"
            );
        }
    }

    #[test]
    fn agrees_with_binary_search_cfr() {
        let tc = cfr_timecodes(24000.0 / 1001.0, 3000);
        let fr = Framerate::from_timecodes_iter(tc.iter().copied()).unwrap();
        let tc64: &[i64] = &tc;
        let (num, den) = (fr.numerator, fr.denominator);

        // Sweep every ms across the whole range plus generous out-of-range tails.
        let last = *tc64.last().unwrap();
        for ms in -2000..=(last + 2000) {
            let got = fr.frame_at_time(subtitle::StartTime(ms), TimeMode::Exact);
            let want = ref_frame_at_exact(tc64, num, den, ms);
            assert_eq!(got.0, want, "EXACT mismatch at ms={ms}");
        }
    }

    #[test]
    fn agrees_with_binary_search_vfr() {
        // Realistic mixed content: 23.976 telecine sections and 29.97 sections.
        let tc = vfr_timecodes(&[
            (24000.0 / 1001.0, 800),
            (30000.0 / 1001.0, 500),
            (24000.0 / 1001.0, 1200),
            (60000.0 / 1001.0, 300),
        ]);
        let fr = Framerate::from_timecodes_iter(tc.iter().copied()).unwrap();
        let tc64: &[i64] = &tc;
        let (num, den) = (fr.numerator, fr.denominator);

        let last = *tc64.last().unwrap();
        for ms in -1000..=(last + 1000) {
            let got = fr.frame_at_time(subtitle::StartTime(ms), TimeMode::Exact);
            let want = ref_frame_at_exact(tc64, num, den, ms);
            assert_eq!(got.0, want, "EXACT VFR mismatch at ms={ms}");
        }

        // The whole point: the segment count should be small (one per CFR run,
        // give or take a boundary split), not O(n).
        assert!(
            fr.segment_count() <= 12,
            "expected few segments, got {}",
            fr.segment_count()
        );
    }

    #[test]
    fn start_end_semantics_1fps() {
        // 1 FPS: frame boundaries every 1000 ms. The documented intervals are:
        //   EXACT: frame 0 = [0, 999]
        //   START: frame 0 = [-999, 0]
        //   END:   frame 0 = [1, 1000]
        let tc: Vec<i64> = (0..10).map(|frame| frame * 1000).collect();
        let fr = Framerate::from_timecodes(tc).unwrap();

        // EXACT
        assert_eq!(
            fr.frame_at_time(subtitle::StartTime(0), TimeMode::Exact),
            Number(0)
        );
        assert_eq!(
            fr.frame_at_time(subtitle::StartTime(999), TimeMode::Exact),
            Number(0)
        );
        assert_eq!(
            fr.frame_at_time(subtitle::StartTime(1000), TimeMode::Exact),
            Number(1)
        );

        // START: first frame whose start <= line start; frame 0 covers [-999, 0].
        assert_eq!(
            fr.frame_at_time(subtitle::StartTime(0), TimeMode::Start),
            Number(0)
        );
        assert_eq!(
            fr.frame_at_time(subtitle::StartTime(-999), TimeMode::Start),
            Number(0)
        );
        assert_eq!(
            fr.frame_at_time(subtitle::StartTime(1), TimeMode::Start),
            Number(1)
        );
        assert_eq!(
            fr.frame_at_time(subtitle::StartTime(1000), TimeMode::Start),
            Number(1)
        );

        // END: frame 0 covers [1, 1000].
        assert_eq!(
            fr.frame_at_time(subtitle::StartTime(1), TimeMode::EndInclusive),
            Number(0)
        );
        assert_eq!(
            fr.frame_at_time(subtitle::StartTime(1000), TimeMode::EndInclusive),
            Number(0)
        );
        assert_eq!(
            fr.frame_at_time(subtitle::StartTime(1001), TimeMode::EndInclusive),
            Number(1)
        );
    }

    #[test]
    fn start_end_relations_hold_everywhere() {
        // The algebraic identities Start(ms) = Exact(ms-1)+1 and
        // End(ms) = Exact(ms-1) must hold for all ms by construction; verify
        // against the reference EXACT on VFR data.
        let tc = vfr_timecodes(&[(24000.0 / 1001.0, 400), (30000.0 / 1001.0, 400)]);
        let last = *tc.last().unwrap();
        let fr = Framerate::from_timecodes(tc).unwrap();
        for ms in -500..=(last + 500) {
            let exact_prev = fr.frame_at_time(subtitle::StartTime(ms - 1), TimeMode::Exact);
            let time = subtitle::StartTime(ms);
            assert_eq!(
                fr.frame_at_time(time, TimeMode::Start),
                exact_prev + Delta(1)
            );
            assert_eq!(fr.frame_at_time(time, TimeMode::EndInclusive), exact_prev);
        }
    }

    #[test]
    fn time_at_frame_start_end_midpoints() {
        // With clean 1 FPS data, START time of a frame is the midpoint rounding
        // between prev and cur exact times; END between cur and next.
        let tc: Vec<i64> = (0..5).map(|frame| frame * 1000).collect();
        let fr = Framerate::from_timecodes(tc).unwrap();
        // Exact times are 0,1000,2000,3000,4000.
        // START of frame 2: prev=1000, cur=2000 -> 1000 + (1000+1)/2 = 1500.
        let frame = Number(2);
        assert_eq!(
            fr.time_at_frame(frame, TimeMode::Start),
            subtitle::StartTime(1500)
        );
        // END of frame 2: cur=2000, next=3000 -> 2000 + (1000+1)/2 = 2500.
        assert_eq!(
            fr.time_at_frame(frame, TimeMode::EndInclusive),
            subtitle::StartTime(2500)
        );
    }

    #[test]
    fn out_of_range_monotonic_and_invertible() {
        // Aegisub guarantees: TimeAtFrame outside the range is monotonic and
        // round-trips through FrameAtTime back to the original frame.
        let tc = cfr_timecodes(24000.0 / 1001.0, 200);
        let fr = Framerate::from_timecodes(tc).unwrap();
        let n = fr.len() as i32;

        let mut prev_t = subtitle::StartTime(i64::MIN);
        for frame_n in n..(n + 500) {
            let frame = Number(frame_n);
            let time = fr.time_at_frame(frame, TimeMode::Exact);
            assert!(
                time > prev_t,
                "time not monotonic past end at frame {frame}"
            );
            prev_t = time;
            assert_eq!(
                fr.frame_at_time(time, TimeMode::Exact),
                frame,
                "round-trip failed past end at frame {frame}"
            );
        }

        // Negative frames likewise.
        for frame_n in -300..0 {
            let frame = Number(frame_n);
            let time = fr.time_at_frame(frame, TimeMode::Exact);
            assert_eq!(
                fr.frame_at_time(time, TimeMode::Exact),
                frame,
                "round-trip failed before start at frame {frame}"
            );
        }
    }

    #[test]
    fn coincident_timecodes_are_handled() {
        // Some VFR streams contain frames with identical start times (zero-length
        // frames). The lookup must still return *a* valid frame and the
        // builder must not panic.
        let tc = vec![0_i64, 33, 33, 33, 66, 100, 133];
        let fr = Framerate::from_timecodes_iter(tc.iter().copied()).unwrap();
        let tc64: &[i64] = &tc;
        let (num, den) = (fr.numerator, fr.denominator);
        for ms in -50..=200 {
            let got = fr.frame_at_time(subtitle::StartTime(ms), TimeMode::Exact);
            let want = ref_frame_at_exact(tc64, num, den, ms);
            assert_eq!(got.0, want, "coincident mismatch at ms={ms}");
        }
    }

    #[test]
    fn large_table_builds_fast_and_compresses() {
        // 100k-frame piecewise-CFR table must (a) build well under any sane time
        // budget — the cone segmentation is O(n) — and (b) collapse to one
        // segment per CFR run, not O(n) segments. We also spot-check correctness
        // at the run boundaries where the model is most stressed.
        let runs = [
            (24000.0 / 1001.0, 40_000_usize),
            (30000.0 / 1001.0, 20_000),
            (24000.0 / 1001.0, 40_000),
        ];
        let tc = vfr_timecodes(&runs);
        let fr = Framerate::from_timecodes_iter(tc.iter().copied()).unwrap();
        assert_eq!(fr.len(), 100_000);
        assert!(
            fr.segment_count() <= 6,
            "expected ~3 segments, got {}",
            fr.segment_count()
        );

        let tc64: &[i64] = &tc;
        let (num, den) = (fr.numerator, fr.denominator);
        // Check correctness in windows around each run boundary.
        let boundaries = [0_i64, 40_000, 60_000, 99_999];
        for &boundary in &boundaries {
            let center = tc64[boundary as usize];
            for ms in (center - 100)..=(center + 100) {
                assert_eq!(
                    fr.frame_at_time(subtitle::StartTime(ms), TimeMode::Exact).0,
                    ref_frame_at_exact(tc64, num, den, ms),
                    "boundary mismatch near frame {boundary} at ms={ms}"
                );
            }
        }
    }

    #[test]
    fn segment_model_actually_compresses() {
        // Pure CFR is a single line: ideally one segment for the whole table.
        let tc = cfr_timecodes(24000.0 / 1001.0, 10000);
        let fr = Framerate::from_timecodes(tc).unwrap();
        // Round-off in CFR timecodes (the +0.5 rounding) introduces a bounded
        // sawtooth, so we may get a few segments, but it must be tiny relative
        // to the 10k frames.
        assert!(
            fr.segment_count() < 50,
            "CFR should compress to << n segments, got {}",
            fr.segment_count()
        );
    }

    #[test]
    fn cfr_rejects_bad_timebase() {
        assert_eq!(
            Framerate::cfr(0, 1).unwrap_err(),
            FramerateError::InvalidTimebase
        );
        assert_eq!(
            Framerate::cfr(24, 0).unwrap_err(),
            FramerateError::InvalidTimebase
        );
        assert_eq!(
            Framerate::cfr(-24, 1).unwrap_err(),
            FramerateError::InvalidTimebase
        );
        Framerate::cfr_fps(f64::NAN).unwrap_err();
        Framerate::cfr_fps(0.0).unwrap_err();
        Framerate::cfr_fps(-5.0).unwrap_err();
        Framerate::cfr_fps(1001.0).unwrap_err();
    }

    #[test]
    fn cfr_basic_properties() {
        // 25 fps exactly: numerator 25e9, denominator 1e9.
        let fr = Framerate::cfr(25 * DEFAULT_DENOMINATOR, DEFAULT_DENOMINATOR).unwrap();
        assert!((fr.fps() - 25.0).abs() < 1e-9);
        assert_eq!(fr.segment_count(), 1);

        // EXACT frame at frame n is exactly 40*n ms (1000/25), truncating.
        assert_eq!(
            fr.time_at_frame(Number(0), TimeMode::Exact),
            subtitle::StartTime(0)
        );
        assert_eq!(
            fr.time_at_frame(Number(1), TimeMode::Exact),
            subtitle::StartTime(40)
        );
        assert_eq!(
            fr.time_at_frame(Number(25), TimeMode::Exact),
            subtitle::StartTime(1000)
        );

        // FrameAtTime EXACT: last frame whose start <= ms.
        assert_eq!(
            fr.frame_at_time(subtitle::StartTime(0), TimeMode::Exact),
            Number(0)
        );
        assert_eq!(
            fr.frame_at_time(subtitle::StartTime(39), TimeMode::Exact),
            Number(0)
        );
        assert_eq!(
            fr.frame_at_time(subtitle::StartTime(40), TimeMode::Exact),
            Number(1)
        );
        assert_eq!(
            fr.frame_at_time(subtitle::StartTime(1000), TimeMode::Exact),
            Number(25)
        );
    }

    #[test]
    fn cfr_roundtrips_over_wide_range() {
        // Exact integer-ms framerate (1000/25 = 40 ms) round-trips cleanly for a
        // wide frame range, including well past where an i32 ms stamp would have
        // overflowed (frame 6e7 at 40 ms = 2.4e9 ms > i32::MAX).
        let fr = Framerate::cfr(25 * DEFAULT_DENOMINATOR, DEFAULT_DENOMINATOR).unwrap();
        for &frame_n in &[0_i32, 1, 100, 1_000_000, 60_000_000, 100_000_000] {
            let frame = Number(frame_n);
            let time = fr.time_at_frame(frame, TimeMode::Exact);
            assert!(
                time > subtitle::StartTime(i64::from(i32::MAX)) || frame_n < 53_687_092,
                "sanity: large frames should exceed i32 ms range"
            );
            assert_eq!(
                fr.frame_at_time(time, TimeMode::Exact),
                frame,
                "CFR round-trip failed at frame {frame} (t={time})"
            );
        }
    }

    #[test]
    fn cfr_start_end_semantics() {
        // 1 FPS CFR: same interval semantics as the timecode-built 1 FPS case.
        let fr = Framerate::cfr(DEFAULT_DENOMINATOR, DEFAULT_DENOMINATOR).unwrap();
        assert_eq!(
            fr.frame_at_time(subtitle::StartTime(0), TimeMode::Exact),
            Number(0)
        );
        assert_eq!(
            fr.frame_at_time(subtitle::StartTime(999), TimeMode::Exact),
            Number(0)
        );
        assert_eq!(
            fr.frame_at_time(subtitle::StartTime(1000), TimeMode::Exact),
            Number(1)
        );

        assert_eq!(
            fr.frame_at_time(subtitle::StartTime(0), TimeMode::Start),
            Number(0)
        );
        assert_eq!(
            fr.frame_at_time(subtitle::StartTime(-999), TimeMode::Start),
            Number(0)
        );
        assert_eq!(
            fr.frame_at_time(subtitle::StartTime(1), TimeMode::Start),
            Number(1)
        );

        assert_eq!(
            fr.frame_at_time(subtitle::StartTime(1), TimeMode::EndInclusive),
            Number(0)
        );
        assert_eq!(
            fr.frame_at_time(subtitle::StartTime(1000), TimeMode::EndInclusive),
            Number(0)
        );
        assert_eq!(
            fr.frame_at_time(subtitle::StartTime(1001), TimeMode::EndInclusive),
            Number(1)
        );
    }

    #[test]
    fn i64_supports_pathologically_long_video() {
        // Verify correct behavior with extremely long videos
        let big = subtitle::StartTime(i64::from(i32::MAX) + 1_000_000); // ~2.148e9 ms
        let tc = [0_i64, big.0 / 2, big.0];
        let fr = Framerate::from_timecodes_iter(tc.iter().copied()).unwrap();
        // The final timecode must survive storage intact (no i32 truncation).
        assert_eq!(fr.time_at_frame(Number(2), TimeMode::Exact), big);
        // And a lookup at that time returns the final frame.
        assert_eq!(fr.frame_at_time(big, TimeMode::Exact), Number(2));
        assert_eq!(
            fr.frame_at_time(big - subtitle::Duration(1), TimeMode::Exact),
            Number(1)
        );
    }
}
