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

/// The internal representation of a [`FramedTrack`]'s values. Each variant trades
/// off differently between memory layout and the kind of per-frame data it can
/// express; the mapping and zipping operations pick the cheapest path for the
/// variants involved (for instance *moving* values out of a uniquely-owned `Fixed`
/// rather than cloning them).
#[derive(Debug, Clone)]
enum Inner<T> {
    /// A single frame-agnostic value, conceptually present on every frame. It is
    /// splatted over a frame range on demand, adapted per frame via the track's
    /// `frame_adapter` if one is set.
    Single(T),

    /// A contiguous block of per-frame values; see [`Fixed`].
    Fixed(Fixed<T>),

    /// Sparse per-frame value lists keyed by frame. Frames absent from the map are
    /// treated as empty. Used when the number of values differs from frame to frame.
    Variable(BTreeMap<media::FrameNumber, SmallVec<T, SMALL_VEC_SIZE>>),

    /// A non-empty stack of frame-agnostic values that all share the same inherent
    /// timing. Behaves like `Single`, except that splatting it over a frame range
    /// produces a `Fixed` of width `n` (the stack size) rather than width 1, and
    /// flattening it yields all `n` values. The shared-timing invariant cannot be
    /// statically enforced (`InherentTiming` is not always available), so timing is
    /// taken from the first element.
    Stack(Vec<T>),
}

/// A vec that stores `width` objects for `data.len() / width` frames,
/// starting from `start` (inclusive).
#[derive(Debug, Clone)]
struct Fixed<T> {
    start: media::FrameNumber,
    width: u16,
    data: Vec<T>,
}

/// A closure that adapts a value in place to a specific frame, for example by
/// baking a `Single` event onto the frame it is being materialized for. Stored
/// behind an `Rc` so that cloning a `FramedTrack` stays cheap.
type FrameAdapterFn<'a, T> = Rc<dyn Fn(&mut T, media::FrameNumber) + 'a>;

