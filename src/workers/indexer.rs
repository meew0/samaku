use crate::{media, message};
use std::thread;

pub(super) type MessageCallback = dyn FnOnce(media::Index) -> message::Message + Send;

pub(super) enum MessageIn {
    Index(media::Indexer, Box<MessageCallback>),
}

pub(super) fn spawn(
    tx_out: super::GlobalSender,
    _shared_state: &crate::SharedState,
) -> super::Worker<MessageIn> {
    let (tx_in, rx_in) = std::sync::mpsc::channel::<MessageIn>();

    let handle = thread::Builder::new()
        .name("samaku_indexer".to_owned())
        .spawn(move || {
            loop {
                match rx_in.recv() {
                    Ok(message) => match message {
                        MessageIn::Index(indexer, callback) => match indexer.run() {
                            Ok(index) => {
                                tx_out.send(callback(index));
                            }
                            Err(err) => {
                                tx_out.error(err, "Indexing failed");
                            }
                        },
                    },
                    Err(_) => return,
                }
            }
        })
        .unwrap();

    super::Worker {
        worker_type: super::Type::Indexer,
        _handle: handle,
        message_in: tx_in,
    }
}
