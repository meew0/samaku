use crate::{message, model};
use std::collections::HashMap;
use std::sync::mpsc::RecvTimeoutError;
use std::thread;
use std::time::Duration;

const RATE_LIMIT_INTERVAL: Duration = Duration::from_millis(100);

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
            let mut pending: HashMap<model::toast::Id, f32> = HashMap::new();
            loop {
                // Collect all messages that arrive within the next 100ms window,
                // keeping only the latest value per key.
                let deadline = std::time::Instant::now() + RATE_LIMIT_INTERVAL;
                loop {
                    let remaining = deadline.saturating_duration_since(std::time::Instant::now());
                    if remaining.is_zero() {
                        break;
                    }
                    match rx_in.recv_timeout(remaining) {
                        Ok(MessageIn::Progress(key, progress)) => {
                            pending.insert(key, progress);
                        }
                        Err(RecvTimeoutError::Timeout) => break,
                        Err(RecvTimeoutError::Disconnected) => return,
                    }
                }
                // Flush one update per key.
                for (key, progress) in pending.drain() {
                    tx_out.send(message::Message::UpdateToastProgress(key, progress));
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
