//! Code for specific types of objects.

use super::{Id, RestoreData, Store, Value};
use crate::media;
use crate::model::select::Selection;
use std::any::Any;
use std::collections::HashSet;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Type {
    MotionTrack,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Stores {
    pub motion_tracks: Store<media::motion::Track>,
}

impl Stores {
    fn apply<R, F: ApplyFn<R>>(&mut self, object_type: Type, apply_fn: F) -> R {
        match object_type {
            Type::MotionTrack => apply_fn.apply(&mut self.motion_tracks),
        }
    }

    /// Create a new instance of the specified object, and insert it into the respective store.
    ///
    /// Returns the newly inserted ID as well as a clone of the object.
    pub fn add(&mut self, object_type: Type, instance: Box<dyn Value>) -> (Id, Box<dyn Value>) {
        struct Add(Box<dyn Any>);
        impl ApplyFn<(Id, Box<dyn Value>)> for Add {
            fn apply<V: Value + Clone>(self, store: &mut Store<V>) -> (Id, Box<dyn Value>) {
                let downcast: V = *self.0.downcast().expect("downcast failed");
                let cloned = downcast.clone();

                let new_id = store.add(downcast);
                (new_id, Box::new(cloned))
            }
        }

        self.apply(object_type, Add(instance))
    }

    pub fn delete(&mut self, object_type: Type, ids: HashSet<Id>) -> (RestoreData, Selection<Id>) {
        struct Delete(HashSet<Id>);
        impl ApplyFn<(RestoreData, Selection<Id>)> for Delete {
            fn apply<V: Value + Clone>(self, store: &mut Store<V>) -> (RestoreData, Selection<Id>) {
                let ids = self.0;

                let old_selected = store.selection.deselect_all(ids.iter().copied());
                let restore = ids
                    .iter()
                    .filter_map(|&id| {
                        store.remove(id).map(|instance| {
                            let value_box: Box<dyn Value> = Box::new(instance);
                            Some((id, value_box))
                        })
                    })
                    .collect();

                (RestoreData(restore), old_selected)
            }
        }

        self.apply(object_type, Delete(ids))
    }

    pub fn restore(
        &mut self,
        object_type: Type,
        restore: RestoreData,
        old_selected: Selection<Id>,
    ) -> HashSet<Id> {
        struct Restore(RestoreData, Selection<Id>);
        impl ApplyFn<HashSet<Id>> for Restore {
            fn apply<V: Value + Clone>(self, store: &mut Store<V>) -> HashSet<Id> {
                let Restore(mut restore, old_selected) = self;
                let mut restored = HashSet::with_capacity(restore.0.len());

                if let Some(restore_inner) = Arc::get_mut(&mut restore.0) {
                    // We own the only copy of the Arc, so we can move values out of the slice.
                    for opt in restore_inner {
                        let (id, instance_value_box) =
                            opt.take().expect("missing RestoreData value");
                        // We need to coerce here
                        let instance_any_box: Box<dyn Any> = instance_value_box;
                        let instance = *instance_any_box.downcast().expect("downcast failed");
                        store.restore(id, instance);
                        restored.insert(id);
                    }
                } else {
                    // There are more copies of the Arc, so we have to clone the inner values.
                    for opt in restore.0.iter() {
                        let &(id, ref instance_value_box_ref) =
                            opt.as_ref().expect("missing RestoreData value");
                        // We need to upcast the trait object itself here; casting the `Box` would
                        // require changing the vtable behind the reference, which is not possible.
                        let instance_any_ref: &dyn Any = &**instance_value_box_ref;
                        let instance_ref: &V =
                            instance_any_ref.downcast_ref().expect("downcast failed");
                        store.restore(id, instance_ref.clone());
                        restored.insert(id);
                    }
                }

                store.selection.select_from(&old_selected);
                restored
            }
        }

        self.apply(object_type, Restore(restore, old_selected))
    }

    pub fn selection_mut(&mut self, object_type: Type) -> &mut Selection<Id> {
        match object_type {
            Type::MotionTrack => &mut self.motion_tracks.selection,
        }
    }

    #[must_use]
    pub fn undo_names(object_type: Type) -> UndoNames {
        match object_type {
            Type::MotionTrack => UndoNames {
                create: "Create motion track",
                delete: "Delete tracks",
                restore: "Restore tracks",
                toggle_selection: "Toggle motion track selection",
                select_only: "Select motion track",
                set_selection_single: "Set motion track selection (single)",
                set_selection_many: "Select motion tracks",
            },
        }
    }
}

impl Default for Stores {
    fn default() -> Self {
        Self {
            motion_tracks: Store::new(),
        }
    }
}

trait ApplyFn<R> {
    fn apply<V: Value + Clone>(self, store: &mut Store<V>) -> R;
}

pub struct UndoNames {
    pub create: &'static str,
    pub delete: &'static str,
    pub restore: &'static str,
    pub toggle_selection: &'static str,
    pub select_only: &'static str,
    pub set_selection_single: &'static str,
    pub set_selection_many: &'static str,
}

impl Store<media::motion::Track> {
    #[must_use]
    pub fn stab(&self, frame: media::FrameNumber) -> Vec<(Id, &media::motion::Track)> {
        // TODO 2026-05-31: optimize this using interavl
        // 2026-07-30: still a good idea after object refactor?
        self.map
            .iter()
            .filter_map(|(&id, track)| {
                (frame >= track.first_frame && frame <= track.last_frame).then_some((id, track))
            })
            .collect()
    }
}
