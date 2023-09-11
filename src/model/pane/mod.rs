pub mod video;

pub enum PaneState {
    Unassigned,
    Video(video::State),
}

pub struct PaneData {
    pub id: u64,
    pub state: PaneState,
}

impl PaneData {
    pub(crate) fn new(id: u64) -> PaneData {
        PaneData {
            id,
            state: PaneState::Unassigned,
        }
    }
}
