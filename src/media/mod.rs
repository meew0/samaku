pub use audio::Audio;
pub use audio::Properties as AudioProperties;
pub use video::FrameRate;
pub use video::Metadata as VideoMetadata;
pub use video::Video;

mod audio;
mod bindings;
mod motion;
pub mod subtitle;
mod video;
