use serde::{Deserialize, Serialize};
use std::{sync::Arc, time::Duration};
use suanleme_common::{config::HotConfig, log::LogConfig};
use suanleme_macro::hot_config;
use tracing::{debug, debug_span, info};

#[hot_config]
pub struct AppCfg {
    pub server_port: u16,
    pub datasource: DatasourceConfig,
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

#[tokio::test]
async fn test() {
    let _logwork = suanleme_common::log::init_log(
        &LogConfig::default()
            .env_filter(Some("config_test=debug".to_owned()))
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
    //只需要加载一次配置的话使用get_config即可
    let config1: AppCfg = nacos_config
        .get_config("suanlema-common:DEFAULT_GROUP")
        .await
        .unwrap();
    debug!("{:?}", config1);
    //需要进行热配置读取的话,则使用get_hot_config即可
    let config2: AppCfg = nacos_config
        .get_hot_config("suanlema-common:DEFAULT_GROUP")
        .await
        .unwrap();
    debug!("{:?}", config1);
    drop(_enter);
    drop(span);
    loop {
        info!("{:?}", config2.get_hot_config().await);
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

// #[tokio::test]
// async fn test2() {
//     let mut redis_client =
//         init_redis_client(&RedisConfig::default().host("127.0.0.1:6379".to_owned()))
//             .await
//             .unwrap();
//     let lock = redis_client.get_lock("key", 60).await.unwrap();
//     let _ = tokio::time::sleep(Duration::from_secs(10)).await;
//     drop(lock);
//     let _ = tokio::time::sleep(Duration::from_secs(1)).await;
// }

// #[derive(Default, Data, StrategyDebug)]
// pub struct MyTest<T> {
//     #[strategy(limit = 5)]
//     str1: String,
//     str2: String,
//     #[strategy(limit = 20)]
//     resd: T,
// }

// #[derive(Default, Data, StrategyDebug)]
// pub struct MyPoi {
//     str3: String,
//     str4: String,
// }

// #[test]
// fn test3() {
//     println!(
//         "{:?}",
//         MyTest::default()
//             .str1("12".to_owned())
//             .str2("str2".to_owned())
//             .resd(
//                 MyPoi::default()
//                     .str3("str3".to_owned())
//                     .str4("str4".to_owned())
//             )
//     );
// }
