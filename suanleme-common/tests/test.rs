use serde::{Deserialize, Serialize};
use std::sync::Arc;
use suanleme_common::config::HotConfig;
use suanleme_common::log::LogConfig;
use suanleme_macro::{hot_config, Data, StrategyDebug};
use tracing::{debug, debug_span, info};

#[hot_config]
pub struct AppCfg {
    pub server_port: u16,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DatasourceConfig {
    pub suanleme_db: SuanlemeDb,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SuanlemeDb {
    pub url: String,
    pub username: Option<String>,
    pub password: Option<String>,
}

#[tokio::main]
async fn main() {
    let _logwork = suanleme_common::log::init_log(
        &LogConfig::default()
            .env_filter(None)
            .path(Some(
                "/Users/kwsc98/Desktop/workspace/gitlab/suanleme-common/log".to_owned(),
            ))
            .endpoint(Some("http://127.0.0.1:4317".to_owned()))
            .devmode(Some(true)),
        "suanleme-common4",
    );
    let span = debug_span!("trace_span", id = "1221").entered();
    let _enter = span.enter();
    let nacos_config = suanleme_common::nacos::NacosConfig::default()
        .server_addr("127.0.0.1:8848".to_owned())
        .app_name(Some("fusen-service".to_owned()));
    let nacos_config = suanleme_common::nacos::NacosConfiguration::init_nacos_configuration(
        Arc::new(nacos_config),
    )
    .await
    .unwrap();
    // //只需要加载一次配置的话使用get_config即可
    // let config1: AppCfg = nacos_config
    //     .get_config("suanlema-common:DEFAULT_GROUP")
    //     .await
    //     .unwrap();
    debug!("{:?}", 111);
    //需要进行热配置读取的话,则使用get_hot_config即可
    // let config2: AppCfg = nacos_config
    //     .get_hot_config("suanlema-common:DEFAULT_GROUP")
    //     .await
    //     .unwrap();
    drop(_enter);
    drop(span);
    // loop {
    //     info!("{:?}", config2.get_hot_config().await);
    //     tokio::time::sleep(Duration::from_secs(2)).await;
    // }
}

#[derive(StrategyDebug, Default, Data)]
struct UserInfo {
    #[strategy(mask)]
    username: String,
    #[strategy(limit = 5)]
    phone: String,
    #[strategy(ignore)]
    password: String,
}

#[test]
fn test3() {
    println!(
        "{:?}",
        UserInfo::default()
            .username("后的为阿升升升升但污".to_owned())
            .phone("你是一个是为".to_owned())
            .password("qwer1234".to_owned())
    );
}
