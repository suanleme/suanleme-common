use std::{collections::HashMap, time::Duration};

use suanleme_common::{
    log::LogConfig,
    redis::{init_redis_client, RedisConfig},
    tracing::info,
};

#[tokio::main]
async fn main() {
    let _logwork = suanleme_common::log::init_log(
        &LogConfig::default()
            .env_filter(Some("test=debug,suanleme_common=debug".to_owned()))
            .path(Some(
                "/Users/kwsc98/Desktop/workspace/suanleme-common/log".to_owned(),
            ))
            .endpoint(Some("http://127.0.0.1:4317".to_owned()))
            .devmode(Some(true)),
        "suanleme-common4",
    );
    let mut redis = init_redis_client(&RedisConfig {
        db: 0,
        host: "127.0.0.1:6379".to_string(),
        username: None,
        password: None,
    })
    .await
    .unwrap();
    let _re = redis.set_hash("dasd", "dsds1", 4, -1).await;
    let re  = redis.get_hash_field_ttl("dasd", "dsds1").await.unwrap();
    info!("{:?}", re);
    let _re = redis.set_hash("dasd", "dsds2", 2, 10).await;
    let _re = redis.get_hash_all::<String>("dasd").await;
    let re = redis.del_hash_field("dasd","dsds2").await;
    info!("{:?}", re);
    let re = redis.get_hash_field::<String>("dasd","dsds2").await;
    let re_c = redis.clone();
    // drop(redis);
    tokio::time::sleep(Duration::from_secs(10)).await;
    drop(re_c);
    tokio::time::sleep(Duration::from_secs(1200)).await;
    info!("{:?}", re);
    let mut hash = HashMap::new();
    hash.insert("k", "k-0");
    hash.insert("k1", "k-01");
    hash.insert("k2", "k-02");
    let result = redis
        .set_all_hash("key1", &hash.into_iter().collect::<Vec<(&str, &str)>>(), 10)
        .await;
    println!("{:?}", result);
}