impl<'a, T> FramedTrack<'a, T>
where
    T: Clone,
{
    /// Construct a `Single` track: one frame-agnostic value, conceptually present
    /// on every frame, that is splatted over a frame range when zipped against a
    /// frame-based track.
    #[must_use]
    pub fn from_single(value: T) -> Self {
        Self {
            inner: Rc::new(Inner::Single(value)),
            frame_adapter: None,
        }
    }

    /// Like `from_single`, but with a frame adapter that is applied to the value
    /// each time it is materialized for a specific frame (e.g. baking an event onto
    /// that frame). Operations that keep the value frame-agnostic, such as
    /// `map_same`, retain the adapter.
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

    /// Construct a `Stack` track from a non-empty list of frame-agnostic values
    /// that all share the same inherent timing. Useful for nodes that turn one
    /// value into several covering the same frame range (e.g. a gradient node
    /// splitting one event into several strips).
    ///
    /// # Panics
    /// Panics if `values` is empty.
    #[must_use]
    pub fn from_stack(values: Vec<T>) -> Self {
        assert!(!values.is_empty(), "`FramedTrack` stack must be non-empty");
        Self {
            inner: Rc::new(Inner::Stack(values)),
            frame_adapter: None,
        }
    }

    /// Like `from_stack`, but with a frame adapter that is applied to each value
    /// when it is materialized for a specific frame.
    ///
    /// # Panics
    /// Panics if `values` is empty.
    #[must_use]
    pub fn from_stack_with_adapter<A: Fn(&mut T, media::FrameNumber) + 'a>(
        values: Vec<T>,
        adapter: A,
    ) -> Self {
        assert!(!values.is_empty(), "`FramedTrack` stack must be non-empty");
        Self {
            inner: Rc::new(Inner::Stack(values)),
            frame_adapter: Some(Rc::new(adapter)),
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
            Inner::Stack(values) => Inner::Stack(values.into_iter().map(map_fn).collect()),
        };

        FramedTrack {
            inner: Rc::new(new_inner),
            frame_adapter: new_adapter,
        }
    }

    /// Maps every value of this track with a function that returns a vector,
    /// expanding each input value into possibly several output values, removing
    /// the adapter.
    ///
    /// The shape of the track changes accordingly:
    /// - a `Single` becomes a `Stack` holding the returned vector;
    /// - a `Stack` becomes a larger `Stack` of all returned values concatenated;
    /// - a `Fixed` becomes a wider `Fixed`, its width multiplied by the per-element
    ///   output length (which must be identical for every element);
    /// - a `Variable` stays `Variable`, with each frame's values flat-mapped.
    ///
    /// If the return track will be of the same type, use `expand_same` instead
    /// which retains the adapter.
    #[must_use]
    pub fn expand<U: Clone, F: FnMut(T) -> Vec<U>>(self, map_fn: F) -> FramedTrack<'a, U> {
        self.expand_direct(map_fn, None)
    }

    /// Like `expand`, but retaining the adapter.
    #[must_use]
    pub fn expand_same<F: FnMut(T) -> Vec<T>>(mut self, map_fn: F) -> FramedTrack<'a, T> {
        let adapter = self.frame_adapter.take();
        self.expand_direct(map_fn, adapter)
    }

    /// Like `expand`, but where the returned track will have a new custom adapter.
    #[must_use]
    pub fn expand_adapt<U: Clone, F: FnMut(T) -> Vec<U>, A: Fn(&mut U, media::FrameNumber) + 'a>(
        self,
        map_fn: F,
        new_adapter: A,
    ) -> FramedTrack<'a, U> {
        self.expand_direct(map_fn, Some(Rc::new(new_adapter)))
    }

    fn expand_direct<U: Clone, F: FnMut(T) -> Vec<U>>(
        self,
        mut map_fn: F,
        new_adapter: Option<FrameAdapterFn<'a, U>>,
    ) -> FramedTrack<'a, U> {
        let inner = Rc::unwrap_or_clone(self.inner);

        let new_inner = match inner {
            Inner::Single(value) => {
                let values = map_fn(value);
                assert!(
                    !values.is_empty(),
                    "`expand` map function must return a non-empty vec"
                );
                Inner::Stack(values)
            }
            Inner::Stack(values) => {
                let mapped: Vec<U> = values.into_iter().flat_map(&mut map_fn).collect();
                assert!(
                    !mapped.is_empty(),
                    "`expand` map function must return a non-empty result"
                );
                Inner::Stack(mapped)
            }
            Inner::Fixed(fixed) => {
                let Fixed { start, width, data } = fixed;
                if data.is_empty() {
                    Inner::Fixed(Fixed {
                        start,
                        width,
                        data: Vec::new(),
                    })
                } else {
                    // Map the first element to learn the per-element output length `k`,
                    // then require every other element to match it so the result is a
                    // well-formed `Fixed` of width `width * k`. The frame-major layout
                    // is preserved because we expand elements in order.
                    let old_len = data.len();
                    let mut iter = data.into_iter();
                    let first = map_fn(iter.next().expect("data is non-empty"));
                    let per_element = first.len();
                    assert!(
                        per_element > 0,
                        "`expand` map function must return a non-empty vec"
                    );
                    let mut new_data = Vec::with_capacity(old_len * per_element);
                    new_data.extend(first);
                    for element in iter {
                        let mapped = map_fn(element);
                        assert_eq!(
                            mapped.len(),
                            per_element,
                            "`expand` map function must return vecs of equal length for a `Fixed` track"
                        );
                        new_data.extend(mapped);
                    }
                    let new_width = u16::try_from(usize::from(width) * per_element)
                        .expect("`expand` width overflow");
                    Inner::Fixed(Fixed {
                        start,
                        width: new_width,
                        data: new_data,
                    })
                }
            }
            Inner::Variable(map) => {
                let mapped = map
                    .into_iter()
                    .map(|(frame, vec)| {
                        let new_vec: SmallVec<U, SMALL_VEC_SIZE> =
                            vec.into_iter().flat_map(&mut map_fn).collect();
                        (frame, new_vec)
                    })
                    .collect();
                Inner::Variable(mapped)
            }
        };

        FramedTrack {
            inner: Rc::new(new_inner),
            frame_adapter: new_adapter,
        }
    }

    /// Flatten the whole track into a single `Vec`, mapping every value with
    /// `map_fn`. This is the terminal operation an output node uses to collect all
    /// values regardless of representation: a `Single` yields one element, a `Fixed`
    /// or `Variable` yields each per-frame element in frame order, and a `Stack`
    /// yields all of its values. Reuses the track's allocation when it is uniquely
    /// owned.
    pub fn into_vec<U, F: FnMut(T) -> U>(self, mut map_fn: F) -> Vec<U> {
        let inner = Rc::unwrap_or_clone(self.inner);

        match inner {
            Inner::Single(value) => vec![map_fn(value)],
            Inner::Fixed(fixed) => fixed.data.into_iter().map(map_fn).collect(),
            Inner::Variable(map) => map
                .into_values()
                .flat_map(|values| values.into_iter().map(&mut map_fn).collect::<Vec<U>>())
                .collect(),
            Inner::Stack(values) => values.into_iter().map(map_fn).collect(),
        }
    }

    /// Describe the track's shape and element count without inspecting the values
    /// themselves. Used, for example, to display socket contents in the node editor.
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
            Inner::Stack(ref values) => Size::Stack {
                total: values.len(),
            },
        }
    }
}

