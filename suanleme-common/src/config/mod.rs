use std::sync::Arc;

pub mod toml;
pub mod yaml;

pub trait HotConfig {
    fn build_hot_config(
        &mut self,
        ident: Self,
        listener: tokio::sync::mpsc::Receiver<nacos_sdk::api::config::ConfigResponse>,
    ) -> Result<(), crate::error::BoxError>;
    
    #[allow(async_fn_in_trait)]
    async fn get_hot_config(&self) -> Option<Arc<Self>>;
}
