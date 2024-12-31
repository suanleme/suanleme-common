use std::collections::HashMap;
use std::hash::Hash;
use std::marker::PhantomData;
use tokio::sync::mpsc::{self, UnboundedSender};
use tokio::sync::oneshot;

use crate::error::BoxError;

enum CacheSender<K, V> {
    Get(K),
    Insert((K, V)),
    Remove(K),
    Clone,
}

enum CacheReceiver<K, V> {
    Get(Option<V>),
    Insert(Option<V>),
    Remove(Option<V>),
    Clone(HashMap<K, V>),
}
type AsyncMapSender<K, V> =
    UnboundedSender<(CacheSender<K, V>, oneshot::Sender<CacheReceiver<K, V>>)>;

#[derive(Clone)]
pub struct AsyncMap<K, V> {
    _k: PhantomData<K>,
    _v: PhantomData<V>,
    sender: AsyncMapSender<K, V>,
}

impl<K, V> Default for AsyncMap<K, V>
where
    K: Hash + Eq + std::marker::Send + Sync + 'static + Clone,
    V: std::marker::Send + Sync + 'static + Clone,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<K, V> AsyncMap<K, V>
where
    K: Hash + Eq + std::marker::Send + Sync + 'static + Clone,
    V: std::marker::Send + Sync + 'static + Clone,
{
    pub fn new() -> Self {
        let (sender, mut receiver) =
            mpsc::unbounded_channel::<(CacheSender<K, V>, oneshot::Sender<CacheReceiver<K, V>>)>();
        tokio::spawn(async move {
            let mut map = HashMap::<K, V>::new();
            while let Some(msg) = receiver.recv().await {
                match msg.0 {
                    CacheSender::Get(key) => {
                        let _ = msg.1.send(CacheReceiver::Get(map.get(&key).cloned()));
                    }
                    CacheSender::Insert((key, value)) => {
                        let value = map.insert(key, value);
                        let _ = msg.1.send(CacheReceiver::Insert(value));
                    }
                    CacheSender::Remove(key) => {
                        let value = map.remove(&key);
                        let _ = msg.1.send(CacheReceiver::Remove(value));
                    }
                    CacheSender::Clone => {
                        let value = map.clone();
                        let _ = msg.1.send(CacheReceiver::Clone(value));
                    }
                }
            }
        });
        Self {
            _k: PhantomData,
            _v: PhantomData,
            sender,
        }
    }

    pub async fn get(&self, key: K) -> Result<Option<V>, BoxError> {
        let oneshot = oneshot::channel();
        let _ = self.sender.send((CacheSender::Get(key), oneshot.0));
        match oneshot.1.await? {
            CacheReceiver::Get(value) => Ok(value),
            _ => Err("err receiver".into()),
        }
    }

    pub async fn insert(&self, key: K, value: V) -> Result<Option<V>, BoxError> {
        let oneshot = oneshot::channel();
        let _ = self
            .sender
            .send((CacheSender::Insert((key, value)), oneshot.0));
        match oneshot.1.await? {
            CacheReceiver::Insert(value) => Ok(value),
            _ => Err("err receiver".into()),
        }
    }

    pub async fn remove(&self, key: K) -> Result<Option<V>, BoxError> {
        let oneshot = oneshot::channel();
        let _ = self.sender.send((CacheSender::Remove(key), oneshot.0));
        match oneshot.1.await? {
            CacheReceiver::Remove(value) => Ok(value),
            _ => Err("err receiver".into()),
        }
    }

    pub async fn map_clone(&self) -> Result<HashMap<K, V>, BoxError> {
        let oneshot = oneshot::channel();
        let _ = self.sender.send((CacheSender::Clone, oneshot.0));
        match oneshot.1.await? {
            CacheReceiver::Clone(value) => Ok(value),
            _ => Err("err receiver".into()),
        }
    }
}
