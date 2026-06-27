use crate::media;
use smallvec::SmallVec;
use std::borrow::Cow;
use std::collections::BTreeMap;
use std::rc::Rc;

/// A collection of elements of type `T` that are more or less associated with some video frames.
///
/// This struct is designed to serve as the default type by which NDE nodes communicate values
/// between each other. If a node outputs only one value, it can be represented as `Single`,
/// such that if another node outputs frame-by-frame values using `Fixed`, the former value
/// can be splat over the latter using `frame_zip`.
///
/// Internally, it is represented as an `Rc` of inner data and normally passed around by ownership
/// rather than by reference, in order to be able to reuse allocations in the common case that
/// only one copy of the `FramedTrack` ever exists.
///
/// It also contains a `frame_adapter`, a closure that specifies the way that a potential
/// `Single` value should be adapted to fit a specific frame (e.g. baking events).
#[derive(Clone)]
pub struct FramedTrack<'a, T: Clone> {
    inner: Rc<Inner<T>>,
    frame_adapter: Option<FrameAdapterFn<'a, T>>,
}

impl<T: Clone + std::fmt::Debug> std::fmt::Debug for FramedTrack<'_, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FramedTrack")
            .field("inner", &self.inner)
            .field(
                "frame_adapter",
                &self.frame_adapter.as_ref().map(|_| "<closure>"),
            )
            .finish()
    }
}

#[derive(Debug, Clone)]
enum Inner<T> {
    Single(T),
    Fixed(Fixed<T>),
    Variable(BTreeMap<media::FrameNumber, SmallVec<T, SMALL_VEC_SIZE>>),
}

/// A vec that stores `width` objects for `data.len() / width` frames,
/// starting from `start` (inclusive).
#[derive(Debug, Clone)]
struct Fixed<T> {
    start: media::FrameNumber,
    width: u16,
    data: Vec<T>,
}

type FrameAdapterFn<'a, T> = Rc<dyn Fn(&mut T, media::FrameNumber) + 'a>;

impl<'a, T> FramedTrack<'a, T>
where
    T: Clone,
{
    #[must_use]
    pub fn from_single(value: T) -> Self {
        Self {
            inner: Rc::new(Inner::Single(value)),
            frame_adapter: None,
        }
    }

    #[must_use]
    pub fn from_single_with_adapter<A: Fn(&mut T, media::FrameNumber) + 'a>(
        value: T,
        adapter: A,
    ) -> Self {
        Self {
            inner: Rc::new(Inner::Single(value)),
            frame_adapter: Some(Rc::new(adapter)),
        }
    }

    /// Construct a `Fixed` track directly from flattened per-frame data: `width`
    /// elements per frame, for `data.len() / width` consecutive frames starting
    /// at `start`. Useful for nodes that compute a value for every frame up front
    /// (e.g. a motion-track node emitting per-frame positions).
    ///
    /// # Panics
    /// Panics if `width` is zero or if `data.len()` is not a multiple of `width`.
    #[must_use]
    pub fn from_fixed(start: media::FrameNumber, width: u16, data: Vec<T>) -> Self {
        assert!(width > 0, "`FramedTrack` width must be nonzero");
        assert_eq!(
            data.len() % usize::from(width),
            0,
            "`FramedTrack` data length must be a multiple of its width"
        );
        Self {
            inner: Rc::new(Inner::Fixed(Fixed { start, width, data })),
            frame_adapter: None,
        }
    }

    /// Construct a `Variable` track from per-frame element lists. Frames not
    /// present in the iterator are treated as empty.
    #[must_use]
    pub fn from_variable<I: IntoIterator<Item = (media::FrameNumber, Vec<T>)>>(entries: I) -> Self {
        let map = entries
            .into_iter()
            .map(|(frame, values)| (frame, SmallVec::from_vec(values)))
            .collect();
        Self {
            inner: Rc::new(Inner::Variable(map)),
            frame_adapter: None,
        }
    }

    /// Construct a `Variable` track from per-frame elements. Frames not
    /// present in the iterator are treated as empty.
    #[must_use]
    pub fn from_variable_singles<I: IntoIterator<Item = (media::FrameNumber, T)>>(
        entries: I,
    ) -> Self {
        let map = entries
            .into_iter()
            .map(|(frame, value)| (frame, SmallVec::from_buf([value])))
            .collect();
        Self {
            inner: Rc::new(Inner::Variable(map)),
            frame_adapter: None,
        }
    }

    /// Maps every value of this track with the given function, removing the adapter.
    /// If the return track will be of the same type, use `map_same` instead which
    /// retains the adapter.
    #[must_use]
    pub fn map<U: Clone, F: FnMut(T) -> U>(self, map_fn: F) -> FramedTrack<'a, U> {
        self.map_direct(map_fn, None)
    }

    /// Maps every value of this track with the given function, retaining the adapter.
    #[must_use]
    pub fn map_same<F: FnMut(T) -> T>(mut self, map_fn: F) -> FramedTrack<'a, T> {
        let adapter = self.frame_adapter.take();
        self.map_direct(map_fn, adapter)
    }

    /// Maps every value of this track with the given function, where the returned
    /// track will have a new custom adapter.
    #[must_use]
    pub fn map_adapt<U: Clone, F: FnMut(T) -> U, A: Fn(&mut U, media::FrameNumber) + 'a>(
        self,
        map_fn: F,
        new_adapter: A,
    ) -> FramedTrack<'a, U> {
        self.map_direct(map_fn, Some(Rc::new(new_adapter)))
    }

    fn map_direct<U: Clone, F: FnMut(T) -> U>(
        self,
        mut map_fn: F,
        new_adapter: Option<FrameAdapterFn<'a, U>>,
    ) -> FramedTrack<'a, U> {
        let inner = Rc::unwrap_or_clone(self.inner);

        let new_inner = match inner {
            Inner::Single(value) => Inner::Single(map_fn(value)),
            Inner::Fixed(fixed) => {
                let Fixed { start, width, data } = fixed;
                let new_data = data.into_iter().map(map_fn).collect();
                Inner::Fixed(Fixed {
                    start,
                    width,
                    data: new_data,
                })
            }
            Inner::Variable(map) => Inner::Variable(
                map.into_iter()
                    .map(|(frame, vec)| (frame, vec.into_iter().map(&mut map_fn).collect()))
                    .collect(),
            ),
        };

        FramedTrack {
            inner: Rc::new(new_inner),
            frame_adapter: new_adapter,
        }
    }

    pub fn into_vec<U, F: FnMut(T) -> U>(self, mut map_fn: F) -> Vec<U> {
        let inner = Rc::unwrap_or_clone(self.inner);

        match inner {
            Inner::Single(value) => vec![map_fn(value)],
            Inner::Fixed(fixed) => fixed.data.into_iter().map(map_fn).collect(),
            Inner::Variable(map) => map
                .into_values()
                .flat_map(|values| values.into_iter().map(&mut map_fn).collect::<Vec<U>>())
                .collect(),
        }
    }

    #[must_use]
    pub fn size(&self) -> Size {
        match *self.inner {
            Inner::Single(_) => Size::Single,
            Inner::Fixed(ref fixed) => Size::Fixed {
                frame_count: fixed.total_frames(),
                total: fixed.data.len(),
            },
            Inner::Variable(ref map) => Size::Variable {
                frame_count: map.len(),
                total: map.values().map(SmallVec::len).sum(),
            },
        }
    }
}

