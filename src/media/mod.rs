pub use audio::Audio;
pub use audio::Properties as AudioProperties;
pub use frame::{
    Delta as FrameDelta,
    Framerate as FrameRate,
    Number as FrameNumber,
    TimeMode,
    UNLOADED_FRAMERATE,
    util as frame_util, // primarily for test/benchmark purposes
};
pub use index::{Index, Indexer, ProgressCallback};
pub use video::Metadata as VideoMetadata;
pub use video::Video;

mod audio;
mod bindings;
mod frame;
mod index;
pub mod motion;
pub mod subtitle;
mod video;

/// Initialize media libraries that need to be initialized.
pub fn init() {
    audio::init();
}
