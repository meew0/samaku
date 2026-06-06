use std::ops::{Add, AddAssign, Sub, SubAssign};

use crate::subtitle;

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

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Rate {
    pub numerator: u64,
    pub denominator: u64,
}

impl Rate {
    pub const F24: Rate = Rate {
        numerator: 24,
        denominator: 1,
    };

    pub const F23_976: Rate = Rate {
        numerator: 24000,
        denominator: 1001,
    };

    /// Get the number of the closest frame before the given time point in milliseconds.
    ///
    /// # Panics
    /// Panics if the resulting frame number would not fit into an `i32`.
    #[must_use]
    pub(crate) fn ms_to_frame(&self, ass_ms: i64) -> Number {
        #[expect(
            clippy::cast_possible_wrap,
            reason = "numerator is guaranteed to be smaller than i64 max"
        )]
        let numerator = ass_ms * self.numerator as i64;
        #[expect(
            clippy::cast_possible_wrap,
            reason = "denominator is guaranteed to be smaller than i64 max"
        )]
        let denominator = 1000 * self.denominator as i64;
        Number(
            (numerator / denominator)
                .try_into()
                .expect("overflow while converting time to frame number"),
        )
    }

    /// Get the number of the closest frame *after* the given time point in milliseconds.
    ///
    /// # Panics
    /// Panics if the resulting frame number would not fit into an `i32`.
    #[must_use]
    pub(crate) fn ms_to_frame_after(&self, ass_ms: i64) -> Number {
        #[expect(
            clippy::cast_possible_wrap,
            reason = "denominator is guaranteed to be smaller than i64 max"
        )]
        let denominator = 1000 * self.denominator as i64;
        #[expect(
            clippy::cast_possible_wrap,
            reason = "numerator is guaranteed to be smaller than i64 max"
        )]
        let numerator = (ass_ms * self.numerator as i64) + denominator - 1;
        Number(
            (numerator / denominator)
                .try_into()
                .expect("overflow while converting time to frame number"),
        )
    }

    #[must_use]
    pub(crate) fn frame_to_ms(&self, frame: Number) -> i64 {
        #[expect(
            clippy::cast_possible_wrap,
            reason = "denominator is guaranteed to be smaller than i64 max"
        )]
        let inv_numerator = i64::from(frame.0 * 1000) * self.denominator as i64;
        #[expect(
            clippy::cast_possible_wrap,
            reason = "numerator is guaranteed to be smaller than i64 max"
        )]
        let result = inv_numerator / self.numerator as i64;
        result
    }

    pub(crate) fn ass_time_to_frame(&self, ass_time: subtitle::StartTime) -> Number {
        self.ms_to_frame(ass_time.0)
    }

    pub(crate) fn ass_time_to_frame_after(&self, ass_time: subtitle::StartTime) -> Number {
        self.ms_to_frame_after(ass_time.0)
    }

    pub(crate) fn frame_to_ass_time(&self, frame: Number) -> subtitle::StartTime {
        subtitle::StartTime(self.frame_to_ms(frame))
    }

    pub(crate) fn iter_from(&self, frame: Number) -> impl Iterator<Item = (Number, i64)> {
        FrameIterator {
            frame_rate: self,
            current: frame,
        }
    }
}

impl From<Rate> for f64 {
    /// Convert the frame rate to a floating-point value by dividing the numerator by the
    /// denominator. May lose precision for very large numerators/denominators.
    #[expect(
        clippy::cast_precision_loss,
        reason = "amount of precision loss is acceptable in this case"
    )]
    fn from(value: Rate) -> Self {
        value.numerator as f64 / value.denominator as f64
    }
}

struct FrameIterator<'a> {
    frame_rate: &'a Rate,
    current: Number,
}

impl Iterator for FrameIterator<'_> {
    type Item = (Number, i64);

    fn next(&mut self) -> Option<Self::Item> {
        self.current += Delta(1);
        Some((self.current, self.frame_rate.frame_to_ms(self.current)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_timing_24() {
        let frame_rate = Rate::F24;

        assert_eq!(
            frame_rate.ass_time_to_frame(subtitle::StartTime(0)),
            Number(0)
        );
        assert_eq!(
            frame_rate.ass_time_to_frame_after(subtitle::StartTime(0)),
            Number(0)
        );

        assert_eq!(
            frame_rate.ass_time_to_frame(subtitle::StartTime(1)),
            Number(0)
        );
        assert_eq!(
            frame_rate.ass_time_to_frame_after(subtitle::StartTime(1)),
            Number(1)
        );

        assert_eq!(
            frame_rate.ass_time_to_frame(subtitle::StartTime(999)),
            Number(23)
        );
        assert_eq!(
            frame_rate.ass_time_to_frame_after(subtitle::StartTime(999)),
            Number(24)
        );

        assert_eq!(
            frame_rate.ass_time_to_frame(subtitle::StartTime(1000)),
            Number(24)
        );
        assert_eq!(
            frame_rate.ass_time_to_frame_after(subtitle::StartTime(1000)),
            Number(24)
        );
    }

    #[test]
    fn frame_timing_23_976() {
        let frame_rate = Rate::F23_976;

        assert_eq!(
            frame_rate.ass_time_to_frame(subtitle::StartTime(0)),
            Number(0)
        );
        assert_eq!(
            frame_rate.ass_time_to_frame_after(subtitle::StartTime(0)),
            Number(0)
        );

        assert_eq!(
            frame_rate.ass_time_to_frame(subtitle::StartTime(1)),
            Number(0)
        );
        assert_eq!(
            frame_rate.ass_time_to_frame_after(subtitle::StartTime(1)),
            Number(1)
        );

        assert_eq!(
            frame_rate.ass_time_to_frame(subtitle::StartTime(1000)),
            Number(23)
        );
        assert_eq!(
            frame_rate.ass_time_to_frame_after(subtitle::StartTime(1000)),
            Number(24)
        );
    }
}
