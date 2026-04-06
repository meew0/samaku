use std::fmt;

/// Status of a toast, used to determine its colour scheme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Status {
    #[default]
    Primary,
    Secondary,
    Success,
    Danger,
}

impl Status {
    pub const ALL: &'static [Self] = &[Self::Primary, Self::Secondary, Self::Success, Self::Danger];
}

impl fmt::Display for Status {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Status::Primary => "Primary",
            Status::Secondary => "Secondary",
            Status::Success => "Success",
            Status::Danger => "Danger",
        }
        .fmt(formatter)
    }
}

/// The content of a toast's body area.
#[derive(Debug, Clone)]
pub enum Content<Message> {
    /// Plain text message (default behaviour).
    Message,
    /// A progress bar displaying the progress of some task. Value is in range 0.0–1.0.
    Progress { progress: f32 },
    /// Two buttons that allow the user to confirm or deny some action. Pressing either fires
    /// the stored message and closes the toast.
    Confirm {
        confirm_label: String,
        deny_label: String,
        /// Boxed to break the recursive type when `M = message::Message`.
        on_confirm: Box<Message>,
        on_deny: Box<Message>,
    },
}

impl<M: PartialEq> PartialEq for Content<M> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Message, Self::Message) | (Self::Progress { .. }, Self::Progress { .. }) => true,
            (
                Self::Confirm {
                    confirm_label: cl1,
                    deny_label: dl1,
                    ..
                },
                Self::Confirm {
                    confirm_label: cl2,
                    deny_label: dl2,
                    ..
                },
            ) => cl1 == cl2 && dl1 == dl2,
            _ => false,
        }
    }
}

/// A single toast notification.
#[derive(Debug, Clone)]
pub struct Toast<Message = ()> {
    /// Stable ID assigned by [`List::push`]. Zero until pushed.
    pub id: Id,
    /// How many identical toasts have been grouped together (deduplication).
    pub count: u32,
    pub title: String,
    pub body: String,
    pub status: Status,
    /// Per-toast timeout override. `None` means use the [`Manager`]'s default.
    pub timeout_secs: Option<u64>,
    pub content: Content<Message>,
}

impl<M> Toast<M> {
    /// Create a plain-text message toast.
    #[must_use]
    pub fn message(status: Status, title: String, body: String) -> Self {
        Self {
            id: Id(0),
            count: 1,
            title,
            body,
            status,
            timeout_secs: None,
            content: Content::Message,
        }
    }

    /// Create a progress-bar toast. Update the progress via
    /// [`List::update_progress`] using the ID returned by [`List::push`].
    #[must_use]
    pub fn progress(status: Status, title: String, body: String) -> Self {
        Self {
            id: Id(0),
            count: 1,
            title,
            body,
            status,
            timeout_secs: None,
            content: Content::Progress { progress: 0.0 },
        }
    }

    /// Create a confirm/deny toast. When the user presses a button the stored message is
    /// dispatched and the toast is closed.
    #[must_use]
    pub fn confirm(
        status: Status,
        title: String,
        body: String,
        confirm_label: String,
        deny_label: String,
        on_confirm: M,
        on_deny: M,
    ) -> Self {
        Self {
            id: Id(0),
            count: 1,
            title,
            body,
            status,
            timeout_secs: None,
            content: Content::Confirm {
                confirm_label,
                deny_label,
                on_confirm: Box::new(on_confirm),
                on_deny: Box::new(on_deny),
            },
        }
    }

    /// "Danger" status toast for anyhow errors.
    #[must_use]
    pub fn error(err: &anyhow::Error) -> Self {
        Self::message(Status::Danger, "Error".to_owned(), format!("{err:#}"))
    }

    /// Override the timeout for this specific toast (builder-style).
    #[must_use]
    pub fn with_timeout(self, secs: u64) -> Self {
        Self {
            timeout_secs: Some(secs),
            ..self
        }
    }
}

impl<M> PartialEq for Toast<M> {
    fn eq(&self, other: &Self) -> bool {
        // Ignore id and count; only plain-message toasts are deduplicated.
        matches!(self.content, Content::Message)
            && matches!(other.content, Content::Message)
            && self.title == other.title
            && self.body == other.body
            && self.status == other.status
    }
}

impl<M> Eq for Toast<M> {}

/// Manages a collection of toasts with deduplication and stable per-toast IDs.
pub struct List<M> {
    toasts: Vec<Toast<M>>,
    next_id: Id,
}

impl<M> List<M> {
    #[must_use]
    pub fn new() -> Self {
        Self {
            toasts: Vec::new(),
            next_id: Id(1),
        }
    }

    /// Add a toast. Assigns a stable ID. For plain-message toasts, deduplicates by
    /// incrementing the count on an identical existing toast instead of adding a new one.
    /// Also prints the toast to the console.
    pub fn push(&mut self, mut toast: Toast<M>) -> Id {
        println!(
            "[toast status={:?}] [{}] {}",
            toast.status, toast.title, toast.body
        );

        if matches!(&toast.content, Content::Message)
            && let Some(existing) = self.toasts.iter_mut().find(|existing| **existing == toast)
        {
            existing.count += 1;
            return existing.id;
        }

        let id = self.next_id;
        toast.id = id;
        self.next_id = Id(self.next_id.0 + 1);
        self.toasts.push(toast);

        id
    }

    // Utility methods for various toast types
    pub fn progress(&mut self, title: &str, body: &str) -> Id {
        self.push(Toast::progress(
            Status::Primary,
            title.to_owned(),
            body.to_owned(),
        ))
    }

    /// Remove the toast at `index`. Handles the race condition where two close events
    /// arrive for the same toast gracefully.
    pub fn remove(&mut self, index: usize) {
        if index < self.toasts.len() {
            self.toasts.remove(index);
        } else {
            self.toasts.pop();
        }
    }

    #[must_use]
    pub fn as_slice(&self) -> &[Toast<M>] {
        &self.toasts
    }

    /// Update the progress value of the toast with the given stable ID.
    /// Returns `true` if the toast was found and updated.
    pub fn update_progress(&mut self, id: Id, progress: f32) -> bool {
        if let Some(toast) = self.toasts.iter_mut().find(|existing| existing.id == id)
            && let Content::Progress {
                progress: ref mut stored_progress,
            } = toast.content
        {
            *stored_progress = progress.clamp(0.0, 1.0);
            return true;
        }
        false
    }

    /// Convenience method: if `result` is `Ok`, returns `Some(val)`. If it is `Err`,
    /// adds a danger toast with the error message and returns `None`.
    pub fn anyhow<T>(&mut self, result: anyhow::Result<T>) -> Option<T> {
        match result {
            Ok(val) => Some(val),
            Err(err) => {
                self.push(Toast::error(&err));
                None
            }
        }
    }
}

impl<M> Default for List<M> {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Id(u64);
