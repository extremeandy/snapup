#![warn(missing_docs)]
#![crate_name = "snapup"]
#![cfg_attr(all(doc, CHANNEL_NIGHTLY), feature(doc_auto_cfg))]

//! Abstractions for handling snapshots with streams of subsequent updates.
#![doc(html_root_url = "https://docs.rs/snapup/0.1.3/")]

mod join_with_parent;

use futures::{Stream, StreamExt};
use std::{collections::HashSet, future, hash::Hash};

/// Wraps a 'snapshot' (initial data) with updates (some kind of update which
/// can be applied to the snapshot to update).
///
/// This is useful for modelling behaviour of CRUD repositories where updates are
/// also monitored.
#[derive(Debug)]
pub struct SnapshotWithUpdates<Snapshot, Updates> {
    /// Initial snapshot of the data
    pub snapshot: Snapshot,

    /// Stream of updates that can be applied to the snapshot
    pub updates: Updates,
}

impl<Snapshot, Updates> SnapshotWithUpdates<Snapshot, Updates> {
    /// Constructor
    pub fn new(snapshot: Snapshot, updates: Updates) -> Self {
        Self { snapshot, updates }
    }

    /// Converts the struct into a tuple of (snapshot, updates)
    pub fn into_inner(self) -> (Snapshot, Updates) {
        let Self { snapshot, updates } = self;
        (snapshot, updates)
    }
}

impl<Key, Value, Snapshot, Updates> SnapshotWithUpdates<Snapshot, Updates>
where
    Key: Clone + Hash + Eq,
    Value: Clone,
    Snapshot: IntoIterator<Item = (Key, Value)>,
    Updates: Stream,
    Updates::Item: IntoIterator<Item = (Key, Option<Value>)>,
{
    /// Aggregates updates into a stream of snapshots by sequentially applying updates
    /// to the initial snapshot.
    pub fn into_snapshots(
        self,
    ) -> SnapshotWithUpdates<im::HashMap<Key, Value>, impl Stream<Item = im::HashMap<Key, Value>>>
    {
        let snapshot = self.snapshot.into_iter().collect::<im::HashMap<_, _>>();

        let updates = self.updates.scan(snapshot.clone(), |state, next| {
            for (key, value) in next {
                match value {
                    Some(value) => {
                        state.insert(key, value);
                    }
                    None => {
                        state.remove(&key);
                    }
                }
            }

            future::ready(Some(state.clone()))
        });

        SnapshotWithUpdates::new(snapshot, updates)
    }

    pub fn map_values<R>(
        self,
        f: impl Fn(Value) -> R + Clone,
    ) -> SnapshotWithUpdates<
        impl IntoIterator<Item = (Key, R)>,
        impl Stream<Item = impl IntoIterator<Item = (Key, Option<R>)>>,
    > {
        let snapshot = self.snapshot.into_iter().map({
            let f = f.clone();
            move |(key, value)| (key, f(value))
        });

        let updates = self.updates.map({
            move |updates| {
                updates.into_iter().map({
                    let f = f.clone();
                    move |(key, value)| (key, value.map(f.clone()))
                })
            }
        });

        SnapshotWithUpdates::new(snapshot, updates)
    }

    pub fn filter(
        self,
        f: impl Fn(&Key, &Value) -> bool + Clone,
    ) -> SnapshotWithUpdates<
        impl IntoIterator<Item = (Key, Value)>,
        impl Stream<Item = impl IntoIterator<Item = (Key, Option<Value>)>>,
    > {
        let snapshot = self
            .snapshot
            .into_iter()
            .filter({
                let f = f.clone();
                move |(key, value)| f(key, value)
            })
            .collect::<Vec<_>>();

        let filtered_keys = snapshot
            .iter()
            .map(|(key, _)| key)
            .cloned()
            .collect::<HashSet<_>>();

        let updates = self
            .updates
            .scan(filtered_keys, move |filtered_keys, updates| {
                let mut filtered_updates = vec![];

                for (key, value) in updates {
                    match value {
                        Some(value) if f(&key, &value) => {
                            filtered_keys.insert(key.to_owned());
                            filtered_updates.push((key, Some(value)));
                        }
                        _ => {
                            if filtered_keys.remove(&key) {
                                filtered_updates.push((key, None));
                            }
                        }
                    }
                }

                let result = (filtered_updates.len() > 0).then_some(filtered_updates);

                future::ready(result)
            });

        SnapshotWithUpdates::new(snapshot, updates)
    }

    pub fn filter_keys(
        self,
        f: impl Fn(&Key) -> bool + Clone,
    ) -> SnapshotWithUpdates<
        impl IntoIterator<Item = (Key, Value)>,
        impl Stream<Item = impl IntoIterator<Item = (Key, Option<Value>)>>,
    > {
        let snapshot = self.snapshot.into_iter().filter({
            let f = f.clone();
            move |(key, _)| f(key)
        });

        let updates = self.updates.filter_map(move |updates| {
            let updates = updates
                .into_iter()
                .filter({
                    let f = f.clone();
                    move |(key, _)| f(key)
                })
                .collect::<Vec<_>>();

            let result = if updates.len() == 0 {
                None
            } else {
                Some(updates)
            };

            future::ready(result)
        });

        SnapshotWithUpdates::new(snapshot, updates)
    }
}
