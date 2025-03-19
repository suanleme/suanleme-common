use crate::error::BoxError;
use std::marker::PhantomData;
use tokio::sync::mpsc::{self, UnboundedSender};
use tokio::sync::oneshot;

enum CacheSender<V> {
    Get,
    Insert(V),
    Clean,
}

enum CacheReceiver<V> {
    Get(Option<V>),
    Insert(Option<V>),
    Clean(Option<V>),
}
type AsyncCacheSender<V> = UnboundedSender<(CacheSender<V>, oneshot::Sender<CacheReceiver<V>>)>;

#[derive(Clone)]
pub struct AsyncCache<V> {
    _v: PhantomData<V>,
    sender: AsyncCacheSender<V>,
}

impl<V> Default for AsyncCache<V>
where
    V: std::marker::Send + Sync + 'static + Clone,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<V> AsyncCache<V>
where
    V: std::marker::Send + Sync + 'static + Clone,
{
    pub fn new() -> Self {
        let (sender, mut receiver) =
            mpsc::unbounded_channel::<(CacheSender<V>, oneshot::Sender<CacheReceiver<V>>)>();
        tokio::spawn(async move {
            let mut cache = None;
            while let Some(msg) = receiver.recv().await {
                match msg.0 {
                    CacheSender::Get => {
                        let _ = msg.1.send(CacheReceiver::Get(cache.as_ref().cloned()));
                    }
                    CacheSender::Insert(value) => {
                        let old_value = cache.replace(value);
                        let _ = msg.1.send(CacheReceiver::Insert(old_value));
                    }
                    CacheSender::Clean => {
                        let old_value = cache.take();
                        let _ = msg.1.send(CacheReceiver::Clean(old_value));
                    }
                }
            }
        });
        Self {
            _v: PhantomData,
            sender,
        }
    }

    pub async fn get(&self) -> Result<Option<V>, BoxError> {
        let oneshot = oneshot::channel();
        let _ = self.sender.send((CacheSender::Get, oneshot.0));
        match oneshot.1.await? {
            CacheReceiver::Get(value) => Ok(value),
            _ => Err("err receiver".into()),
        }
    }

    pub async fn insert(&self, value: V) -> Result<Option<V>, BoxError> {
        let oneshot = oneshot::channel();
        let _ = self.sender.send((CacheSender::Insert(value), oneshot.0));
        match oneshot.1.await? {
            CacheReceiver::Insert(value) => Ok(value),
            _ => Err("err receiver".into()),
        }
    }

    pub async fn remove(&self) -> Result<Option<V>, BoxError> {
        let oneshot = oneshot::channel();
        let _ = self.sender.send((CacheSender::Clean, oneshot.0));
        match oneshot.1.await? {
            CacheReceiver::Clean(value) => Ok(value),
            _ => Err("err receiver".into()),
        }
    }
}
