use core::slice;

pub use bestsource::AudioProperties as Properties;

use super::bindings::bestsource;

pub struct Audio {
    source: bestsource::BestAudioSource,
    pub properties: Properties,
}

impl Audio {
    pub fn load<P: AsRef<std::path::Path>>(filename: P) -> Audio {
        let source = bestsource::BestAudioSource::new(
            filename,
            -1,
            -1,
            false,
            0,
            bestsource::CacheMode::Disable,
            std::path::Path::new(""),
            0.0,
        );
        let properties = source.get_audio_properties();

        println!("audio properties: {properties:?}");

        Audio { source, properties }
    }

    /// Fills the given data with `count_frames` frames, starting from `start_frame`.
    ///
    /// # Panics
    /// Bestsource indexes frames using signed integers, so this function panics if `start_frame`
    /// or `count_frames` is greater than the maximum value of `i64` (signed), that is,
    /// `2**63 - 1`.
    pub fn fill_buffer_packed<T>(&mut self, data: &mut [T], start_frame: u64, count_frames: u64) {
        // Transmute buffer
        let data_u8 =
            unsafe { slice::from_raw_parts_mut(data.as_mut_ptr().cast::<u8>(), size_of_val(data)) };

        self.source.get_packed_audio(
            data_u8,
            start_frame.try_into().expect("start_frame overflow"),
            count_frames.try_into().expect("count_frames overflow"),
        );
    }
}