/// Represents the imagined “size” of a `FramedTrack` without any values.
pub enum Size {
    /// A `Single` value (always exactly one element).
    Single,
    /// A `Fixed` track: `total` elements spread evenly over `frame_count` frames.
    Fixed { frame_count: usize, total: usize },
    /// A `Variable` track: `total` elements over `frame_count` populated frames.
    Variable { frame_count: usize, total: usize },
    /// A `Stack`: `total` frame-agnostic values.
    Stack { total: usize },
}

impl<T: Clone> Inner<T> {
    /// Get the representative value for `number`: the single value a width-1
    /// consumer expects from this track at that frame. `Single`/`Stack` are
    /// frame-agnostic, so they always yield a value (cloned and adapted, the stack
    /// using its first element); `Fixed`/`Variable` borrow that frame's first
    /// element from storage, or return `None` if the frame is not covered.
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
            // A stack is frame-agnostic like a `Single`, so its first element
            // represents the frame.
            Inner::Stack(ref values) => Some(Cow::Owned(clone_adapt(&values[0], number, adapter))),
        }
    }

    /// Get every value present at `number` as a slice. `Single`/`Stack` are
    /// frame-agnostic, so their value(s) appear on every frame (cloned and adapted);
    /// `Fixed`/`Variable` borrow that frame's elements from storage, or return an
    /// empty slice if the frame is not covered.
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
            // All of a stack's values appear on every frame, each adapted to it.
            Inner::Stack(ref values) => Cow::Owned(
                values
                    .iter()
                    .map(|value| clone_adapt(value, number, adapter))
                    .collect(),
            ),
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

    /// Get all `width` elements stored at the given frame as a slice, or an empty
    /// slice if the frame is out of bounds.
    #[must_use]
    fn get_frame_all(&self, number: media::FrameNumber) -> &[T] {
        let Some(n) = self.frame_n(number) else {
            return &[];
        };
        &self.data[n..(n + usize::from(self.width))]
    }

    /// The number of frames this `Fixed` covers, as a `FrameDelta`.
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

    /// The number of frames this `Fixed` covers.
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
    /// The result is shaped according to this track's representation: a `Single` or
    /// `Stack` is splatted over the frame range given by its inherent timing,
    /// producing a `Fixed` of width 1 or `n` (the stack size) respectively, while a
    /// `Fixed`/`Variable` is mapped frame by frame in place.
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
    /// Combine this track with `track2` value by value, without necessarily
    /// iterating over every frame. Unlike `frame_zip`, this does not require
    /// `T1: InherentTiming`: when both tracks are frame-agnostic (`Single`/`Stack`)
    /// the result is too and no frame iteration happens; otherwise the frame-based
    /// track drives the iteration. The map function receives each `T1` value
    /// together with the matching `T2` value for its frame, or `None` if `track2`
    /// does not cover that frame.
    ///
    /// This variant drops any frame adapter; use `generic_zip_same` to retain it or
    /// `generic_zip_adapt` to install a new one.
    #[must_use]
    pub fn generic_zip<T2: Clone + 'static, U: Clone, F: FnMut(T1, Option<Cow<T2>>) -> U>(
        self,
        track2: FramedTrack<'a, T2>,
        map_fn: F,
    ) -> FramedTrack<'a, U> {
        self.generic_zip_inner(track2, map_fn, None)
    }

    /// Like `generic_zip`, but retaining this track's frame adapter so the result
    /// can still be adapted per frame. The map function must return `T1`, since the
    /// adapter is typed for `T1`.
    #[must_use]
    pub fn generic_zip_same<T2: Clone + 'static, F: FnMut(T1, Option<Cow<T2>>) -> T1>(
        self,
        track2: FramedTrack<'a, T2>,
        map_fn: F,
    ) -> FramedTrack<'a, T1> {
        let adapter = self.frame_adapter.clone(); // this clone is cheap since the adapter is an `Rc`
        self.generic_zip_inner(track2, map_fn, adapter)
    }

    /// Like `generic_zip`, but installing `new_adapter` as the result's frame
    /// adapter (when given). Only meaningful if the result stays frame-agnostic.
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
        // `Single` and `Stack` are both frame-agnostic: they carry no frame
        // information of their own and (in `generic_zip`, which lacks
        // `InherentTiming`) cannot supply a frame range. When both sides are
        // frame-agnostic we combine them directly; otherwise the frame-based side
        // drives the iteration.
        let self_agnostic = matches!(&*self.inner, Inner::Single(_) | Inner::Stack(_));
        let track2_agnostic = matches!(&*track2.inner, Inner::Single(_) | Inner::Stack(_));

        if self_agnostic && track2_agnostic {
            let new_inner = match (
                Rc::unwrap_or_clone(self.inner),
                Rc::unwrap_or_clone(track2.inner),
            ) {
                (Inner::Single(value1), Inner::Single(value2)) => {
                    Inner::Single(map_fn(value1, Some(Cow::Owned(value2))))
                }
                (Inner::Single(value1), Inner::Stack(values2)) => {
                    // The single value is paired with each of the stack's values.
                    Inner::Stack(
                        values2
                            .into_iter()
                            .map(|value2| map_fn(value1.clone(), Some(Cow::Owned(value2))))
                            .collect(),
                    )
                }
                (Inner::Stack(values1), Inner::Single(value2)) => {
                    // Each of our stack's values is paired with the single value.
                    Inner::Stack(
                        values1
                            .into_iter()
                            .map(|value1| map_fn(value1, Some(Cow::Borrowed(&value2))))
                            .collect(),
                    )
                }
                (Inner::Stack(values1), Inner::Stack(values2)) => {
                    // Two frameless stacks have no canonical alignment, so this is
                    // best-effort: each of our values is paired with the other
                    // stack's first (representative) value.
                    let representative = &values2[0];
                    Inner::Stack(
                        values1
                            .into_iter()
                            .map(|value1| map_fn(value1, Some(Cow::Borrowed(representative))))
                            .collect(),
                    )
                }
                _ => unreachable!("both sides checked to be `Single` or `Stack`"),
            };

            FramedTrack {
                inner: Rc::new(new_inner),
                frame_adapter: new_adapter,
            }
        } else if self_agnostic {
            // We are frame-agnostic but `track2` is not, so `track2` drives the frames.
            if matches!(&*self.inner, Inner::Stack(_)) {
                // Each frame carries all `n` of our stack values, so the output is
                // `n` wide. We use `track2`'s representative value per frame.
                track2.frame_zip_sliced_fixed_width_inner(
                    self,
                    |_frame, repr2: SmallVec<T2, SMALL_VEC_SIZE>, stack1: Cow<[T1]>| {
                        stack1
                            .iter()
                            .map(|value1| map_fn(value1.clone(), repr2.first().map(Cow::Borrowed)))
                            .collect()
                    },
                    |_| panic!("timing_fn called unexpectedly"),
                )
            } else {
                // We are `Single`: use `track2` as the reference and splat our value.
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
            Inner::Stack(values) => {
                // Splatting a stack of `n` values over a frame range yields a
                // `Fixed` of width `n`: every frame carries all `n` values.
                let (start, _) = timing_fn(&values[0]);
                let width = u16::try_from(values.len()).expect("stack width overflow");
                let data = Self::frame_zip_t1_stack(
                    &values,
                    track2,
                    map_fn,
                    timing_fn,
                    self.frame_adapter.as_ref(),
                );
                Inner::Fixed(Fixed { start, width, data })
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

    fn frame_zip_t1_stack<
        T2: Clone,
        U: Clone,
        F: FnMut(media::FrameNumber, T1, Option<Cow<T2>>) -> U,
    >(
        values1: &[T1],
        track2: FramedTrack<T2>,
        mut map_fn: F,
        timing_fn: TimingFn<T1>,
        t1_adapter: Option<&FrameAdapterFn<'a, T1>>,
    ) -> Vec<U> {
        let (start, duration) = timing_fn(&values1[0]);
        let n = values1.len();

        let mut result =
            Vec::with_capacity(n * usize::try_from(duration.0).expect("duration overflow"));

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

            // Fast path: like `frame_zip_t1_single`, the other track is `Fixed` with
            // exactly our bounds. We move out one representative `T2` value per frame
            // (stepping over its width) and pair it with each of our `n` stack values.
            // The representative is only borrowed (not cloned) for each of the `n` calls.
            let mut frame = start;
            for value2 in fixed2.data.into_iter().step_by(usize::from(fixed2.width)) {
                for value1 in values1 {
                    let new_value1 = clone_adapt(value1, frame, t1_adapter);
                    result.push(map_fn(frame, new_value1, Some(Cow::Borrowed(&value2))));
                }
                frame += media::FrameDelta(1);
            }
        } else {
            // We cannot make any assumptions. We fetch each frame's representative
            // value once and reuse it (by reference) for all `n` stack values.
            for frame_n in start.0..(start + duration).0 {
                let frame = media::FrameNumber(frame_n);
                let value2 = track2_inner_rc.get_frame_adapt(frame, track2_adapter.as_ref());
                let value2_ref = value2.as_deref();
                for value1 in values1 {
                    let new_value1 = clone_adapt(value1, frame, t1_adapter);
                    result.push(map_fn(frame, new_value1, value2_ref.map(Cow::Borrowed)));
                }
            }
        }

        result
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
            Inner::Stack(values) => {
                let new_fixed = Self::frame_zip_sliced_fixed_width_t1_stack(
                    &values,
                    track2,
                    map_fn,
                    timing_fn,
                    self.frame_adapter.as_ref(),
                );
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

    fn frame_zip_sliced_fixed_width_t1_stack<
        T2: Clone,
        U: Clone,
        F: FnMut(
            media::FrameNumber,
            SmallVec<T1, SMALL_VEC_SIZE>,
            Cow<[T2]>,
        ) -> SmallVec<U, SMALL_VEC_SIZE>,
    >(
        values1: &[T1],
        track2: FramedTrack<T2>,
        mut map_fn: F,
        timing_fn: TimingFn<T1>,
        t1_adapter: Option<&FrameAdapterFn<'a, T1>>,
    ) -> Fixed<U> {
        let (start, duration) = timing_fn(&values1[0]);
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
            // each frame's elements directly. Our `n` stack values appear on every
            // frame, each adapted to it.
            let width2 = usize::from(fixed2.width);
            let mut frame = start;
            for values2 in fixed2.data.chunks(width2) {
                let values1_adapted: SmallVec<T1, SMALL_VEC_SIZE> = values1
                    .iter()
                    .map(|value1| clone_adapt(value1, frame, t1_adapter))
                    .collect();
                let mapped = map_fn(frame, values1_adapted, Cow::Borrowed(values2));
                Self::append_fixed(&mut result, mapped, start, num_frames);
                frame += media::FrameDelta(1);
            }
        } else {
            // We cannot make any assumptions. We clone our stack values per frame.
            for frame_n in start.0..(start + duration).0 {
                let frame = media::FrameNumber(frame_n);
                let values2 = track2_inner_rc.get_frame_adapt_all(frame, track2_adapter.as_ref());
                let values1_adapted: SmallVec<T1, SMALL_VEC_SIZE> = values1
                    .iter()
                    .map(|value1| clone_adapt(value1, frame, t1_adapter))
                    .collect();
                let mapped = map_fn(frame, values1_adapted, values2);
                Self::append_fixed(&mut result, mapped, start, num_frames);
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
            Inner::Stack(values1) => {
                let (start, duration) = timing_fn(&values1[0]);
                let mut variable_map = BTreeMap::new();

                // Our `n` stack values appear on every frame, each adapted to it.
                for frame_n in start.0..(start + duration).0 {
                    let frame = media::FrameNumber(frame_n);
                    let values2 =
                        track2_inner_rc.get_frame_adapt_all(frame, track2_adapter.as_ref());
                    let values1_adapted: SmallVec<T1, SMALL_VEC_SIZE> = values1
                        .iter()
                        .map(|value1| clone_adapt(value1, frame, self.frame_adapter.as_ref()))
                        .collect();
                    let mapped = map_fn(frame, values1_adapted, values2);
                    variable_map.insert(frame, mapped);
                }

                Inner::Variable(variable_map)
            }
        };

        FramedTrack {
            inner: Rc::new(new_inner),
            frame_adapter: None,
        }
    }
}

impl<T> FramedTrack<'_, Option<T>>
where
    T: Clone + 'static,
{
    /// Flatten a `FramedTrack` of `Option`s into a new track of the inner type
    /// that only contains the `Some` values.
    ///
    /// Will return `None` for a single or a stack that would be empty,
    /// but will return empty variable maps/fixed tracks.
    pub fn flatten<'b>(self) -> Option<FramedTrack<'b, T>> {
        let inner = Rc::unwrap_or_clone(self.inner);

        let new_inner = match inner {
            Inner::Single(single_opt) => Inner::Single(single_opt?),
            Inner::Fixed(fixed) => {
                let Fixed { start, width, data } = fixed;
                if data.iter().all(Option::is_some) {
                    let new_data = data.into_iter().map(Option::unwrap).collect();
                    Inner::Fixed(Fixed {
                        start,
                        width,
                        data: new_data,
                    })
                } else {
                    let mut map = BTreeMap::new();
                    for (i, element_opt) in data.into_iter().enumerate() {
                        if let Some(element) = element_opt {
                            let frame_i = i / usize::from(width);
                            #[expect(
                                clippy::cast_possible_wrap,
                                reason = "frame number should fit in i32"
                            )]
                            #[expect(
                                clippy::cast_possible_truncation,
                                reason = "frame number should fit in i32"
                            )]
                            let frame = start + media::FrameDelta(frame_i as i32);
                            map.entry(frame).or_insert_with(SmallVec::new).push(element);
                        }
                    }

                    Inner::Variable(map)
                }
            }
            Inner::Variable(map) => Inner::Variable(
                map.into_iter()
                    .filter_map(|(frame, vec)| {
                        let new_vec: SmallVec<T, 1> = vec.into_iter().flatten().collect();

                        (!new_vec.is_empty()).then_some((frame, new_vec))
                    })
                    .collect(),
            ),
            Inner::Stack(stack) => {
                let new_stack: Vec<T> = stack.into_iter().flatten().collect();
                if new_stack.is_empty() {
                    return None;
                }

                Inner::Stack(new_stack)
            }
        };

        Some(FramedTrack {
            inner: Rc::new(new_inner),
            frame_adapter: None,
        })
    }
}

