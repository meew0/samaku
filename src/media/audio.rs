use std::path::Path;

use super::bindings::{bestsource, c_string};

const DEFAULT_SCRIPT: &str = include_str!("default_scripts/audio.py");

pub struct Audio {
    bas: bestsource::BestAudioSource,
    properties: bestsource::AudioProperties,
}

impl Audio {
    pub fn load<P: AsRef<Path>>(filename: P) -> Audio {
        let bas = bestsource::BestAudioSource::new(filename, -1, -1, 0, Path::new(""), 0.0);
        let properties = bas.get_audio_properties();

        println!("audio properties: {:?}", properties);

        Audio { bas, properties }
    }
}
