use std::{collections::HashSet, thread};

use crate::{media, message, view};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum MessageIn {
    Libass(i32, String),
}

pub fn spawn(
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
                        let message = match message {
                            MessageIn::Libass(level, string) => {
                                if string.starts_with("[0x") {
                                    if let Some(pos) = string.find("]: ") {
                                        MessageIn::Libass(level, string[(pos + 3)..].to_string())
                                    } else {
                                        MessageIn::Libass(level, string)
                                    }
                                } else {
                                    MessageIn::Libass(level, string)
                                }
                            }
                        };

                        // Skip already seen messages, to avoid flooding the screen with messages
                        // that are generated over and over
                        if !seen.contains(&message) {
                            seen.insert(message.clone());
                            match message {
                                MessageIn::Libass(level, string) => {
                                    let status_and_title = match level {
                                        0 | 1 => {
                                            Some((view::toast::Status::Danger, "libass error"))
                                        }
                                        2 => Some((view::toast::Status::Primary, "libass warning")),
                                        _ => None,
                                    };

                                    if let Some((status, title)) = status_and_title {
                                        let toast = view::toast::Toast::new(
                                            status,
                                            title.to_owned(),
                                            string,
                                        );
                                        tx_out
                                            .unbounded_send(message::Message::Toast(toast))
                                            .unwrap();
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
