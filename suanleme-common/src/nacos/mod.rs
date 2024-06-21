use std::sync::Arc;

use nacos_sdk::api::{
    config::{ConfigChangeListener, ConfigResponse, ConfigService, ConfigServiceBuilder},
    error::Error,
    props::ClientProps,
};
use suanleme_macro::builder;
use tokio::sync::mpsc;
use tracing::{error, info};

use crate::{
    config::{toml::get_toml_by_context, yaml::get_yaml_by_context, HotConfig},
    error::BoxError,
};

#[builder]
pub struct NacosConfig {
    server_addr: String,
    namespace: String,
    group: Option<String>,
    app_name: Option<String>,
    username: String,
    password: String,
}

#[derive(Clone)]
pub struct NacosConfiguration {
    config_service: Arc<Box<dyn ConfigService>>,
    _config: Arc<NacosConfig>,
}

impl NacosConfiguration {
    pub async fn init_nacos_configuration(
        config: NacosConfig,
    ) -> Result<NacosConfiguration, Error> {
        let mut client_props = ClientProps::new();
        let app_name = config
            .app_name
            .as_ref()
            .map_or("service".to_owned(), |e| e.to_owned());
        client_props = client_props
            .server_addr(config.server_addr.clone())
            .namespace(config.namespace.clone())
            .app_name(app_name.clone())
            .auth_username(config.username.clone())
            .auth_password(config.password.clone());
        let builder = ConfigServiceBuilder::new(client_props);
        let builder = if !config.username.is_empty() {
            builder.enable_auth_plugin_http()
        } else {
            builder
        };
        Ok(NacosConfiguration {
            config_service: Arc::new(Box::new(builder.build()?)),
            _config: Arc::new(config),
        })
    }

    pub async fn get_config<'a, T: serde::Deserialize<'a>>(
        &self,
        data_id: &str,
        group: &str,
    ) -> Result<T, BoxError> {
        let config_response = self
            .config_service
            .get_config(data_id.to_owned(), group.to_owned())
            .await?;
        NacosConfiguration::config_build(config_response)
    }

    pub fn config_build<'a, T: serde::Deserialize<'a>>(
        config_response: ConfigResponse,
    ) -> Result<T, BoxError> {
        match config_response.content_type().as_str() {
            "toml" => get_toml_by_context(config_response.content()),
            "yaml" => get_yaml_by_context(config_response.content()),
            _type => Err(format!("not support {:?}", _type).into()),
        }
    }

    pub async fn get_hot_config<'a, T: serde::Deserialize<'a> + HotConfig>(
        &self,
        data_id: &str,
        group: &str,
    ) -> Result<T, BoxError> {
        let temp_ident: T = self.get_config(data_id, group).await?;
        let mut ident: T = self.get_config(data_id, group).await?;
        let (config_listener, receiver) = HotConfigChangeListener::new();
        ident.build_hot_config(temp_ident, receiver)?;
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

struct HotConfigChangeListener {
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
