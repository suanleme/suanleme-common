//! Run with `cargo test -p suanleme-common --test redis -- --ignored`.
//!
//! The runner must create a disposable Redis, set `suanleme-common:test-owner` to
//! its UUID, and provide SUANLEME_COMMON_TEST_REDIS_ADDR / SUANLEME_COMMON_TEST_REDIS_OWNER.
//! The address must be loopback on a non-default port; the hash test requires HEXPIRE.
//! The runner removes its owned instance even if a test is interrupted.

use std::net::SocketAddr;
use std::time::Duration;

use redis::AsyncCommands;
use suanleme_common::error::BoxError;
use suanleme_common::redis::{self as common_redis, RedisClient, RedisConfig};
use suanleme_common::utils::token;
use uuid::Uuid;

const OWNER_MARKER: &str = "suanleme-common:test-owner";

fn owned_config(address: &str, owner: &str) -> Result<RedisConfig, BoxError> {
    let address: SocketAddr = address.parse()?;
    let owner = Uuid::parse_str(owner)?;
    if !address.ip().is_loopback()
        || address.port() == 6379
        || address.port() == 0
        || owner.is_nil()
    {
        return Err(
            "requires an owned loopback Redis on a non-default port and a UUID owner".into(),
        );
    }
    Ok(RedisConfig {
        host: address.to_string(),
        db: 0,
        ..Default::default()
    })
}

async fn owned_client() -> Result<RedisClient, BoxError> {
    let address = std::env::var("SUANLEME_COMMON_TEST_REDIS_ADDR")?;
    let owner = std::env::var("SUANLEME_COMMON_TEST_REDIS_OWNER")?;
    let config = owned_config(&address, &owner)?;
    let client = redis::Client::open(format!("redis://{}/0", config.host))?;
    let mut connection = tokio::time::timeout(
        Duration::from_secs(5),
        client.get_multiplexed_async_connection(),
    )
    .await??;
    let marker: Option<String> =
        tokio::time::timeout(Duration::from_secs(5), connection.get(OWNER_MARKER)).await??;
    if marker.as_deref() != Some(owner.as_str()) {
        return Err("Redis owner marker does not match; refusing test writes".into());
    }
    tokio::time::timeout(
        Duration::from_secs(5),
        common_redis::init_redis_client(&config),
    )
    .await?
    .map_err(Into::into)
}

async fn cleanup(client: &mut RedisClient, keys: &[String]) -> Result<(), BoxError> {
    // Every key was registered before the worker started; never scan or flush a database.
    for key in keys {
        client.delete(key).await?;
        assert_eq!(client.get_ttl(key).await?, None, "test key was not removed");
    }
    Ok(())
}

#[test]
fn owned_address_rejects_shared_or_invalid_targets() {
    let owner = Uuid::new_v4().to_string();
    for address in [
        "127.0.0.1:6379",
        "0.0.0.0:15000",
        "192.0.2.1:15000",
        "127.0.0.1:0",
    ] {
        assert!(owned_config(address, &owner).is_err(), "{address}");
    }
    assert!(owned_config("127.0.0.1:15000", "").is_err());
    assert!(owned_config("127.0.0.1:15000", &Uuid::nil().to_string()).is_err());
    assert!(owned_config("127.0.0.1:15000", &owner).is_ok());
}

#[tokio::test]
#[ignore = "requires SUANLEME_COMMON_TEST_REDIS_ADDR, UUID OWNER and matching owned Redis marker"]
async fn string_roundtrips_are_awaited_and_cleaned() -> Result<(), BoxError> {
    let mut client = owned_client().await?;
    let prefix = format!("suanleme-common:test:{}:", Uuid::new_v4());
    let keys: Vec<_> = (0..100).map(|index| format!("{prefix}{index}")).collect();
    let worker_keys = keys.clone();
    let mut worker_client = client.clone();
    let worker = tokio::spawn(async move {
        for key in worker_keys {
            let value = token::get_token();
            assert_eq!(worker_client.set_ex(&key, &value, -1).await?, "OK");
            assert_eq!(worker_client.get_str(&key).await?, Some(value));
            assert_eq!(worker_client.get_ttl(&key).await?, Some(None));
        }
        Ok::<_, BoxError>(())
    });
    let result = worker.await;
    cleanup(&mut client, &keys).await?;
    result??;
    Ok(())
}

#[tokio::test]
#[ignore = "requires an owned Redis with HEXPIRE and SUANLEME_COMMON_TEST_REDIS_ADDR / UUID OWNER"]
async fn hash_field_expiration_is_checked_and_cleaned() -> Result<(), BoxError> {
    let mut client = owned_client().await?;
    let keys = vec![format!("suanleme-common:test:{}:hash", Uuid::new_v4())];
    let key = keys[0].clone();
    let mut worker_client = client.clone();
    let worker = tokio::spawn(async move {
        assert_eq!(worker_client.set_hash(&key, "removed", 4, 0).await?, 2);
        assert_eq!(worker_client.set_hash(&key, "kept", 2, 30).await?, 1);
        let fields = worker_client.get_hash_all::<i64>(&key).await?;
        assert_eq!(fields.len(), 1);
        assert_eq!(fields.get("kept"), Some(&2));
        assert_eq!(fields.get("removed"), None);
        assert_eq!(worker_client.set_hash(&key, "kept", 2, 0).await?, 2);
        assert!(worker_client.get_hash_all::<i64>(&key).await?.is_empty());
        Ok::<_, BoxError>(())
    });
    let result = worker.await;
    cleanup(&mut client, &keys).await?;
    result??;
    Ok(())
}
