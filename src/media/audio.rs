use core::slice;

pub use bestsource::AudioProperties;

use super::bindings::bestsource;

pub struct Audio {
    source: bestsource::BestAudioSource,
    pub properties: bestsource::AudioProperties,
}

impl Audio {
    pub fn load<P: AsRef<std::path::Path>>(filename: P) -> Audio {
        let source =
            bestsource::BestAudioSource::new(filename, -1, -1, 0, std::path::Path::new(""), 0.0);
        let properties = source.get_audio_properties();

        println!("audio properties: {:?}", properties);

        Audio { source, properties }
    }

    pub fn fill_buffer_packed<T>(&mut self, data: &mut [T], start_frame: i64, count_frames: i64) {
        // Transmute buffer
        let data_u8 = unsafe {
            slice::from_raw_parts_mut(data.as_mut_ptr() as *mut u8, std::mem::size_of_val(data))
        };

        self.source
            .get_packed_audio(data_u8, start_frame, count_frames);
    }
}
