use serde::{Deserialize, Serialize};
use std::{sync::Arc, time::Duration};
use suanleme_common::{
    config::HotConfig,
    log::LogConfig,
    redis::{init_redis_client, RedisConfig},
};
use suanleme_macro::hot_config;
use tracing::info;

#[hot_config]
pub struct AppCfg {
    pub server_port: u16,
    pub datasource: DatasourceConfig,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DatasourceConfig {
    pub suanleme_db: SuanlemeDb,
    pub pool: PoolConfig,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SuanlemeDb {
    pub url: String,
    pub username: Option<String>,
    pub password: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PoolConfig {
    pub max_connections: Option<u32>,
    pub min_connections: Option<u32>,
    pub connect_timeout: Option<u64>,
    pub acquire_timeout: Option<u64>,
    pub idle_timeout: Option<u64>,
    pub max_lifetime: Option<u64>,
    pub sqlx_logging: Option<bool>,
    pub sqlx_logging_level: Option<String>,
}

#[tokio::test]
async fn test() {
    suanleme_common::log::init_log(
        &LogConfig::default()
            .level("info".to_owned())
            .path("/Users/kwsc98/Desktop/workspace/gitlab/suanleme-common/log".to_owned())
            .devmode(Some(true)),
        "suanleme-common",
    );
    let nacos_config = suanleme_common::nacos::NacosConfig::builder()
        .server_addr("127.0.0.1:8848".to_owned())
        .app_name(Some("fusen-service".to_owned()))
        .build();
    let nacos_config = suanleme_common::nacos::NacosConfiguration::init_nacos_configuration(
        Arc::new(nacos_config),
    )
    .await
    .unwrap();
    //只需要加载一次配置的话使用get_config即可
    let config1: AppCfg = nacos_config
        .get_config("suanlema-common:DEFAULT_GROUP")
        .await
        .unwrap();
    info!("{:?}", config1);
    //需要进行热配置读取的话,则使用get_hot_config即可
    let config2: AppCfg = nacos_config
        .get_hot_config("suanlema-common:DEFAULT_GROUP")
        .await
        .unwrap();
    loop {
        info!("{:?}", config2.get_hot_config().await);
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

#[tokio::test]
async fn test2() {
    let mut redis_client =
        init_redis_client(&RedisConfig::default().host("127.0.0.1:6379".to_owned()))
            .await
            .unwrap();
    let lock = redis_client.get_lock("key", 60).await.unwrap();
    let _ = tokio::time::sleep(Duration::from_secs(10)).await;
    drop(lock);
    let _ = tokio::time::sleep(Duration::from_secs(1)).await;
}
