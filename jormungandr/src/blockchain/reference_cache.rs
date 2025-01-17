use crate::{blockcfg::HeaderHash, blockchain::Ref};
use std::{collections::HashMap, convert::Infallible, time::Duration};
use tokio::{
    prelude::*,
    sync::lock::Lock,
    timer::{self, delay_queue, DelayQueue},
};

/// object that store the [`Ref`] in a cache. Every time a [`Ref`]
/// is accessed its TTL will be reset. Once the TTL of [`Ref`] has
/// expired it may be removed from the cache.
///
/// The cache expired [`Ref`] will be removed only if the [`Ref`]'s
/// TTL has expired and [`purge`] has been called and has completed.
///
/// [`Ref`]: ./struct.Ref.html
/// [`purge`]: ./struct.Ref.html#method.purge
#[derive(Clone)]
pub struct RefCache {
    inner: Lock<RefCacheData>,
}

/// cache of already loaded in-memory block `Ref`
struct RefCacheData {
    entries: HashMap<HeaderHash, (Ref, delay_queue::Key)>,
    expirations: DelayQueue<HeaderHash>,

    ttl: Duration,
}

impl RefCache {
    /// create a new `RefCache` with the given expiration `Duration`.
    ///
    pub fn new(ttl: Duration) -> Self {
        RefCache {
            inner: Lock::new(RefCacheData::new(ttl)),
        }
    }

    /// return a future that will attempt to insert the given [`Ref`]
    /// in the cache.
    ///
    /// # Errors
    ///
    /// there is no error possible yet.
    ///
    pub fn insert(
        &self,
        key: HeaderHash,
        value: Ref,
    ) -> impl Future<Item = (), Error = Infallible> {
        let mut inner = self.inner.clone();
        future::poll_fn(move || Ok(inner.poll_lock()))
            .map(move |mut guard| guard.insert(key, value))
    }

    /// return a future to get a [`Ref`] from the cache
    ///
    /// The future returns `None` if the `Ref` was not found in the
    /// cache. This does not mean the associated block is not in the
    /// blockchain storage. It only means it is not in the cache:
    /// it has not been seen _recently_.
    ///
    /// # Errors
    ///
    /// No error possible yet
    ///
    pub fn get(&self, key: HeaderHash) -> impl Future<Item = Option<Ref>, Error = Infallible> {
        let mut inner = self.inner.clone();

        future::poll_fn(move || Ok(inner.poll_lock()))
            .map(move |mut guard| guard.get(&key).cloned())
    }

    /// return a future to remove a specific [`Ref`] from the cache.
    ///
    pub fn remove(&self, key: HeaderHash) -> impl Future<Item = (), Error = Infallible> {
        let mut inner = self.inner.clone();

        future::poll_fn(move || Ok(inner.poll_lock())).map(move |mut guard| guard.remove(&key))
    }

    /// return a future that will remove every expired [`Ref`] from the cache
    ///
    pub fn purge(&self) -> impl Future<Item = (), Error = timer::Error> {
        let mut inner = self.inner.clone();

        future::poll_fn(move || Ok(inner.poll_lock()))
            .and_then(|mut guard| future::poll_fn(move || guard.poll_purge()))
    }
}

impl RefCacheData {
    fn new(ttl: Duration) -> Self {
        RefCacheData {
            entries: HashMap::new(),
            expirations: DelayQueue::new(),
            ttl,
        }
    }

    fn insert(&mut self, key: HeaderHash, value: Ref) {
        let delay = self.expirations.insert(key.clone(), self.ttl);

        self.entries.insert(key, (value, delay));
    }

    fn get(&mut self, key: &HeaderHash) -> Option<&Ref> {
        if let Some((v, k)) = self.entries.get(key) {
            self.expirations.reset(k, self.ttl);

            Some(v)
        } else {
            None
        }
    }

    fn remove(&mut self, key: &HeaderHash) {
        if let Some((_, cache_key)) = self.entries.remove(key) {
            self.expirations.remove(&cache_key);
        }
    }

    fn poll_purge(&mut self) -> Poll<(), timer::Error> {
        while let Some(entry) = try_ready!(self.expirations.poll()) {
            self.entries.remove(entry.get_ref());
        }

        Ok(Async::Ready(()))
    }
}
