use std::ops::{Add, AddAssign, Sub, SubAssign};

pub mod playback;
pub mod reticule;

/// Identifies a video frame by number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FrameNumber(pub i32);

/// A difference in counted video frames.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FrameDelta(pub i32);

impl Add<FrameDelta> for FrameNumber {
    type Output = FrameNumber;

    fn add(self, rhs: FrameDelta) -> Self::Output {
        FrameNumber(self.0 + rhs.0)
    }
}

impl AddAssign<FrameDelta> for FrameNumber {
    fn add_assign(&mut self, rhs: FrameDelta) {
        self.0 += rhs.0;
    }
}

impl Sub<FrameDelta> for FrameNumber {
    type Output = FrameNumber;

    fn sub(self, rhs: FrameDelta) -> Self::Output {
        FrameNumber(self.0 - rhs.0)
    }
}

impl SubAssign<FrameDelta> for FrameNumber {
    fn sub_assign(&mut self, rhs: FrameDelta) {
        self.0 -= rhs.0;
    }
}