/// Represents the imagined “size” of a `FramedTrack` without any values.
pub enum Size {
    Single,
    Fixed { frame_count: usize, total: usize },
    Variable { frame_count: usize, total: usize },
}

impl<T: Clone> Inner<T> {
    #[must_use]
    fn get_frame_adapt(
        &self,
        number: media::FrameNumber,
        adapter: Option<&FrameAdapterFn<T>>,
    ) -> Option<Cow<'_, T>> {
        match *self {
            Inner::Single(ref value) => Some(Cow::Owned(clone_adapt(value, number, adapter))),
            Inner::Fixed(ref fixed) => fixed
                .get_frame(number)
                .map(|element| Cow::Borrowed(element)),
            Inner::Variable(ref map) => map
                .get(&number)
                .and_then(|vec| vec.first())
                .map(|element| Cow::Borrowed(element)),
        }
    }

    #[must_use]
    fn get_frame_adapt_all(
        &self,
        number: media::FrameNumber,
        adapter: Option<&FrameAdapterFn<T>>,
    ) -> Cow<'_, [T]> {
        match *self {
            Inner::Single(ref value) => Cow::Owned(vec![clone_adapt(value, number, adapter)]),
            Inner::Fixed(ref fixed) => Cow::Borrowed(fixed.get_frame_all(number)),
            Inner::Variable(ref map) => map
                .get(&number)
                .map_or(Cow::Owned(vec![]), |vec| Cow::Borrowed(vec.as_slice())),
        }
    }
}

impl<T> Fixed<T> {
    /// Get the first element stored at the given frame.
    /// Primarily intended to be used for `width = 1` objects.
    #[must_use]
    fn get_frame(&self, number: media::FrameNumber) -> Option<&T> {
        let n = self.frame_n(number)?;
        Some(&self.data[n])
    }

    #[must_use]
    fn get_frame_all(&self, number: media::FrameNumber) -> &[T] {
        let Some(n) = self.frame_n(number) else {
            return &[];
        };
        &self.data[n..(n + usize::from(self.width))]
    }

    #[inline]
    #[must_use]
    fn duration(&self) -> media::FrameDelta {
        let num_frames = self.total_frames();
        media::FrameDelta(i32::try_from(num_frames).expect("`Fixed` duration overflow"))
    }

    /// Calculate the index of the first element of the given frame.
    /// Returns `None` if the frame is out of bounds.
    fn frame_n(&self, frame: media::FrameNumber) -> Option<usize> {
        let n = usize::try_from(i32::from(self.width) * (frame - self.start).0).ok()?;
        ((n + usize::from(self.width)) <= self.data.len()).then_some(n)
    }

    fn total_frames(&self) -> usize {
        self.data.len() / usize::from(self.width)
    }
}

// Stack size of the `SmallVec`s used as return values from mapping functions.
const SMALL_VEC_SIZE: usize = 1;

impl<T1> FramedTrack<'_, T1>
where
    T1: Clone + InherentTiming + 'static,
{
    /// Iterates over pairs of `T1` and `T2` given the other `FramedTrack`,
    /// for each frame this `FramedTrack` covers.
    ///
    /// This method iterates over single `T1` values, with the mapping
    /// function also returning single values. It is useful if input items
    /// need to be mapped one by one per-frame, but no items will be
    /// created or removed.
    ///
    /// Uses the inherent timing of this track, if necessary
    /// (e.g. a track containing a single event with timing information).
    ///
    /// # Panics
    /// Panics if the inherent duration of a `Single` value is negative.
    #[must_use]
    pub fn frame_zip<
        T2: Clone,
        U: Clone,
        F: FnMut(media::FrameNumber, T1, Option<Cow<T2>>) -> U,
    >(
        self,
        track2: FramedTrack<T2>,
        map_fn: F,
    ) -> FramedTrack<'static, U> {
        self.frame_zip_inner(track2, map_fn, <T1 as InherentTiming>::timing)
    }

    /// Iterate over pairs of `T1` and `T2` slices per frame, where the output
    /// vector may be of different length than the input vector, but must be
    /// the same for every frame. Use this method over
    /// `frame_zip_sliced_variable_width` if possible.
    #[must_use]
    pub fn frame_zip_sliced_fixed_width<
        T2: Clone,
        U: Clone,
        F: FnMut(
            media::FrameNumber,
            SmallVec<T1, SMALL_VEC_SIZE>,
            Cow<[T2]>,
        ) -> SmallVec<U, SMALL_VEC_SIZE>,
    >(
        self,
        track2: FramedTrack<T2>,
        map_fn: F,
    ) -> FramedTrack<'static, U> {
        self.frame_zip_sliced_fixed_width_inner(track2, map_fn, <T1 as InherentTiming>::timing)
    }

    /// Iterate over pairs of `T1` and `T2` slices per frame, where the output
    /// vector may be of different length than the input vector and may be
    /// of different length between different iterations.
    ///
    /// Try to avoid this method in favor of `frame_zip_sliced_fixed_width`,
    /// if possible.
    #[must_use]
    pub fn frame_zip_sliced_variable_width<
        T2: Clone,
        U: Clone,
        F: FnMut(
            media::FrameNumber,
            SmallVec<T1, SMALL_VEC_SIZE>,
            Cow<[T2]>,
        ) -> SmallVec<U, SMALL_VEC_SIZE>,
    >(
        self,
        track2: FramedTrack<T2>,
        map_fn: F,
    ) -> FramedTrack<'static, U> {
        self.frame_zip_sliced_variable_width_inner(track2, map_fn, <T1 as InherentTiming>::timing)
    }
}

