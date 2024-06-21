use std::sync::Arc;

use nacos_sdk::api::{
    config::{ConfigService, ConfigServiceBuilder},
    error::Error,
    props::ClientProps,
};
use suanleme_macro::builder;

use crate::{
    config::{toml::get_toml_by_context, yaml::get_yaml_by_context},
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
        match config_response.content_type().as_str() {
            "toml" => get_toml_by_context(config_response.content()),
            "yaml" => get_yaml_by_context(config_response.content()),
            _type => Err(format!("not support {:?}", _type).into()),
        }
    }
}