/// Clone `value` and, if an adapter is given, adapt the clone to `frame`. This is
/// how frame-agnostic values (`Single`/`Stack`) are materialized for a specific
/// frame, e.g. baking an event onto it.
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

/// A function extracting the inherent `(start, duration)` timing of a value. The
/// zip routines are shared between the `InherentTiming`-based public methods
/// (which pass [`InherentTiming::timing`]) and the `generic_zip` family (which pass
/// a panicking stub and rely on the frame-based track to drive iteration instead).
type TimingFn<T> = fn(&T) -> (media::FrameNumber, media::FrameDelta);

/// Implemented by values that carry their own frame range, such as `nde::Event`. A
/// `Single` or `Stack` track of such a value can be splatted over that range by
/// `frame_zip` without a separate frame-based track to follow.
pub trait InherentTiming {
    /// Return this value's `(start_frame, duration)`.
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
    use assert_matches2::assert_matches;
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

    fn unwrap_stack<T: Clone>(track: FramedTrack<T>) -> Vec<T> {
        match Rc::unwrap_or_clone(track.inner) {
            Inner::Stack(values) => values,
            _ => panic!("expected a `Stack` track"),
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

    #[test]
    fn stack_construction_and_flattening() {
        // `from_stack` builds a `Stack`; `unwrap_stack` recovers its values.
        let values = unwrap_stack(FramedTrack::from_stack(vec![timed(1), timed(2), timed(3)]));
        assert_eq!(values, vec![timed(1), timed(2), timed(3)]);

        // `into_vec` emits every stack value (mapped), like an output node would.
        let flat = FramedTrack::from_stack(vec![10, 20, 30]).into_vec(|value| value + 1);
        assert_eq!(flat, vec![11, 21, 31]);

        // `size` reports the stack's total length.
        assert!(matches!(
            FramedTrack::from_stack(vec![1, 2, 3, 4]).size(),
            super::Size::Stack { total: 4 }
        ));

        // `map` preserves the `Stack` shape.
        let mapped = unwrap_stack(FramedTrack::from_stack(vec![1, 2, 3]).map(|value| value * 10));
        assert_eq!(mapped, vec![10, 20, 30]);
    }

    #[test]
    #[should_panic(expected = "non-empty")]
    fn from_stack_rejects_empty() {
        std::hint::black_box(FramedTrack::<i32>::from_stack(vec![]));
    }

    #[test]
    fn expand_shapes() {
        // `Single` -> `Stack`: the returned vec becomes the stack.
        let stack = unwrap_stack(
            FramedTrack::from_single(10).expand(|value| vec![value, value + 1, value + 2]),
        );
        assert_eq!(stack, vec![10, 11, 12]);

        // `Fixed` -> wider `Fixed`: width multiplied by the per-element output length.
        let wide =
            unwrap_fixed(fixed_track(2, 1, vec![10, 20, 30]).expand(|value| vec![value, -value]));
        assert_eq!(wide.start, fr(2));
        assert_eq!(wide.width, 2);
        assert_eq!(wide.data, vec![10, -10, 20, -20, 30, -30]);

        // A width-2 `Fixed` expands each element, so the width becomes 2 * 2 = 4,
        // with the frame-major layout preserved.
        let wide2 = unwrap_fixed(
            fixed_track(0, 2, vec![1, 2, 3, 4]).expand(|value| vec![value, value * 10]),
        );
        assert_eq!(wide2.width, 4);
        assert_eq!(wide2.data, vec![1, 10, 2, 20, 3, 30, 4, 40]);

        // `Variable` -> `Variable`: each frame's elements are flat-mapped.
        let var = unwrap_variable(
            variable_track(vec![(0, vec![1]), (1, vec![2, 3])])
                .expand(|value| vec![value, value + 100]),
        );
        assert_eq!(var[&fr(0)].as_slice(), &[1, 101]);
        assert_eq!(var[&fr(1)].as_slice(), &[2, 102, 3, 103]);

        // `Stack` -> `Stack`: all returned values concatenated in order.
        let bigger =
            unwrap_stack(FramedTrack::from_stack(vec![1, 2]).expand(|value| vec![value, value]));
        assert_eq!(bigger, vec![1, 1, 2, 2]);
    }

    #[test]
    #[should_panic(expected = "equal length")]
    fn expand_fixed_rejects_ragged() {
        std::hint::black_box(fixed_track(0, 1, vec![1, 2, 3]).expand(|value| {
            if value == 2 {
                vec![value]
            } else {
                vec![value, value]
            }
        }));
    }

    #[test]
    fn expand_retains_and_sets_adapter() {
        // The frame adapter rewrites the tag to the current frame number.
        let adapter = |value: &mut Timed, frame: media::FrameNumber| value.tag = frame.0;

        // `expand_same` keeps the adapter: the resulting `Stack`, splatted over its
        // frame range, has the adapter applied per frame to each of its values.
        let track = FramedTrack::from_single_with_adapter(
            Timed {
                start: 0,
                dur: 2,
                tag: 0,
            },
            adapter,
        )
        .expand_same(|value| vec![value.clone(), value]);
        let out = unwrap_fixed(track.frame_zip(fixed_track(0, 1, vec![100, 200]), combine));
        assert_eq!(out.width, 2);
        assert_eq!(
            out.data,
            vec![(0, 0, 100), (0, 0, 100), (1, 1, 200), (1, 1, 200)]
        );

        // `expand_adapt` installs a new adapter on the result.
        let track = FramedTrack::from_single(Timed {
            start: 0,
            dur: 2,
            tag: 99,
        })
        .expand_adapt(|value| vec![value], adapter);
        let out = unwrap_fixed(track.frame_zip(fixed_track(0, 1, vec![5, 6]), combine));
        assert_eq!(out.data, vec![(0, 0, 5), (1, 1, 6)]);
    }

    #[test]
    fn frame_zip_stack_t1() {
        // A `Stack` of two values spanning frames 5, 6, 7 (timing from the first).
        let make_t1 = || {
            FramedTrack::from_stack(vec![
                Timed {
                    start: 5,
                    dur: 3,
                    tag: 100,
                },
                Timed {
                    start: 5,
                    dur: 3,
                    tag: 200,
                },
            ])
        };

        // (a) `Fixed` T2 with matching bounds: the fast path. Each frame produces two
        // outputs, so the result is `Fixed` width 2.
        let out = unwrap_fixed(make_t1().frame_zip(fixed_track(5, 1, vec![10, 20, 30]), combine));
        assert_eq!(out.start, fr(5));
        assert_eq!(out.width, 2);
        assert_eq!(
            out.data,
            vec![
                (5, 100, 10),
                (5, 200, 10),
                (6, 100, 20),
                (6, 200, 20),
                (7, 100, 30),
                (7, 200, 30),
            ]
        );

        // (b) `Single` T2: the slow path splatting the single over every frame.
        let out = unwrap_fixed(make_t1().frame_zip(FramedTrack::from_single(7), combine));
        assert_eq!(out.width, 2);
        assert_eq!(
            out.data,
            vec![
                (5, 100, 7),
                (5, 200, 7),
                (6, 100, 7),
                (6, 200, 7),
                (7, 100, 7),
                (7, 200, 7),
            ]
        );

        // (c) `Variable` T2 with a gap at frame 6: that frame's pair gets `None`.
        let out = unwrap_fixed(
            make_t1().frame_zip(variable_track(vec![(5, vec![10]), (7, vec![30])]), combine),
        );
        assert_eq!(
            out.data,
            vec![
                (5, 100, 10),
                (5, 200, 10),
                (6, 100, -1),
                (6, 200, -1),
                (7, 100, 30),
                (7, 200, 30),
            ]
        );
    }

    #[test]
    fn frame_zip_sliced_stack_t1() {
        let make_t1 = || {
            FramedTrack::from_stack(vec![
                Timed {
                    start: 0,
                    dur: 2,
                    tag: 1,
                },
                Timed {
                    start: 0,
                    dur: 2,
                    tag: 2,
                },
            ])
        };

        // Sliced fixed width: the per-frame T1 slice is both stack values; sum their
        // tags with the frame's T2 values, emitting one output per frame.
        let sum_one = |frame: media::FrameNumber,
                       v1: SmallVec<Timed, SMALL_VEC_SIZE>,
                       v2: Cow<[i32]>|
         -> SmallVec<(i32, i32), SMALL_VEC_SIZE> {
            let tag_sum: i32 = v1.iter().map(|entry| entry.tag).sum();
            let mut out = SmallVec::new();
            out.push((frame.0, tag_sum + v2.iter().sum::<i32>()));
            out
        };
        let out = unwrap_fixed(
            make_t1().frame_zip_sliced_fixed_width(fixed_track(0, 1, vec![10, 20]), sum_one),
        );
        assert_eq!(out.width, 1);
        assert_eq!(out.data, vec![(0, 13), (1, 23)]);

        // Sliced variable width against a `Variable` T2 whose width differs per frame.
        let zip_all = |frame: media::FrameNumber,
                       v1: SmallVec<Timed, SMALL_VEC_SIZE>,
                       v2: Cow<[i32]>|
         -> SmallVec<(i32, i32), SMALL_VEC_SIZE> {
            v2.iter()
                .map(|value2| (frame.0, v1[0].tag + value2))
                .collect()
        };
        let out = unwrap_variable(make_t1().frame_zip_sliced_variable_width(
            variable_track(vec![(0, vec![10, 11]), (1, vec![20])]),
            zip_all,
        ));
        assert_eq!(out[&fr(0)].as_slice(), &[(0, 11), (0, 12)]);
        assert_eq!(out[&fr(1)].as_slice(), &[(1, 21)]);
    }

    #[test]
    fn generic_zip_stack() {
        let add =
            |value1: i32, value2_opt: Option<Cow<i32>>| value1 + value2_opt.map_or(0, |cow| *cow);

        // `Stack` + `Single` -> `Stack`: each stack value paired with the single.
        let out = unwrap_stack(
            FramedTrack::from_stack(vec![1, 2, 3]).generic_zip(FramedTrack::from_single(10), add),
        );
        assert_eq!(out, vec![11, 12, 13]);

        // `Single` + `Stack` -> `Stack`: the single paired with each stack value.
        let out = unwrap_stack(
            FramedTrack::from_single(100).generic_zip(FramedTrack::from_stack(vec![1, 2, 3]), add),
        );
        assert_eq!(out, vec![101, 102, 103]);

        // `Stack` + `Stack` -> `Stack`: best-effort, each of ours paired with the
        // other stack's first (representative) value.
        let out = unwrap_stack(
            FramedTrack::from_stack(vec![1, 2])
                .generic_zip(FramedTrack::from_stack(vec![10, 20, 30]), add),
        );
        assert_eq!(out, vec![11, 12]);

        // `Stack` + `Fixed` -> `Fixed` width n: the other track drives the frames.
        let pair = |value1: Timed, value2_opt: Option<Cow<i32>>| {
            (value1.tag, value2_opt.map_or(-1, |cow| *cow))
        };
        let out = unwrap_fixed(
            FramedTrack::from_stack(vec![timed(1), timed(2)])
                .generic_zip(fixed_track(0, 1, vec![10, 20, 30]), pair),
        );
        assert_eq!(out.start, fr(0));
        assert_eq!(out.width, 2);
        assert_eq!(
            out.data,
            vec![(1, 10), (2, 10), (1, 20), (2, 20), (1, 30), (2, 30),]
        );

        // `Stack` + `Variable` -> `Variable`: the other track drives the frames.
        let out = unwrap_variable(
            FramedTrack::from_stack(vec![timed(1), timed(2)])
                .generic_zip(variable_track(vec![(0, vec![10]), (2, vec![30])]), pair),
        );
        assert_eq!(out[&fr(0)].as_slice(), &[(1, 10), (2, 10)]);
        assert_eq!(out[&fr(2)].as_slice(), &[(1, 30), (2, 30)]);
    }

    #[test]
    fn flatten() {
        let single = FramedTrack::from_single(Some(1));
        assert_matches!(single.flatten(), Some(flat));
        assert_eq!(unwrap_single(flat), 1);

        let none: Option<()> = None;
        let single_none = FramedTrack::from_single(none);
        assert!(single_none.flatten().is_none());

        let data = (1..=9).map(Some).collect();
        let fixed_all = FramedTrack::from_fixed(media::FrameNumber(1), 3, data);
        assert_matches!(fixed_all.flatten(), Some(flat));
        assert_eq!(unwrap_fixed(flat).data.len(), 9);

        let mut data: Vec<Option<i32>> = (1..=9).map(Some).collect();
        data[5] = None;
        let fixed_part = FramedTrack::from_fixed(media::FrameNumber(1), 3, data);
        assert_matches!(fixed_part.flatten(), Some(flat));
        let map = unwrap_variable(flat);
        assert_eq!(map[&media::FrameNumber(1)].len(), 3);
        assert_eq!(map[&media::FrameNumber(2)].len(), 2);
    }
}