impl<'a, T1> FramedTrack<'a, T1>
where
    T1: Clone + 'static,
{
    /// Iterates over pairs of `T1` and `T2` given the other `FramedTrack`,
    /// but not necessarily over all frames separately. If both tracks are
    /// `Single`, no iteration will be performed.
    #[must_use]
    pub fn generic_zip<T2: Clone + 'static, U: Clone, F: FnMut(T1, Option<Cow<T2>>) -> U>(
        self,
        track2: FramedTrack<'a, T2>,
        map_fn: F,
    ) -> FramedTrack<'a, U> {
        self.generic_zip_inner(track2, map_fn, None)
    }

    #[must_use]
    pub fn generic_zip_same<T2: Clone + 'static, F: FnMut(T1, Option<Cow<T2>>) -> T1>(
        mut self,
        track2: FramedTrack<'a, T2>,
        map_fn: F,
    ) -> FramedTrack<'a, T1> {
        let adapter = self.frame_adapter.take();
        self.generic_zip_inner(track2, map_fn, adapter)
    }

    #[must_use]
    pub fn generic_zip_adapt<
        'b,
        T2: Clone + 'static,
        U: Clone,
        F: FnMut(T1, Option<Cow<T2>>) -> U,
        A: Fn(&mut U, media::FrameNumber) + 'b,
    >(
        self,
        track2: FramedTrack<T2>,
        map_fn: F,
        new_adapter: Option<A>,
    ) -> FramedTrack<'b, U> {
        let frame_adapter_fn = new_adapter.map(|new_adapter_value| {
            let frame_adapter_fn_value: FrameAdapterFn<'b, U> = Rc::new(new_adapter_value);
            frame_adapter_fn_value
        });
        self.generic_zip_inner(track2, map_fn, frame_adapter_fn)
    }

    #[must_use]
    fn generic_zip_inner<'b, T2: Clone + 'static, U: Clone, F: FnMut(T1, Option<Cow<T2>>) -> U>(
        self,
        track2: FramedTrack<T2>,
        mut map_fn: F,
        new_adapter: Option<FrameAdapterFn<'b, U>>,
    ) -> FramedTrack<'b, U> {
        if let &Inner::Single(_) = &*self.inner {
            if let &Inner::Single(_) = &*track2.inner {
                let Inner::Single(value1) = Rc::unwrap_or_clone(self.inner) else {
                    unreachable!("checked to be `Fixed` above");
                };
                let Inner::Single(value2) = Rc::unwrap_or_clone(track2.inner) else {
                    unreachable!("checked to be `Fixed` above");
                };

                let new_value = map_fn(value1, Some(Cow::Owned(value2)));

                FramedTrack {
                    inner: Rc::new(Inner::Single(new_value)),
                    frame_adapter: new_adapter,
                }
            } else {
                // We are `Single`, but the other is not.
                // Use the other as the reference.
                track2.frame_zip_inner(
                    self,
                    |_frame, value2, value1| {
                        // value1 should never be None since track1 is Single.
                        let cow1 = value1.unwrap();
                        map_fn(cow1.into_owned(), Some(Cow::Owned(value2)))
                    },
                    |_| panic!("timing_fn called unexpectedly"),
                )
            }
        } else {
            self.frame_zip_inner(
                track2,
                |_frame, value1, value2| map_fn(value1, value2),
                |_| panic!("timing_fn called unexpectedly"),
            )
        }
    }

    #[must_use]
    fn frame_zip_inner<
        T2: Clone,
        U: Clone,
        F: FnMut(media::FrameNumber, T1, Option<Cow<T2>>) -> U,
    >(
        self,
        track2: FramedTrack<T2>,
        mut map_fn: F,
        timing_fn: TimingFn<T1>,
    ) -> FramedTrack<'static, U> {
        let inner = Rc::unwrap_or_clone(self.inner);

        let new_inner = match inner {
            Inner::Single(value) => {
                let (start, _) = timing_fn(&value);
                let data = Self::frame_zip_t1_single(
                    &value,
                    track2,
                    map_fn,
                    timing_fn,
                    self.frame_adapter.as_ref(),
                );
                Inner::Fixed(Fixed {
                    start,
                    width: 1,
                    data,
                })
            }
            Inner::Fixed(fixed) => {
                let (width, start) = (fixed.width, fixed.start);
                let data = Self::frame_zip_t1_fixed(fixed, track2, map_fn);
                Inner::Fixed(Fixed { start, width, data })
            }
            Inner::Variable(map) => {
                let mapped = map
                    .into_iter()
                    .map(|(frame, vec)| {
                        (
                            frame,
                            vec.into_iter()
                                .map(|element| {
                                    map_fn(
                                        frame,
                                        element,
                                        track2
                                            .inner
                                            .get_frame_adapt(frame, track2.frame_adapter.as_ref()),
                                    )
                                })
                                .collect(),
                        )
                    })
                    .collect();

                Inner::Variable(mapped)
            }
        };

        FramedTrack {
            inner: Rc::new(new_inner),
            frame_adapter: None,
        }
    }

    fn frame_zip_t1_single<
        T2: Clone,
        U: Clone,
        F: FnMut(media::FrameNumber, T1, Option<Cow<T2>>) -> U,
    >(
        value1: &T1,
        track2: FramedTrack<T2>,
        mut map_fn: F,
        timing_fn: TimingFn<T1>,
        t1_adapter: Option<&FrameAdapterFn<'a, T1>>,
    ) -> Vec<U> {
        let (start, duration) = timing_fn(value1);

        let mut result =
            Vec::with_capacity(usize::try_from(duration.0).expect("duration overflow"));

        let FramedTrack {
            inner: track2_inner_rc,
            frame_adapter: track2_adapter,
        } = track2;

        if let &Inner::Fixed(ref fixed2_ref) = &*track2_inner_rc
            && fixed2_ref.start == start
            && fixed2_ref.duration() == duration
        {
            let Inner::Fixed(fixed2) = Rc::unwrap_or_clone(track2_inner_rc) else {
                unreachable!("checked to be `Fixed` above");
            };

            // If the other track is `Fixed` with exactly the same length as we have,
            // we can `into_iter` to get ownership of the values inside
            // without needing to clone anything.
            // This will be a very common case in practice, so it's worth optimizing
            // for this specifically.
            // We step over the width so we get exactly one item from each frame,
            // and keep a running `frame` counter rather than recomputing it per element.
            // TODO: we can make these `Fixed` cases more general,
            // since we just need to return `None` for out-of-range values.
            let mut frame = start;
            for value2 in fixed2.data.into_iter().step_by(usize::from(fixed2.width)) {
                let new_value1 = clone_adapt(value1, frame, t1_adapter);
                result.push(map_fn(frame, new_value1, Some(Cow::Owned(value2))));
                frame += media::FrameDelta(1);
            }
        } else {
            // We cannot make any assumptions. We need to clone elements one by one.
            for frame_n in start.0..(start + duration).0 {
                let frame = media::FrameNumber(frame_n);
                let value2 = track2_inner_rc.get_frame_adapt(frame, track2_adapter.as_ref());
                let new_value1 = clone_adapt(value1, frame, t1_adapter);
                result.push(map_fn(frame, new_value1, value2));
            }
        }

        result
    }

    fn frame_zip_t1_fixed<
        T2: Clone,
        U: Clone,
        F: FnMut(media::FrameNumber, T1, Option<Cow<T2>>) -> U,
    >(
        fixed1: Fixed<T1>,
        track2: FramedTrack<T2>,
        mut map_fn: F,
    ) -> Vec<U> {
        let start = fixed1.start;
        let duration = fixed1.duration();

        let FramedTrack {
            inner: track2_inner_rc,
            frame_adapter: track2_adapter,
        } = track2;

        if fixed1.width == 1
            && let &Inner::Fixed(ref fixed2_ref) = &*track2_inner_rc
            && fixed2_ref.start == start
            && fixed2_ref.duration() == duration
        {
            let Inner::Fixed(fixed2) = Rc::unwrap_or_clone(track2_inner_rc) else {
                unreachable!("checked to be `Fixed` above");
            };

            // Same as `frame_zip_t1_single`: optimize for the case where
            // `track2_inner` is `Fixed` with the same bounds.
            // We need to further check 2 separate sub-cases: where our
            // width is 1 (so we always use the first element of T2)
            // and where our width is the same as T2.
            // In the first case, we use the first element of each frame,
            // as above, with a running `frame` counter (one frame per element).
            let mut data = Vec::with_capacity(fixed1.data.len());
            let mut frame = start;
            let value2_iter = fixed2.data.into_iter().step_by(usize::from(fixed2.width));
            for (value1, value2) in fixed1.data.into_iter().zip(value2_iter) {
                data.push(map_fn(frame, value1, Some(Cow::Owned(value2))));
                frame += media::FrameDelta(1);
            }
            data
        } else if let &Inner::Fixed(ref fixed2_ref) = &*track2_inner_rc
            && fixed1.width == fixed2_ref.width
            && fixed2_ref.start == start
            && fixed2_ref.duration() == duration
        {
            let Inner::Fixed(fixed2) = Rc::unwrap_or_clone(track2_inner_rc) else {
                unreachable!("checked to be `Fixed` above");
            };

            // Sub-case 2: both fixeds have the same width,
            // so we can directly zip them.
            // We use matching elements from each frame, advancing `frame` once
            // every `width` elements instead of dividing per element.
            let width = usize::from(fixed1.width);
            let mut data = Vec::with_capacity(fixed1.data.len());
            let mut frame = start;
            let mut column = 0_usize;
            for (value1, value2) in fixed1.data.into_iter().zip(fixed2.data) {
                data.push(map_fn(frame, value1, Some(Cow::Owned(value2))));
                column += 1;
                if column == width {
                    column = 0;
                    frame += media::FrameDelta(1);
                }
            }
            data
        } else {
            // We cannot make any assumptions. We need to clone elements one by one.
            // In particular, this also occurs if both tracks are Fixed with the same bounds
            // but not matching in width.
            let width = usize::from(fixed1.width);
            let mut data = Vec::with_capacity(fixed1.data.len());
            let mut frame = start;
            let mut column = 0_usize;
            for value1 in fixed1.data {
                let value2 = track2_inner_rc.get_frame_adapt(frame, track2_adapter.as_ref());
                data.push(map_fn(frame, value1, value2));
                column += 1;
                if column == width {
                    column = 0;
                    frame += media::FrameDelta(1);
                }
            }
            data
        }
    }

    /// Iterate over pairs of `T1` and `T2` slices per frame, where the output
    /// vector may be of different length than the input vector, but must be
    /// the same for every frame. Use this method over
    /// `frame_zip_sliced_variable_width` if possible.
    #[must_use]
    fn frame_zip_sliced_fixed_width_inner<
        T2: Clone,
        U: Clone,
        F: FnMut(
            media::FrameNumber,
            SmallVec<T1, SMALL_VEC_SIZE>,
            Cow<[T2]>,
        ) -> SmallVec<U, SMALL_VEC_SIZE>,
    >(
        self,
        track2: FramedTrack<T2>,
        map_fn: F,
        timing_fn: TimingFn<T1>,
    ) -> FramedTrack<'static, U> {
        // Defer to `variable_width` for the `Variable` case since it would be the same.
        if matches!(&*self.inner, &Inner::Variable(_)) {
            return self.frame_zip_sliced_variable_width_inner(track2, map_fn, timing_fn);
        }

        let inner = Rc::unwrap_or_clone(self.inner);

        let new_inner = match inner {
            Inner::Single(value) => {
                let new_fixed = Self::frame_zip_sliced_fixed_width_t1_single(
                    &value,
                    track2,
                    map_fn,
                    timing_fn,
                    self.frame_adapter.as_ref(),
                );
                Inner::Fixed(new_fixed)
            }
            Inner::Fixed(fixed) => {
                let new_fixed = Self::frame_zip_sliced_fixed_width_t1_fixed(fixed, track2, map_fn);
                Inner::Fixed(new_fixed)
            }
            Inner::Variable(_) => {
                unreachable!()
            }
        };

        FramedTrack {
            inner: Rc::new(new_inner),
            frame_adapter: None,
        }
    }

    fn frame_zip_sliced_fixed_width_t1_single<
        T2: Clone,
        U: Clone,
        F: FnMut(
            media::FrameNumber,
            SmallVec<T1, SMALL_VEC_SIZE>,
            Cow<[T2]>,
        ) -> SmallVec<U, SMALL_VEC_SIZE>,
    >(
        value1: &T1,
        track2: FramedTrack<T2>,
        mut map_fn: F,
        timing_fn: TimingFn<T1>,
        t1_adapter: Option<&FrameAdapterFn<'a, T1>>,
    ) -> Fixed<U> {
        let (start, duration) = timing_fn(value1);
        let num_frames = usize::try_from(duration.0).expect("duration overflow");

        let FramedTrack {
            inner: track2_inner_rc,
            frame_adapter: track2_adapter,
        } = track2;

        let mut result = None;

        if let &Inner::Fixed(ref fixed2_ref) = &*track2_inner_rc
            && fixed2_ref.start == start
            && fixed2_ref.duration() == duration
        {
            let Inner::Fixed(fixed2) = Rc::unwrap_or_clone(track2_inner_rc) else {
                unreachable!("checked to be `Fixed` above");
            };

            // The other track is `Fixed` with exactly our bounds, so we can borrow
            // each frame's elements directly out of its data without cloning them,
            // iterating forward so the output stays in frame order.
            let width2 = usize::from(fixed2.width);
            let mut frame = start;
            for values2 in fixed2.data.chunks(width2) {
                let new_value1 = clone_adapt(value1, frame, t1_adapter);
                let mapped = map_fn(
                    frame,
                    SmallVec::from_buf([new_value1]),
                    Cow::Borrowed(values2),
                );
                Self::append_fixed(&mut result, mapped, start, num_frames);
                frame += media::FrameDelta(1);
            }
        } else {
            // We cannot make any assumptions. We need to clone elements one by one.
            for frame_n in start.0..(start + duration).0 {
                let frame = media::FrameNumber(frame_n);
                let value2 = track2_inner_rc.get_frame_adapt_all(frame, track2_adapter.as_ref());
                let new_value1 = clone_adapt(value1, frame, t1_adapter);
                let mapped = map_fn(frame, SmallVec::from_buf([new_value1]), value2);
                Self::append_fixed(&mut result, mapped, start, num_frames);
            }
        }

        result.unwrap_or(Fixed {
            start,
            width: 1,
            data: vec![],
        })
    }

    fn frame_zip_sliced_fixed_width_t1_fixed<
        T2: Clone,
        U: Clone,
        F: FnMut(
            media::FrameNumber,
            SmallVec<T1, SMALL_VEC_SIZE>,
            Cow<[T2]>,
        ) -> SmallVec<U, SMALL_VEC_SIZE>,
    >(
        fixed1: Fixed<T1>,
        track2: FramedTrack<T2>,
        mut map_fn: F,
    ) -> Fixed<U> {
        let start = fixed1.start;
        let duration = fixed1.duration();
        let total_frames = fixed1.total_frames();
        let width1 = usize::from(fixed1.width);

        let FramedTrack {
            inner: track2_inner_rc,
            frame_adapter: track2_adapter,
        } = track2;

        let mut result = None;

        // Consume our own elements forward, `width1` at a time per frame, so the
        // output stays in frame order.
        let mut data1 = fixed1.data.into_iter();

        if let &Inner::Fixed(ref fixed2_ref) = &*track2_inner_rc
            && fixed2_ref.start == start
            && fixed2_ref.duration() == duration
        {
            let Inner::Fixed(fixed2) = Rc::unwrap_or_clone(track2_inner_rc) else {
                unreachable!("checked to be `Fixed` above");
            };

            // The other track is `Fixed` with exactly our bounds, so we can borrow
            // its per-frame elements directly without cloning.
            let width2 = usize::from(fixed2.width);
            let mut frame = start;
            for values2 in fixed2.data.chunks(width2) {
                let values1: SmallVec<T1, SMALL_VEC_SIZE> = data1.by_ref().take(width1).collect();
                let mapped = map_fn(frame, values1, Cow::Borrowed(values2));
                Self::append_fixed(&mut result, mapped, start, total_frames);
                frame += media::FrameDelta(1);
            }
        } else {
            // We cannot make any assumptions. We need to clone elements one by one.
            let track2_inner = Rc::unwrap_or_clone(track2_inner_rc);

            for frame_index in 0..duration.0 {
                let frame = start + media::FrameDelta(frame_index);
                let values1: SmallVec<T1, SMALL_VEC_SIZE> = data1.by_ref().take(width1).collect();
                let values2 = track2_inner.get_frame_adapt_all(frame, track2_adapter.as_ref());
                let mapped = map_fn(frame, values1, values2);
                Self::append_fixed(&mut result, mapped, start, total_frames);
            }
        }

        result.unwrap_or(Fixed {
            start,
            width: 1,
            data: vec![],
        })
    }

    fn append_fixed<U: Clone>(
        target_opt: &mut Option<Fixed<U>>,
        to_append: SmallVec<U, SMALL_VEC_SIZE>,
        start: media::FrameNumber,
        total_frames: usize,
    ) {
        if let &mut Some(ref mut target) = target_opt {
            // Extend the already appropriately reserved target with the given data,
            // ensuring that it matches the capacity.
            let width = usize::from(target.width);
            assert_eq!(
                to_append.len(),
                width,
                "inconsistent widths in `frame_zip_sliced_fixed_width`"
            );
            target.data.extend(to_append);
        } else {
            // The target is not yet reserved, so do that and initialize it with the first frame.
            let width = to_append.len();
            let mut data = Vec::with_capacity(total_frames * width);
            data.extend(to_append);
            *target_opt = Some(Fixed {
                start,
                width: u16::try_from(width).expect("width overflow"),
                data,
            });
        }
    }

    /// Iterate over pairs of `T1` and `T2` slices per frame, where the output
    /// vector may be of different length than the input vector and may be
    /// of different length between different iterations.
    ///
    /// Try to avoid this method in favor of `frame_zip_sliced_fixed_width`,
    /// if possible.
    #[must_use]
    fn frame_zip_sliced_variable_width_inner<
        T2: Clone,
        U: Clone,
        F: FnMut(
            media::FrameNumber,
            SmallVec<T1, SMALL_VEC_SIZE>,
            Cow<[T2]>,
        ) -> SmallVec<U, SMALL_VEC_SIZE>,
    >(
        self,
        track2: FramedTrack<T2>,
        mut map_fn: F,
        timing_fn: TimingFn<T1>,
    ) -> FramedTrack<'static, U> {
        let inner = Rc::unwrap_or_clone(self.inner);

        let FramedTrack {
            inner: track2_inner_rc,
            frame_adapter: track2_adapter,
        } = track2;

        let new_inner = match inner {
            Inner::Single(value1) => {
                let (start, duration) = timing_fn(&value1);
                let mut variable_map = BTreeMap::new();

                for frame_n in start.0..(start + duration).0 {
                    let frame = media::FrameNumber(frame_n);
                    let value2 =
                        track2_inner_rc.get_frame_adapt_all(frame, track2_adapter.as_ref());
                    let new_value1 = clone_adapt(&value1, frame, self.frame_adapter.as_ref());
                    let mapped = map_fn(frame, SmallVec::from_buf([new_value1]), value2);
                    variable_map.insert(frame, mapped);
                }

                Inner::Variable(variable_map)
            }
            Inner::Fixed(fixed1) => {
                let start = fixed1.start;
                let duration = fixed1.duration();
                let width1 = usize::from(fixed1.width);
                let track2_inner = Rc::unwrap_or_clone(track2_inner_rc);
                let mut variable_map = BTreeMap::new();

                // Move our elements out of `data` `width1` at a time, forward,
                // without cloning them and without a per-frame heap allocation
                // (a width-1 frame stays inline in the `SmallVec`).
                let mut data1 = fixed1.data.into_iter();

                for frame_index in 0..duration.0 {
                    let frame = start + media::FrameDelta(frame_index);
                    let values1: SmallVec<T1, SMALL_VEC_SIZE> =
                        data1.by_ref().take(width1).collect();
                    let values2 = track2_inner.get_frame_adapt_all(frame, track2_adapter.as_ref());
                    let mapped = map_fn(frame, values1, values2);
                    variable_map.insert(frame, mapped);
                }

                Inner::Variable(variable_map)
            }
            Inner::Variable(map) => {
                let track2_inner = Rc::unwrap_or_clone(track2_inner_rc);
                let mapped = map
                    .into_iter()
                    .map(|(frame, elements)| {
                        let small_vec: SmallVec<U, SMALL_VEC_SIZE> = map_fn(
                            frame,
                            elements,
                            track2_inner.get_frame_adapt_all(frame, track2_adapter.as_ref()),
                        );
                        (frame, small_vec)
                    })
                    .collect();
                Inner::Variable(mapped)
            }
        };

        FramedTrack {
            inner: Rc::new(new_inner),
            frame_adapter: None,
        }
    }
}

