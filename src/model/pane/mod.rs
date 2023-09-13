pub mod video;

#[derive(Debug, Clone)]
pub enum PaneState {
    Unassigned,
    Video(video::State),
}

#[derive(Debug, Clone)]
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
