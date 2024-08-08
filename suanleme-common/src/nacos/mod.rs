use std::sync::Arc;

use nacos_sdk::api::{
    config::{ConfigChangeListener, ConfigResponse, ConfigService, ConfigServiceBuilder},
    error::Error,
    props::ClientProps,
};
use serde::{Deserialize, Serialize};
use suanleme_macro::Data;
use tokio::sync::mpsc;
use tracing::{error, info};

use crate::{
    config::{toml::get_toml_by_context, yaml::get_yaml_by_context, HotConfig},
    error::BoxError,
};

#[derive(Serialize, Deserialize, Debug, Clone, Data, Default)]
pub struct NacosConfig {
    pub server_addr: String,
    pub namespace: Option<String>,
    pub group: Option<String>,
    pub app_name: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
}

#[derive(Clone)]
pub struct NacosConfiguration {
    config_service: Arc<Box<dyn ConfigService>>,
    _config: Arc<NacosConfig>,
}

impl NacosConfiguration {
    pub async fn init_nacos_configuration(
        config: Arc<NacosConfig>,
    ) -> Result<NacosConfiguration, Error> {
        let mut client_props = ClientProps::new();
        let app_name = config
            .app_name
            .as_ref()
            .map_or("service".to_owned(), |e| e.to_owned());
        client_props = client_props
            .server_addr(config.server_addr.clone())
            .namespace(
                config
                    .namespace
                    .as_ref()
                    .map_or(Default::default(), |e| e.clone()),
            )
            .app_name(app_name.clone())
            .auth_username(
                config
                    .username
                    .as_ref()
                    .map_or(Default::default(), |e| e.clone()),
            )
            .auth_password(
                config
                    .password
                    .as_ref()
                    .map_or(Default::default(), |e| e.clone()),
            );
        let builder = ConfigServiceBuilder::new(client_props);
        let builder = if config.username.is_some() {
            builder.enable_auth_plugin_http()
        } else {
            builder
        };
        Ok(NacosConfiguration {
            config_service: Arc::new(Box::new(builder.build()?)),
            _config: config,
        })
    }

    pub async fn get_config<T: serde::de::DeserializeOwned>(
        &self,
        config: &str,
    ) -> Result<T, BoxError> {
        let config: Vec<&str> = config.split(':').collect();
        let data_id = config[0];
        let group = config[1];
        let config_response = self
            .config_service
            .get_config(data_id.to_owned(), group.to_owned())
            .await?;
        NacosConfiguration::config_build(config_response)
    }

    pub async fn get_receive_config<T: serde::de::DeserializeOwned + Send + 'static>(
        &self,
        config: &str,
    ) -> Result<mpsc::Receiver<T>, BoxError> {
        let ident: T = self.get_config(config).await?;
        let config: Vec<&str> = config.split(':').collect();
        let data_id = config[0];
        let group = config[1];
        let (sender, receiver) = mpsc::channel(1);
        sender
            .send(ident)
            .await
            .map_err(|e| format!("get_receive_config error : {}", e))?;
        let (config_listener, mut listener) = HotConfigChangeListener::new();
        self.config_service
            .add_listener(
                data_id.to_owned(),
                group.to_owned(),
                Arc::new(config_listener),
            )
            .await?;
        tokio::spawn(async move {
            while let Some(response) = listener.recv().await {
                if let Ok(ident) = NacosConfiguration::config_build::<T>(response) {
                    let _ = sender.send(ident).await;
                }
            }
        });
        Ok(receiver)
    }

    pub fn config_build<T: serde::de::DeserializeOwned>(
        config_response: ConfigResponse,
    ) -> Result<T, BoxError> {
        match config_response.content_type().as_str() {
            "toml" => get_toml_by_context(config_response.content()),
            "yaml" => get_yaml_by_context(config_response.content()),
            _type => Err(format!("not support {:?}", _type).into()),
        }
    }

    pub async fn get_hot_config<T: serde::de::DeserializeOwned + HotConfig>(
        &self,
        config: &str,
    ) -> Result<T, BoxError> {
        let temp_ident: T = self.get_config(config).await?;
        let mut ident: T = self.get_config(config).await?;
        let (config_listener, receiver) = HotConfigChangeListener::new();
        ident.build_hot_config(temp_ident, receiver)?;
        let config: Vec<&str> = config.split(':').collect();
        let data_id = config[0];
        let group = config[1];
        self.config_service
            .add_listener(
                data_id.to_owned(),
                group.to_owned(),
                Arc::new(config_listener),
            )
            .await?;
        Ok(ident)
    }
}

pub struct HotConfigChangeListener {
    sender: mpsc::Sender<nacos_sdk::api::config::ConfigResponse>,
}
impl HotConfigChangeListener {
    pub fn new() -> (Self, mpsc::Receiver<nacos_sdk::api::config::ConfigResponse>) {
        let (sender, receiver) = mpsc::channel(1);
        (Self { sender }, receiver)
    }
}

impl ConfigChangeListener for HotConfigChangeListener {
    fn notify(&self, config_resp: nacos_sdk::api::config::ConfigResponse) {
        let sender = self.sender.clone();
        tokio::spawn(async move {
            info!("Listener ConfigResponse Change : {}", config_resp);
            if let Err(error) = sender.send(config_resp).await {
                error!("listener error : {}", error);
            }
        });
    }
}
