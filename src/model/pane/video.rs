pub struct State {
    // The frame that is actually being displayed right now.
    // Will be slightly different from the information in
    // PlaybackState due to decoding latency etc.
    pub actual_frame: i32,

    // The image for the frame being displayed
    pub image_handle: Option<iced::widget::image::Handle>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            actual_frame: -1,
            image_handle: None,
        }
    }
}
