#![warn(missing_docs)]
#![crate_name = "snapup"]
#![cfg_attr(all(doc, CHANNEL_NIGHTLY), feature(doc_auto_cfg))]

//! Abstractions for handling snapshots with streams of subsequent updates.
#![doc(html_root_url = "https://docs.rs/snapup/0.1.7/")]

mod join_with_parent;

use futures::{stream, Stream, StreamExt};
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

    /// Creates [`SnapshotWithUpdates`] by taking the first item of the stream as the 'snapshot'
    /// and subsequent items as 'updates'. Returns [`None`] if the stream doesn't contain any items.
    pub async fn from_stream(mut stream: Updates) -> Option<Self>
    where
        Updates: Stream<Item = Snapshot> + Unpin,
    {
        let first = stream.next().await?;
        Some(Self::new(first, stream))
    }

    /// Converts [`SnapshotWithUpdates`] into a [`Stream`] of [`SnapshotOrUpdate`]
    pub fn into_stream(self) -> impl Stream<Item = SnapshotOrUpdate<Snapshot, Updates::Item>>
    where
        Updates: Stream,
    {
        let Self { snapshot, updates } = self;

        stream::once(future::ready(SnapshotOrUpdate::Snapshot(snapshot)))
            .chain(updates.map(SnapshotOrUpdate::Update))
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

    pub fn map_keys<R>(
        self,
        f: impl Fn(Key) -> R + Clone,
    ) -> SnapshotWithUpdates<
        impl IntoIterator<Item = (R, Value)>,
        impl Stream<Item = impl IntoIterator<Item = (R, Option<Value>)>>,
    > {
        let snapshot = self.snapshot.into_iter().map({
            let f = f.clone();
            move |(key, value)| (f(key), value)
        });

        let updates = self.updates.map({
            move |updates| {
                updates.into_iter().map({
                    let f = f.clone();
                    move |(key, value)| (f(key), value)
                })
            }
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

#[cfg(feature = "tokio-sync")]
impl<T: Clone + Send + Sync + 'static> From<tokio::sync::watch::Receiver<T>>
    for SnapshotWithUpdates<T, futures::stream::Skip<tokio_stream::wrappers::WatchStream<T>>>
{
    fn from(mut rx: tokio::sync::watch::Receiver<T>) -> Self {
        let rx_cloned = rx.clone();
        let guard = rx.borrow_and_update();

        // Skip the first value, since WatchStream always emits the current value of the Receiver channel,
        // but we want the 'updates' stream to only contain changes.
        let updates = tokio_stream::wrappers::WatchStream::new(rx_cloned).skip(1);

        // Drop the guard now that the stream is constructed. Now we can be sure we won't miss any
        // values on the stream due to any race conditions where a value was written to the receiver
        // while we were creating the stream. Holding the guard blocks any values being written
        // to the Receiver until we drop the guard.
        let snapshot = guard.clone();

        SnapshotWithUpdates { snapshot, updates }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotOrUpdate<Snapshot, Update> {
    Snapshot(Snapshot),
    Update(Update),
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "tokio-sync")]
    #[test]
    fn test_from_tokio_watch_receiver() {
        use futures::StreamExt;
        use tokio_test::{assert_pending, assert_ready_eq};

        let waker = futures::task::noop_waker_ref();
        let mut cx = std::task::Context::from_waker(&waker);

        let (tx, rx) = tokio::sync::watch::channel(3);

        let (snapshot, mut updates) = super::SnapshotWithUpdates::from(rx).into_inner();

        assert_eq!(snapshot, 3);
        assert_pending!(updates.poll_next_unpin(&mut cx));

        tx.send(4).unwrap();
        assert_ready_eq!(updates.poll_next_unpin(&mut cx), Some(4));
        assert_pending!(updates.poll_next_unpin(&mut cx));

        tx.send(5).unwrap();
        assert_ready_eq!(updates.poll_next_unpin(&mut cx), Some(5));
        assert_pending!(updates.poll_next_unpin(&mut cx));

        drop(tx);
        assert_ready_eq!(updates.poll_next_unpin(&mut cx), None);
    }
}
