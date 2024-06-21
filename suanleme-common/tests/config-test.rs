use fusen_rs::FusenApplicationContext;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Debug)]
pub struct AppCfg {
    pub server_port: u16,
    pub log: LogConfig,
    pub datasource: DatasourceConfig,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LogConfig {
    pub level: String,
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
    let nacos_config = suanleme_common::nacos::NacosConfig::builder()
        .server_addr("127.0.0.1:8848".to_owned())
        .app_name(Some("fusen-service".to_owned()))
        .build();
    let config = suanleme_common::nacos::NacosConfiguration::init_nacos_configuration(nacos_config)
        .await
        .unwrap();
    let config: AppCfg = config
        .get_config("suanlema-common", "DEFAULT_GROUP")
        .await
        .unwrap();
    println!("{:?}", config);
}
