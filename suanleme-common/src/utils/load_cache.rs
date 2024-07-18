use tokio::sync::RwLock;
pub struct LoadCache<T> {
    cache: RwLock<Option<T>>,
}

impl<T> Default for LoadCache<T> {
    fn default() -> Self {
        Self {
            cache: Default::default(),
        }
    }
}

impl<T: Clone> LoadCache<T> {
    pub async fn get(&self) -> T {
        self.cache.read().await.clone().unwrap()
    }

    pub async fn insert(&self, e: T) {
        let _ = self.cache.write().await.insert(e);
    }
}