fn clone_adapt<T: Clone>(
    value: &T,
    frame: media::FrameNumber,
    adapter: Option<&FrameAdapterFn<T>>,
) -> T {
    let mut cloned = value.clone();
    if let Some(frame_adapter) = adapter {
        frame_adapter(&mut cloned, frame);
    }
    cloned
}

type TimingFn<T> = fn(&T) -> (media::FrameNumber, media::FrameDelta);

pub trait InherentTiming {
    fn timing(&self) -> (media::FrameNumber, media::FrameDelta);
}

impl InherentTiming for super::Event {
    fn timing(&self) -> (media::FrameNumber, media::FrameDelta) {
        (self.start, self.duration)
    }
}

#[cfg(test)]
mod tests {
    use super::{Fixed, FramedTrack, InherentTiming, Inner, SMALL_VEC_SIZE};
    use crate::media;
    use smallvec::SmallVec;
    use std::borrow::Cow;
    use std::collections::BTreeMap;
    use std::rc::Rc;

    /// A test value that carries an integer `tag` payload plus its own timing.
    /// Stands in for `nde::Event`, which is the value most commonly used as the
    /// first track (`T1`) in real NDE nodes.
    #[derive(Debug, Clone, PartialEq, Eq)]
    struct Timed {
        start: i32,
        dur: i32,
        tag: i32,
    }

