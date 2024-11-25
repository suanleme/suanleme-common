use std::time::Duration;

use suanleme_common::redis::{init_redis_client, RedisConfig};

#[tokio::main]
async fn main() {
    let mut redis = init_redis_client(&RedisConfig {
        db: 0,
        host: "127.0.0.1:6379".to_string(),
        username: None,
        password: None,
    })
    .await
    .unwrap();
    let re = redis.set_hash("dasd", "dsds", 4, 0).await;
    println!("{:?}", re);
    let _re = redis.set_hash("dasd", "dsds2", 2, 10).await;
    let _re = redis.get_hash_all::<String>("dasd").await;
    // let re = redis.get_hash_field::<String>("dasd","dsds2").await;
    tokio::time::sleep(Duration::from_secs(30)).await;
    let re_c = redis.clone();
    drop(redis);
    tokio::time::sleep(Duration::from_secs(30)).await;
    drop(re_c);
    tokio::time::sleep(Duration::from_secs(30)).await;
    println!("{:?}", re);
}
