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
pub struct FramedTrack<'a, T: Clone> {
    inner: Rc<Inner<T>>,
    frame_adapter: Option<FrameAdapterFn<'a, T>>,
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

type FrameAdapterFn<'a, T> = Box<dyn Fn(&mut T, media::FrameNumber) + 'a>;

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
            frame_adapter: Some(Box::new(adapter)),
        }
    }

    #[must_use]
    pub fn map<U: Clone, F: FnMut(T) -> U>(self, map_fn: F) -> FramedTrack<'a, U> {
        self.map_direct(map_fn, None)
    }

    #[must_use]
    pub fn map_with_new_adapter<
        U: Clone,
        F: FnMut(T) -> U,
        A: Fn(&mut U, media::FrameNumber) + 'a,
    >(
        self,
        map_fn: F,
        new_adapter: A,
    ) -> FramedTrack<'a, U> {
        self.map_direct(map_fn, Some(Box::new(new_adapter)))
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
        let num_frames = self.data.len() / usize::from(self.width);
        media::FrameDelta(i32::try_from(num_frames).expect("`Fixed` duration overflow"))
    }

    fn split_off(&mut self, after: media::FrameNumber) -> Vec<T> {
        let n = self.frame_n(after).expect("frame out of bounds");
        self.data.split_off(n)
    }

    /// Calculate the index of the first element of the given frame.
    /// Returns `None` if the frame is out of bounds.
    fn frame_n(&self, frame: media::FrameNumber) -> Option<usize> {
        let n =
            usize::try_from(self.start.0 + i32::from(self.width) * (frame - self.start).0).ok()?;
        ((n + usize::from(self.width)) < self.data.len()).then_some(n)
    }

    fn total_frames(&self) -> usize {
        self.data.len() / usize::from(self.width)
    }
}

// Stack size of the `SmallVec`s used as return values from mapping functions.
const SMALL_VEC_SIZE: usize = 1;

