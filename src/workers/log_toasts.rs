use std::{collections::HashSet, thread};

use crate::{media, message, model};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) enum MessageIn {
    Libass(i32, String),
}

pub(super) fn spawn(
    tx_out: super::GlobalSender,
    _shared_state: &crate::SharedState,
) -> super::Worker<MessageIn> {
    let (tx_in, rx_in) = std::sync::mpsc::channel::<MessageIn>();

    let handle = thread::Builder::new()
        .name("samaku_log_toasts".to_owned())
        .spawn(move || {
            let mut seen: HashSet<MessageIn> = HashSet::new();
            loop {
                match rx_in.recv() {
                    Ok(message) => {
                        // Remove uninformative pointer value from the start of some libass
                        // messages
                        let modified_message = match message {
                            MessageIn::Libass(level, string) => {
                                if string.starts_with("[0x") {
                                    let modified_string = match string.split_once("]: ") {
                                        Some((_, modified_string)) => modified_string.to_owned(),
                                        None => string,
                                    };
                                    MessageIn::Libass(level, modified_string)
                                } else {
                                    MessageIn::Libass(level, string)
                                }
                            }
                        };

                        // Skip already seen messages, to avoid flooding the screen with messages
                        // that are generated over and over
                        if !seen.contains(&modified_message) {
                            seen.insert(modified_message.clone());
                            match modified_message {
                                MessageIn::Libass(level, string) => {
                                    let status_and_title = match level {
                                        0 | 1 => {
                                            Some((model::toast::Status::Danger, "libass error"))
                                        }
                                        2 => {
                                            Some((model::toast::Status::Primary, "libass warning"))
                                        }
                                        _ => None,
                                    };

                                    if let Some((status, title)) = status_and_title {
                                        let toast = model::toast::Toast::message(
                                            status,
                                            title.to_owned(),
                                            string,
                                        );
                                        tx_out.send(message::Message::Toast(toast));
                                    }
                                }
                            }
                        }
                    }
                    Err(_) => return,
                }
            }
        })
        .unwrap();

    let tx_in2 = tx_in.clone();
    media::subtitle::set_libass_callback(move |level, string| {
        // Ignore errors, since this function should not panic and we can't really do anything else
        drop(tx_in2.send(MessageIn::Libass(level, string)));
    });

    super::Worker {
        worker_type: super::Type::LogToasts,
        _handle: handle,
        message_in: tx_in,
    }
}
