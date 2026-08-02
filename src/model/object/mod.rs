//! “Objects” are entities with consistent CRUD behavior.
//!
//! We use this code to avoid duplicating the messages and the `update` code
//! for every different type of object, while still making it efficient to access
//! specific types of objects in hot paths.

mod create;
mod objects;

use crate::model;
pub use create::create;
pub use objects::Stores;
pub use objects::Type;
use std::any::Any;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Id(u64);

// We want IDs to be opaque in normal use, but test code may occasionally need to construct them.
#[cfg(test)]
impl Id {
    #[must_use]
    pub fn new(value: u64) -> Self {
        Self(value)
    }
}

pub trait Value: super::Named + Any + Send + Sync {}
impl<T: super::Named + Any + Send + Sync> Value for T {}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Store<V: Value> {
    map: HashMap<Id, V>,
    next_id: Id,
    pub selection: super::select::Selection<Id>,
}

impl<V: Value> Store<V> {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn get(&self, id: Id) -> Option<&V> {
        self.map.get(&id)
    }

    #[must_use]
    pub fn get_mut(&mut self, id: Id) -> Option<&mut V> {
        self.map.get_mut(&id)
    }

    pub fn add(&mut self, value: V) -> Id {
        let id = self.next_id;
        self.next_id = Id(id.0 + 1);
        self.map.insert(id, value);
        id
    }

    pub fn remove(&mut self, id: Id) -> Option<V> {
        self.map.remove(&id)
    }

    pub fn restore(&mut self, id: Id, track: V) {
        self.map.insert(id, track);
    }
}

impl<V: Value> Default for Store<V> {
    fn default() -> Self {
        Self {
            map: HashMap::new(),
            next_id: Id(0),
            selection: super::select::Selection::default(),
        }
    }
}

impl<V: Value> super::NamedListIterable for Store<V> {
    type Key = Id;

    fn iter_named(&self) -> impl Iterator<Item = model::NamedEntry<'_, Self::Key>> {
        self.map.iter().map(|(&key, value)| model::NamedEntry {
            id: key,
            name: value.name(),
        })
    }
}

/// Instances of a certain object type, together with their IDs.
///
/// This type is an `Arc` of a slice to enable cloning the `Box<dyn>` and sharing it across threads.
/// The `Option` case is purely to enable moving data out of the slice,
/// since this is not easily possible by iteration.
#[derive(Clone)]
pub struct RestoreData(Arc<[Option<RestoreElement>]>);

/// A value to be restored together with the ID it should have.
type RestoreElement = (Id, Box<dyn Value>);

impl RestoreData {
    #[must_use]
    pub fn new_single(id: Id, value: Box<dyn Value>) -> Self {
        let vec = vec![Some((id, value))];
        Self(vec.into())
    }
}

impl std::fmt::Debug for RestoreData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "RestoreData {{ opaque }}")
    }
}