impl<'a, T1> FramedTrack<'a, T1>
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
        mut map_fn: F,
    ) -> FramedTrack<'a, U> {
        let inner = Rc::unwrap_or_clone(self.inner);

        let new_inner = match inner {
            Inner::Single(value) => {
                let start = value.start();
                let data =
                    Self::frame_zip_t1_single(&value, track2, map_fn, self.frame_adapter.as_ref());
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
        t1_adapter: Option<&FrameAdapterFn<'a, T1>>,
    ) -> Vec<U> {
        let start = value1.start();
        let duration = value1.duration();

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
            if let Inner::Fixed(fixed2) = Rc::unwrap_or_clone(track2_inner_rc) {
                // If the other track is `Fixed` with exactly the same length as we have,
                // we can `into_iter` to get ownership of the values inside
                // without needing to clone anything.
                // This will be a very common case in practice, so it's worth optimizing
                // for this specifically.
                // We need to step over the width though so we get exactly one item
                // from each frame.
                // TODO: we can make these `Fixed` cases more general,
                // since we just need to return `None` for out-of-range values.
                for (i, value2) in fixed2
                    .data
                    .into_iter()
                    .step_by(usize::from(fixed2.width))
                    .enumerate()
                {
                    let frame =
                        start + media::FrameDelta(i32::try_from(i).expect("frame offset overflow"));
                    let new_value1 = clone_adapt(value1, frame, t1_adapter);
                    result.push(map_fn(frame, new_value1, Some(Cow::Owned(value2))));
                }
            } else {
                panic!("unwrap_or_clone failed");
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
            if let Inner::Fixed(fixed2) = Rc::unwrap_or_clone(track2_inner_rc) {
                // Same as `frame_zip_t1_single`: optimize for the case where
                // `track2_inner` is `Fixed` with the same bounds.
                // We need to further check 2 separate sub-cases: where our
                // width is 1 (so we always use the first element of T2)
                // and where our width is the same as T2.
                // In the first case, we use the first element of each frame,
                // as above.
                fixed1
                    .data
                    .into_iter()
                    .enumerate()
                    .zip(fixed2.data.into_iter().step_by(usize::from(fixed2.width)))
                    .map(|((i, value1), value2)| {
                        let frame = start
                            + media::FrameDelta(i32::try_from(i).expect("frame offset overflow"));
                        map_fn(frame, value1, Some(Cow::Owned(value2)))
                    })
                    .collect()
            } else {
                panic!("unwrap_or_clone failed");
            }
        } else if let &Inner::Fixed(ref fixed2_ref) = &*track2_inner_rc
            && fixed1.width == fixed2_ref.width
            && fixed2_ref.start == start
            && fixed2_ref.duration() == duration
        {
            if let Inner::Fixed(fixed2) = Rc::unwrap_or_clone(track2_inner_rc) {
                // Sub-case 2: both fixeds have the same width,
                // so we can directly zip them.
                // We use matching elements from each frame.
                fixed1
                    .data
                    .into_iter()
                    .enumerate()
                    .zip(fixed2.data)
                    .map(|((i, value1), value2)| {
                        let frame_i = i / usize::from(fixed1.width);
                        let frame = start
                            + media::FrameDelta(
                                i32::try_from(frame_i).expect("frame offset overflow"),
                            );
                        map_fn(frame, value1, Some(Cow::Owned(value2)))
                    })
                    .collect()
            } else {
                panic!("unwrap_or_clone failed");
            }
        } else {
            // We cannot make any assumptions. We need to clone elements one by one.
            // In particular, this also occurs if both tracks are Fixed with the same bounds
            // but not matching in width.
            fixed1
                .data
                .into_iter()
                .enumerate()
                .map(|(i, value1)| {
                    let frame_i = i / usize::from(fixed1.width);
                    let frame = start
                        + media::FrameDelta(i32::try_from(frame_i).expect("frame offset overflow"));
                    let value2 = track2_inner_rc.get_frame_adapt(frame, track2_adapter.as_ref());
                    map_fn(frame, value1, value2)
                })
                .collect()
        }
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
    ) -> FramedTrack<'a, U> {
        // Defer to `variable_width` for the `Variable` case since it would be the same.
        if matches!(&*self.inner, &Inner::Variable(_)) {
            return self.frame_zip_sliced_variable_width(track2, map_fn);
        }

        let inner = Rc::unwrap_or_clone(self.inner);

        let new_inner = match inner {
            Inner::Single(value) => {
                let new_fixed = Self::frame_zip_sliced_fixed_width_t1_single(
                    &value,
                    track2,
                    map_fn,
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
        t1_adapter: Option<&FrameAdapterFn<'a, T1>>,
    ) -> Fixed<U> {
        let start = value1.start();
        let duration = value1.duration();
        let num_frames = usize::try_from(duration.0).expect("duration overflow");

        let FramedTrack {
            inner: track2_inner_rc,
            frame_adapter: track2_adapter,
        } = track2;

        if let &Inner::Fixed(ref fixed2_ref) = &*track2_inner_rc
            && fixed2_ref.start == start
            && fixed2_ref.duration() == duration
        {
            if let Inner::Fixed(mut fixed2) = Rc::unwrap_or_clone(track2_inner_rc) {
                let mut result = None;

                for frame_index in (0..duration.0).rev() {
                    let frame_offset = media::FrameDelta(frame_index);
                    let frame = start + frame_offset;
                    let new_value1 = clone_adapt(value1, frame, t1_adapter);
                    let values2 = fixed2.split_off(frame);
                    let mapped =
                        map_fn(frame, SmallVec::from_buf([new_value1]), Cow::Owned(values2));
                    Self::append_fixed(&mut result, mapped, start, fixed2.total_frames());
                }

                result.unwrap_or(Fixed {
                    start,
                    width: 1,
                    data: vec![],
                })
            } else {
                panic!("unwrap_or_clone failed");
            }
        } else {
            // We cannot make any assumptions. We need to clone elements one by one.
            let mut inner_result = Vec::with_capacity(num_frames);

            for frame_n in start.0..(start + duration).0 {
                let frame = media::FrameNumber(frame_n);
                let value2 = track2_inner_rc.get_frame_adapt_all(frame, track2_adapter.as_ref());
                let new_value1 = clone_adapt(value1, frame, t1_adapter);
                let mapped = map_fn(frame, SmallVec::from_buf([new_value1]), value2);
                inner_result.extend(mapped);
            }

            Fixed {
                start,
                width: 1,
                data: inner_result,
            }
        }
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
        mut fixed1: Fixed<T1>,
        track2: FramedTrack<T2>,
        mut map_fn: F,
    ) -> Fixed<U> {
        let start = fixed1.start;
        let duration = fixed1.duration();
        let total_frames = fixed1.total_frames();

        let FramedTrack {
            inner: track2_inner_rc,
            frame_adapter: track2_adapter,
        } = track2;

        if let &Inner::Fixed(ref fixed2_ref) = &*track2_inner_rc
            && fixed2_ref.start == start
            && fixed2_ref.duration() == duration
        {
            if let Inner::Fixed(mut fixed2) = Rc::unwrap_or_clone(track2_inner_rc) {
                let mut result = None;

                for frame_index in (0..duration.0).rev() {
                    let frame_offset = media::FrameDelta(frame_index);
                    let frame = start + frame_offset;
                    let values1 = fixed1.split_off(frame);
                    let values2 = fixed2.split_off(frame);
                    let mapped = map_fn(frame, SmallVec::from_vec(values1), Cow::Owned(values2));
                    Self::append_fixed(&mut result, mapped, start, fixed1.total_frames());
                }

                result.unwrap_or(Fixed {
                    start,
                    width: 1,
                    data: vec![],
                })
            } else {
                panic!("unwrap_or_clone failed");
            }
        } else {
            // We cannot make any assumptions. We need to clone elements one by one.

            let track2_inner = Rc::unwrap_or_clone(track2_inner_rc);
            let mut result = None;

            for frame_index in (0..duration.0).rev() {
                let frame_offset = media::FrameDelta(frame_index);
                let frame = start + frame_offset;
                let values1 = fixed1.split_off(frame);
                let values2 = track2_inner.get_frame_adapt_all(frame, track2_adapter.as_ref());
                let mapped = map_fn(frame, SmallVec::from_vec(values1), values2);
                Self::append_fixed(&mut result, mapped, start, total_frames);
            }

            result.unwrap_or(Fixed {
                start,
                width: 1,
                data: vec![],
            })
        }
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
        mut map_fn: F,
    ) -> FramedTrack<'a, U> {
        let inner = Rc::unwrap_or_clone(self.inner);

        let FramedTrack {
            inner: track2_inner_rc,
            frame_adapter: track2_adapter,
        } = track2;

        let new_inner = match inner {
            Inner::Single(value1) => {
                let start = value1.start();
                let duration = value1.duration();
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
            Inner::Fixed(mut fixed1) => {
                let start = fixed1.start;
                let duration = fixed1.duration();
                let track2_inner = Rc::unwrap_or_clone(track2_inner_rc);
                let mut variable_map = BTreeMap::new();

                for frame_index in (0..duration.0).rev() {
                    let frame_offset = media::FrameDelta(frame_index);
                    let frame = start + frame_offset;
                    let values1 = fixed1.split_off(frame);
                    let values2 = track2_inner.get_frame_adapt_all(frame, track2_adapter.as_ref());
                    let mapped = map_fn(frame, SmallVec::from_vec(values1), values2);
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

pub trait InherentTiming {
    fn start(&self) -> media::FrameNumber;
    fn duration(&self) -> media::FrameDelta;
}

impl InherentTiming for super::Event {
    fn start(&self) -> media::FrameNumber {
        self.start
    }

    fn duration(&self) -> media::FrameDelta {
        self.duration
    }
}
