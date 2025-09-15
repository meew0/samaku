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

impl<T> std::fmt::Debug for Trace<T>
where
    T: std::fmt::Debug,
{
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.inner.fmt(formatter)
    }
}

/// Wrapper type that implements `Clone` but always panics on clone.
///
/// Useful in situations where an enum needs to be cloneable in general, but some variants
/// need to use types that don't (and shouldn't) implement `Clone`. In these cases, you can
/// use `NeverClone<T>` instead of `T` for the variant, which will make it compile, and if
/// that variant is attempted to be cloned regardless, it will panic at runtime.
pub struct NeverClone<T>(pub T);

impl<T> Deref for NeverClone<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T> DerefMut for NeverClone<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.0
    }
}

impl<T> std::fmt::Debug for NeverClone<T>
where
    T: std::fmt::Debug,
{
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

impl<T> Clone for NeverClone<T> {
    fn clone(&self) -> Self {
        panic!("attempted to clone NeverClone instance");
    }
}
