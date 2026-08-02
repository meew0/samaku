use crate::media;

pub fn create(object_type: super::Type, global_state: &crate::Samaku) -> Box<dyn super::Value> {
    match object_type {
        super::Type::MotionTrack => Box::new(create_motion_track(global_state)),
    }
}

fn create_motion_track(global_state: &crate::Samaku) -> media::motion::Track {
    let origin_frame = global_state
        .current_frame()
        .expect("video should be loaded");

    let marker = media::motion::Marker::default();
    media::motion::Track::new(origin_frame, marker, "New track".to_owned())
}
