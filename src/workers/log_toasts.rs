use std::{collections::HashSet, thread};

use crate::{media, message, view};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Message {
    Libass(i32, String),
}

pub fn spawn(
    tx_out: super::GlobalSender,
    _shared_state: &crate::SharedState,
) -> super::Worker<self::Message> {
    let (tx_in, rx_in) = std::sync::mpsc::channel::<self::Message>();

    let handle = thread::Builder::new()
        .name("samaku_log_toasts".to_string())
        .spawn(move || {
            let mut seen: HashSet<Message> = HashSet::new();
            loop {
                match rx_in.recv() {
                    Ok(message) => {
                        // Skip already seen messages, to avoid flooding the screen with messages
                        // that are generated over and over
                        if !seen.contains(&message) {
                            seen.insert(message.clone());
                            match message {
                                self::Message::Libass(level, string) => {
                                    let status_and_title = match level {
                                        0 | 1 => {
                                            Some((view::toast::Status::Danger, "libass error"))
                                        }
                                        2 => Some((view::toast::Status::Primary, "libass warning")),
                                        _ => None,
                                    };

                                    if let Some((status, title)) = status_and_title {
                                        let toast = view::toast::Toast {
                                            status,
                                            title: title.to_string(),
                                            body: string,
                                        };
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
        let _ = tx_in2.send(Message::Libass(level, string));
    });

    super::Worker {
        worker_type: super::Type::LogToasts,
        _handle: handle,
        message_in: tx_in,
    }
}
