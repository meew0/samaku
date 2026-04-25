use super::bindings::ffms2;
use crate::model;
use anyhow::Context as _;
pub use ffms2::ProgressCallback;

pub struct Index {
    inner: ffms2::Index,
}

impl Index {
    pub(super) fn into_inner(self) -> ffms2::Index {
        let Self { inner } = self;
        inner
    }
}

impl std::fmt::Debug for Index {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Index {{ opaque }}")
    }
}

pub struct Indexer {
    inner: ffms2::Indexer,
}

impl Indexer {
    pub(super) fn new(inner: ffms2::Indexer) -> Self {
        Self { inner }
    }

    pub fn set_progress_callback<
        F: FnMut(i64, i64) -> model::CancellationState + Send + 'static,
    >(
        &mut self,
        callback: F,
    ) {
        self.inner.set_progress_callback(Box::new(callback));
    }

    pub fn run(self) -> anyhow::Result<Index> {
        let index = self
            .inner
            .do_indexing(ffms2::IndexErrorHandling::Abort)
            .context("indexing")?;

        Ok(Index { inner: index })
    }
}
