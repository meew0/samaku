use crate::{message, model};
use std::thread;

#[derive(Debug, Clone, PartialEq)]
pub(super) enum MessageIn {
    Progress(model::toast::Id, f32),
}

pub(super) fn spawn(
    tx_out: super::GlobalSender,
    _shared_state: &crate::SharedState,
) -> super::Worker<MessageIn> {
    let (tx_in, rx_in) = std::sync::mpsc::channel::<MessageIn>();

    let handle = thread::Builder::new()
        .name("samaku_progress_listener".to_owned())
        .spawn(move || {
            loop {
                match rx_in.recv() {
                    Ok(message) => match message {
                        MessageIn::Progress(key, progress) => {
                            if tx_out
                                .unbounded_send(message::Message::UpdateToastProgress(
                                    key, progress,
                                ))
                                .is_err()
                            {
                                println!(
                                    "failed to update toast progress: key {key:?} progress {progress}"
                                );
                            }
                        }
                    },
                    Err(_) => return,
                }
            }
        })
        .unwrap();

    super::Worker {
        worker_type: super::Type::ProgressListener,
        _handle: handle,
        message_in: tx_in,
    }
}
