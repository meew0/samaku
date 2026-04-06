pub use audio::Audio;
pub use audio::Properties as AudioProperties;
pub use index::{Index, Indexer, ProgressCallback};
pub use video::FrameRate;
pub use video::Metadata as VideoMetadata;
pub use video::Video;

mod audio;
mod bindings;
mod index;
pub mod motion;
pub mod subtitle;
mod video;

/// Initialize media libraries that need to be initialized.
pub fn init() {
    audio::init();
}