    impl InherentTiming for Timed {
        fn timing(&self) -> (media::FrameNumber, media::FrameDelta) {
            (media::FrameNumber(self.start), media::FrameDelta(self.dur))
        }
    }

    fn fr(n: i32) -> media::FrameNumber {
        media::FrameNumber(n)
    }

    /// A `Timed` whose timing is irrelevant (used for `Fixed`/`Variable` tracks,
    /// where the inherent timing is never consulted).
    fn timed(tag: i32) -> Timed {
        Timed {
            start: 0,
            dur: 0,
            tag,
        }
    }

    fn fixed_track<T: Clone>(start: i32, width: u16, data: Vec<T>) -> FramedTrack<'static, T> {
        FramedTrack {
            inner: Rc::new(Inner::Fixed(Fixed {
                start: fr(start),
                width,
                data,
            })),
            frame_adapter: None,
        }
    }

    fn variable_track<T: Clone>(entries: Vec<(i32, Vec<T>)>) -> FramedTrack<'static, T> {
        let map = entries
            .into_iter()
            .map(|(frame, values)| (fr(frame), SmallVec::from_vec(values)))
            .collect();
        FramedTrack {
            inner: Rc::new(Inner::Variable(map)),
            frame_adapter: None,
        }
    }

    fn unwrap_fixed<T: Clone>(track: FramedTrack<T>) -> Fixed<T> {
        match Rc::unwrap_or_clone(track.inner) {
            Inner::Fixed(fixed) => fixed,
            _ => panic!("expected a `Fixed` track"),
        }
    }

    fn unwrap_variable<T: Clone>(
        track: FramedTrack<T>,
    ) -> BTreeMap<media::FrameNumber, SmallVec<T, SMALL_VEC_SIZE>> {
        match Rc::unwrap_or_clone(track.inner) {
            Inner::Variable(map) => map,
            _ => panic!("expected a `Variable` track"),
        }
    }

    fn unwrap_single<T: Clone>(track: FramedTrack<T>) -> T {
        match Rc::unwrap_or_clone(track.inner) {
            Inner::Single(value) => value,
            _ => panic!("expected a `Single` track"),
        }
    }

    /// Combine `Timed` (T1) with an optional `i32` (T2) into a `(frame, tag, t2)`
    /// triple, encoding an absent T2 as `-1`. Non-capturing, hence `Copy`, so it
    /// can be reused across multiple `frame_zip` calls.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "must take `T1` by value to match the `frame_zip` map signature"
    )]
    fn combine(frame: media::FrameNumber, v1: Timed, v2: Option<Cow<i32>>) -> (i32, i32, i32) {
        (frame.0, v1.tag, v2.map_or(-1, |cow| *cow))
    }

    #[test]
    fn public_constructors() {
        // `from_fixed` builds a `Fixed` track that behaves like one produced by a
        // `frame_zip`.
        let track = FramedTrack::from_fixed(fr(2), 2, vec![1, 2, 3, 4]);
        let fixed = unwrap_fixed(track);
        assert_eq!(fixed.start, fr(2));
        assert_eq!(fixed.width, 2);
        assert_eq!(fixed.data, vec![1, 2, 3, 4]);

        // `from_variable` keys per-frame element lists by frame.
        let track = FramedTrack::from_variable(vec![(fr(0), vec![10]), (fr(2), vec![20, 21])]);
        let map = unwrap_variable(track);
        assert_eq!(map.len(), 2);
        assert_eq!(map[&fr(0)].as_slice(), &[10]);
        assert_eq!(map[&fr(2)].as_slice(), &[20, 21]);
    }

    #[test]
    #[should_panic(expected = "width must be nonzero")]
    fn from_fixed_rejects_zero_width() {
        std::hint::black_box(FramedTrack::from_fixed(fr(0), 0, vec![1, 2, 3]));
    }

    #[test]
    #[should_panic(expected = "multiple of its width")]
    fn from_fixed_rejects_ragged_data() {
        std::hint::black_box(FramedTrack::from_fixed(fr(0), 2, vec![1, 2, 3]));
    }

    #[test]
    fn fixed_indexing() {
        // width 1, covering frames 5, 6, 7.
        let single_wide = Fixed {
            start: fr(5),
            width: 1,
            data: vec![10, 20, 30],
        };
        assert_eq!(single_wide.frame_n(fr(5)), Some(0));
        assert_eq!(single_wide.frame_n(fr(6)), Some(1));
        // The final frame must be reachable.
        assert_eq!(single_wide.frame_n(fr(7)), Some(2));
        // Out of range on both ends.
        assert_eq!(single_wide.frame_n(fr(8)), None);
        assert_eq!(single_wide.frame_n(fr(4)), None);

        assert_eq!(single_wide.get_frame(fr(5)), Some(&10));
        assert_eq!(single_wide.get_frame(fr(7)), Some(&30));
        assert_eq!(single_wide.get_frame(fr(8)), None);
        assert_eq!(single_wide.get_frame_all(fr(6)), &[20]);
        assert!(single_wide.get_frame_all(fr(8)).is_empty());
        assert_eq!(single_wide.duration(), media::FrameDelta(3));
        assert_eq!(single_wide.total_frames(), 3);

        // width 2, covering frames 2, 3.
        let double_wide = Fixed {
            start: fr(2),
            width: 2,
            data: vec![1, 2, 3, 4],
        };
        assert_eq!(double_wide.frame_n(fr(2)), Some(0));
        assert_eq!(double_wide.frame_n(fr(3)), Some(2));
        assert_eq!(double_wide.frame_n(fr(4)), None);
        assert_eq!(double_wide.get_frame_all(fr(2)), &[1, 2]);
        assert_eq!(double_wide.get_frame_all(fr(3)), &[3, 4]);
        // `get_frame` returns the first element of the frame.
        assert_eq!(double_wide.get_frame(fr(3)), Some(&3));
        assert_eq!(double_wide.duration(), media::FrameDelta(2));
        assert_eq!(double_wide.total_frames(), 2);
    }

    #[test]
    fn frame_zip_single_t1() {
        // A `Single` T1 spanning frames 5, 6, 7.
        let make_t1 = || {
            FramedTrack::from_single(Timed {
                start: 5,
                dur: 3,
                tag: 100,
            })
        };

        // (a) `track2` is a `Fixed` with exactly matching bounds: the fast path.
        let track2 = fixed_track(5, 1, vec![10, 20, 30]);
        let out = unwrap_fixed(make_t1().frame_zip(track2, combine));
        assert_eq!(out.start, fr(5));
        assert_eq!(out.width, 1);
        assert_eq!(out.data, vec![(5, 100, 10), (6, 100, 20), (7, 100, 30)]);

        // (b) `track2` is a `Fixed` covering a wider span: the slow path, which
        // indexes into `track2` frame by frame.
        let track2 = fixed_track(4, 1, vec![1, 10, 20, 30, 40]); // frames 4..=8
        let out = unwrap_fixed(make_t1().frame_zip(track2, combine));
        assert_eq!(out.data, vec![(5, 100, 10), (6, 100, 20), (7, 100, 30)]);

        // (c) `track2` is `Single`: splat the single value over every frame.
        let track2 = FramedTrack::from_single(7);
        let out = unwrap_fixed(make_t1().frame_zip(track2, combine));
        assert_eq!(out.data, vec![(5, 100, 7), (6, 100, 7), (7, 100, 7)]);

        // (d) `track2` is `Variable` with a gap at frame 6: that frame gets `None`.
        let track2 = variable_track(vec![(5, vec![10]), (7, vec![30])]);
        let out = unwrap_fixed(make_t1().frame_zip(track2, combine));
        assert_eq!(out.data, vec![(5, 100, 10), (6, 100, -1), (7, 100, 30)]);

        // (e) `track2` shares our start but is shorter: matching start, mismatched
        // duration, so the slow path runs and frame 7 falls off the end.
        let track2 = fixed_track(5, 1, vec![10, 20]); // frames 5, 6 only
        let out = unwrap_fixed(make_t1().frame_zip(track2, combine));
        assert_eq!(out.data, vec![(5, 100, 10), (6, 100, 20), (7, 100, -1)]);
    }

    #[test]
    fn frame_zip_fixed_t1() {
        // (a) width-1 T1 with a matching width-1 T2: the width-1 fast sub-case.
        let t1 = fixed_track(0, 1, vec![timed(1), timed(2), timed(3)]);
        let t2 = fixed_track(0, 1, vec![10, 20, 30]);
        let out = unwrap_fixed(t1.frame_zip(t2, combine));
        assert_eq!(out.width, 1);
        assert_eq!(out.start, fr(0));
        assert_eq!(out.data, vec![(0, 1, 10), (1, 2, 20), (2, 3, 30)]);

        // (b) width-2 T1 with a matching width-2 T2: zipped element by element,
        // preserving the width.
        let t1 = fixed_track(0, 2, vec![timed(1), timed(2), timed(3), timed(4)]);
        let t2 = fixed_track(0, 2, vec![10, 20, 30, 40]);
        let out = unwrap_fixed(t1.frame_zip(t2, combine));
        assert_eq!(out.width, 2);
        assert_eq!(
            out.data,
            vec![(0, 1, 10), (0, 2, 20), (1, 3, 30), (1, 4, 40)]
        );

        // (c) width-2 T1 with a width-1 T2 (matching bounds): the slow path, where
        // every T1 element of a frame is paired with that frame's first T2 value.
        let t1 = fixed_track(0, 2, vec![timed(1), timed(2), timed(3), timed(4)]);
        let t2 = fixed_track(0, 1, vec![10, 20]);
        let out = unwrap_fixed(t1.frame_zip(t2, combine));
        assert_eq!(out.width, 2);
        assert_eq!(
            out.data,
            vec![(0, 1, 10), (0, 2, 10), (1, 3, 20), (1, 4, 20)]
        );

        // (d) width-1 T1 with a `Single` T2: the slow path splatting the single.
        let t1 = fixed_track(0, 1, vec![timed(1), timed(2), timed(3)]);
        let t2 = FramedTrack::from_single(99);
        let out = unwrap_fixed(t1.frame_zip(t2, combine));
        assert_eq!(out.data, vec![(0, 1, 99), (1, 2, 99), (2, 3, 99)]);
    }

    #[test]
    fn frame_zip_variable_t1() {
        // Frame 1 is absent; frame 2 carries two elements.
        let t1 = variable_track(vec![(0, vec![timed(1)]), (2, vec![timed(2), timed(3)])]);
        let t2 = fixed_track(0, 1, vec![10, 20, 30]);
        let out = unwrap_variable(t1.frame_zip(t2, combine));
        assert_eq!(out.len(), 2);
        assert_eq!(out[&fr(0)].as_slice(), &[(0, 1, 10)]);
        assert_eq!(out[&fr(2)].as_slice(), &[(2, 2, 30), (2, 3, 30)]);
    }

    #[test]
    fn frame_zip_sliced_fixed_width() {
        // map_fn that emits one output per frame: (frame, sum).
        let sum_one = |frame: media::FrameNumber,
                       v1: SmallVec<Timed, SMALL_VEC_SIZE>,
                       v2: Cow<[i32]>|
         -> SmallVec<(i32, i32), SMALL_VEC_SIZE> {
            let mut out = SmallVec::new();
            out.push((frame.0, v1[0].tag + v2.iter().sum::<i32>()));
            out
        };

        // (1) `Single` T1, matching `Fixed` T2, width-1 output: fast branch.
        let t1 = || {
            FramedTrack::from_single(Timed {
                start: 0,
                dur: 3,
                tag: 1000,
            })
        };
        let out = unwrap_fixed(
            t1().frame_zip_sliced_fixed_width(fixed_track(0, 1, vec![10, 20, 30]), sum_one),
        );
        assert_eq!(out.start, fr(0));
        assert_eq!(out.width, 1);
        assert_eq!(out.data, vec![(0, 1010), (1, 1020), (2, 1030)]);

        // (2) `Single` T1, matching `Fixed` T2, width-2 output: fast branch,
        // emitting two outputs per frame.
        let sum_two = |frame: media::FrameNumber,
                       v1: SmallVec<Timed, SMALL_VEC_SIZE>,
                       v2: Cow<[i32]>|
         -> SmallVec<(i32, i32), SMALL_VEC_SIZE> {
            let base = v1[0].tag + v2.iter().sum::<i32>();
            let mut out = SmallVec::new();
            out.push((frame.0, base));
            out.push((frame.0, -base));
            out
        };
        let out = unwrap_fixed(
            t1().frame_zip_sliced_fixed_width(fixed_track(0, 1, vec![10, 20, 30]), sum_two),
        );
        assert_eq!(out.width, 2);
        assert_eq!(
            out.data,
            vec![
                (0, 1010),
                (0, -1010),
                (1, 1020),
                (1, -1020),
                (2, 1030),
                (2, -1030)
            ]
        );

        // (3) `Single` T1, `Single` T2, width-2 output: the slow branch must still
        // report a width of 2.
        let out =
            unwrap_fixed(t1().frame_zip_sliced_fixed_width(FramedTrack::from_single(5), sum_two));
        assert_eq!(out.width, 2);
        assert_eq!(
            out.data,
            vec![
                (0, 1005),
                (0, -1005),
                (1, 1005),
                (1, -1005),
                (2, 1005),
                (2, -1005)
            ]
        );

        // (4) `Fixed` T1, matching `Fixed` T2: fast branch, order preserved.
        let t1f = fixed_track(0, 1, vec![timed(1), timed(2), timed(3)]);
        let out = unwrap_fixed(
            t1f.frame_zip_sliced_fixed_width(fixed_track(0, 1, vec![10, 20, 30]), sum_one),
        );
        assert_eq!(out.data, vec![(0, 11), (1, 22), (2, 33)]);

        // (5) `Fixed` T1, `Single` T2: slow branch, order preserved.
        let t1f = fixed_track(0, 1, vec![timed(1), timed(2), timed(3)]);
        let out =
            unwrap_fixed(t1f.frame_zip_sliced_fixed_width(FramedTrack::from_single(100), sum_one));
        assert_eq!(out.data, vec![(0, 101), (1, 102), (2, 103)]);
    }

    #[test]
    fn frame_zip_sliced_variable_width() {
        let zip_all = |frame: media::FrameNumber,
                       v1: SmallVec<Timed, SMALL_VEC_SIZE>,
                       v2: Cow<[i32]>|
         -> SmallVec<(i32, i32), SMALL_VEC_SIZE> {
            v2.iter()
                .map(|value2| (frame.0, v1[0].tag + value2))
                .collect()
        };

        // (1) `Single` T1, `Variable` T2 with an empty frame in the middle.
        let t1 = FramedTrack::from_single(Timed {
            start: 0,
            dur: 3,
            tag: 1,
        });
        let t2 = variable_track(vec![(0, vec![10, 11]), (2, vec![30])]);
        let out = unwrap_variable(t1.frame_zip_sliced_variable_width(t2, zip_all));
        assert_eq!(out[&fr(0)].as_slice(), &[(0, 11), (0, 12)]);
        assert!(out[&fr(1)].is_empty());
        assert_eq!(out[&fr(2)].as_slice(), &[(2, 31)]);

        // (2) `Fixed` T1 with a matching `Fixed` T2.
        let t1f = fixed_track(0, 1, vec![timed(1), timed(2)]);
        let t2 = fixed_track(0, 1, vec![10, 20]);
        let out = unwrap_variable(t1f.frame_zip_sliced_variable_width(t2, zip_all));
        assert_eq!(out[&fr(0)].as_slice(), &[(0, 11)]);
        assert_eq!(out[&fr(1)].as_slice(), &[(1, 22)]);

        // (3) `Variable` T1 with a `Variable` T2 whose per-frame width varies, so
        // the output width genuinely differs between frames.
        let t1v = variable_track(vec![(0, vec![timed(1)]), (1, vec![timed(5)])]);
        let t2 = variable_track(vec![(0, vec![10, 100]), (1, vec![20])]);
        let out = unwrap_variable(t1v.frame_zip_sliced_variable_width(t2, zip_all));
        assert_eq!(out[&fr(0)].as_slice(), &[(0, 11), (0, 101)]);
        assert_eq!(out[&fr(1)].as_slice(), &[(1, 25)]);
    }

    #[test]
    fn adapters_are_applied() {
        // The frame adapter rewrites the tag to the current frame number.
        let adapter = |value: &mut Timed, frame: media::FrameNumber| value.tag = frame.0;

        // Fast path (matching `Fixed` T2).
        let t1 = FramedTrack::from_single_with_adapter(
            Timed {
                start: 0,
                dur: 3,
                tag: 0,
            },
            adapter,
        );
        let out = unwrap_fixed(t1.frame_zip(fixed_track(0, 1, vec![100, 100, 100]), combine));
        assert_eq!(out.data, vec![(0, 0, 100), (1, 1, 100), (2, 2, 100)]);

        // Slow path (`Single` T2).
        let t1 = FramedTrack::from_single_with_adapter(
            Timed {
                start: 0,
                dur: 3,
                tag: 0,
            },
            adapter,
        );
        let out = unwrap_fixed(t1.frame_zip(FramedTrack::from_single(100), combine));
        assert_eq!(out.data, vec![(0, 0, 100), (1, 1, 100), (2, 2, 100)]);

        // `map` transforms values and `Single`/`Fixed`/`Variable` shapes are kept.
        let single = unwrap_single(FramedTrack::from_single(timed(3)).map(|mut value| {
            value.tag *= 10;
            value
        }));
        assert_eq!(single.tag, 30);

        let mapped_fixed = unwrap_fixed(fixed_track(0, 1, vec![1, 2, 3]).map(|value| value + 100));
        assert_eq!(mapped_fixed.data, vec![101, 102, 103]);

        let mapped_variable = unwrap_variable(
            variable_track(vec![(0, vec![1]), (1, vec![2, 3])]).map(|value| value + 100),
        );
        assert_eq!(mapped_variable[&fr(0)].as_slice(), &[101]);
        assert_eq!(mapped_variable[&fr(1)].as_slice(), &[102, 103]);
    }

    #[test]
    fn event_as_t1() {
        use crate::nde::tags;
        use crate::subtitle;

        let make_event = |start: i32, dur: i32| super::super::Event {
            start: fr(start),
            duration: media::FrameDelta(dur),
            layer_index: 0,
            style_index: 0,
            margins: subtitle::Margins::default(),
            global_tags: tags::Global::empty(),
            overrides: tags::Local::empty(),
            text: vec![],
        };

        // A single event (T1) splat over per-frame x-positions (T2), as a node
        // baking positions onto a line would do.
        let event_track = FramedTrack::from_single(make_event(10, 3));
        let positions = fixed_track(10, 1, vec![1, 2, 3]);
        let baked = event_track.frame_zip(positions, |frame, event, x| {
            (frame.0, event.start.0, x.map_or(0, |cow| *cow))
        });
        let out = unwrap_fixed(baked);
        assert_eq!(out.start, fr(10));
        assert_eq!(out.width, 1);
        assert_eq!(out.data, vec![(10, 10, 1), (11, 10, 2), (12, 10, 3)]);

        // A frame adapter that "bakes" the event to a single frame, like
        // `make_static`, must be applied per frame before mapping.
        let event_track = FramedTrack::from_single_with_adapter(
            make_event(10, 3),
            |event: &mut super::super::Event, frame: media::FrameNumber| {
                *event = event.make_static(frame, media::FrameDelta(1));
            },
        );
        let positions = fixed_track(10, 1, vec![0, 0, 0]);
        let baked = event_track.frame_zip(positions, |frame, event, _| {
            (frame.0, event.start.0, event.duration.0)
        });
        let out = unwrap_fixed(baked);
        assert_eq!(out.data, vec![(10, 10, 1), (11, 11, 1), (12, 12, 1)]);
    }
}
