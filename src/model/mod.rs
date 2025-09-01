use std::ops::{Add, AddAssign, Deref, DerefMut, Sub, SubAssign};

pub mod playback;
pub mod reticule;

/// Identifies a video frame by number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct FrameNumber(pub i32);

/// A difference in counted video frames.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
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

/// A wrapper around an arbitrary object that tracks whenever that object might have been modified
/// (by being mutably borrowed).
///
/// We use this to track changes to global state that needs to be reflected in the state of specific
/// iced/iced_aw widgets.
pub struct Trace<T> {
    trace: bool,
    inner: T,
}

impl<T> Trace<T> {
    /// Create a new `Trace`. Note that new traces are considered dirty by default; the first
    /// call to [`check`] will return `true`.
    pub fn new(inner: T) -> Self {
        Self { trace: true, inner }
    }

    /// Checks whether the inner value may have been modified since the last time `check` was
    /// called.
    #[must_use]
    pub fn check(&mut self) -> bool {
        let trace = self.trace;
        self.trace = false;
        trace
    }
}

impl<T> Deref for Trace<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.inner
    }
}

impl<T> DerefMut for Trace<T> {
    fn deref_mut(&mut self) -> &mut T {
        self.trace = true;
        &mut self.inner
    }
}
