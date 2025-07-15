use std::{collections::HashMap, time::Duration};

use suanleme_common::{
    log::LogConfig,
    redis::{init_redis_client, RedisConfig},
    tracing::{info, instrument},
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
    insert_executor().await;
    let mut redis = init_redis_client(&RedisConfig {
        db: 0,
        host: "127.0.0.1:6379".to_string(),
        username: None,
        password: None,
    })
    .await
    .unwrap();
    let _re = redis.set_hash("dasd", "dsds1", 4, -1).await;
    let re = redis.get_hash_field_ttl("dasd", "dsds1").await.unwrap();
    info!("{:?}", re);
    let _re = redis.set_hash("dasd", "dsds2", 2, 1000).await;
    let _re = redis.get_hash_all::<String>("dasd").await;
    let de = vec!["dsds2", "111"];
    let re = redis
        .get_hash_fields::<String, _>("dasd", de.as_slice())
        .await;
    info!("{:?}", re);
    let re = redis.get_hash_field::<String>("dasd", "111").await;
    info!("{:?}", re);
    let re = redis.get_hash_field::<String>("dasd", "dsds2").await;
    let re_c = redis.clone();
    // drop(redis);
    tokio::time::sleep(Duration::from_secs(10)).await;
    drop(re_c);
    tokio::time::sleep(Duration::from_secs(1200)).await;
    info!("{:?}", re);
    let mut hash = HashMap::new();
    hash.insert("String1".to_owned(), "k-0".to_owned());
    hash.insert("String2".to_owned(), "k-01".to_owned());
    hash.insert("String3".to_owned(), "k-02".to_owned());
    let result = redis
        .set_all_hash(&"ds".to_string(), &hash.iter().collect::<Vec<(_, _)>>(), 10)
        .await;
    println!("{result:?}");
    let result = redis
        .set_hash_xx("ds", "ewe1", "ewew7".to_string(), 9)
        .await;
    println!("{result:?}");
}

#[instrument(
    name = "CoinOrderMapper::insert_executor",
    fields(
        span_type = "DB:UPDATE",
        db.table = "coin_orders"
    )
)]
pub async fn insert_executor() {
    tokio::time::sleep(Duration::from_secs(1)).await;
    info!("dsds");
}
