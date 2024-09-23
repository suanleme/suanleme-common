use redis::{aio::MultiplexedConnection, AsyncCommands, RedisError};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;
use std::str::FromStr;
use suanleme_macro::Data;
use tracing::{debug, instrument};

use crate::error::BoxError;

pub struct Lock {
    key: String,
    connect: MultiplexedConnection,
}

impl Drop for Lock {
    fn drop(&mut self) {
        let key = self.key.clone();
        let mut connect = self.connect.clone();
        tokio::spawn(async move {
            debug!("Release Lock : {}", key);
            let result: Result<i64, RedisError> = connect.del::<&str, i64>(&key).await;
            debug!("Release Lock Result: {} - {:?}", key, result);
        });
    }
}

#[derive(Debug, Serialize, Deserialize, Default, Data)]
pub struct RedisConfig {
    pub db: u16,
    pub host: String,
    pub username: Option<String>,
    pub password: Option<String>,
}

pub async fn init_redis_client(config: &RedisConfig) -> Result<RedisClient, RedisError> {
    let redis_url = if let Some(password) = &config.password {
        format!("redis://:{}@{}/{}", password, config.host, config.db)
    } else {
        format!("redis://{}/{}", config.host, config.db)
    };
    Ok(RedisClient {
        connect: redis::Client::open(redis_url)?
            .get_multiplexed_tokio_connection()
            .await?,
    })
}

#[derive(Clone, Debug)]
pub struct RedisClient {
    connect: MultiplexedConnection,
}

impl RedisClient {
    pub async fn get<T: DeserializeOwned>(&mut self, key: &str) -> Result<Option<T>, BoxError> {
        let value: Option<String> = self.get_str(key).await?;
        let Some(str) = value else {
            return Ok(None);
        };
        let value = Value::from_str(&str)?;
        T::deserialize(value)
            .map(|e| Some(e))
            .map_err(|e| e.to_string().into())
    }

    #[instrument(name = "redis get_str", fields(key))]
    pub async fn get_str(&mut self, key: &str) -> Result<Option<String>, BoxError> {
        self.connect.get(key).await.map_err(|e| e.into())
    }

    #[instrument(name = "redis set_ex", fields(key, value, seconds))]
    pub async fn set_ex(
        &mut self,
        key: &str,
        value: &str,
        seconds: i64,
    ) -> Result<String, BoxError> {
        if seconds < 0 {
            self.connect.set(key, value).await.map_err(|e| e.into())
        } else {
            self.connect
                .set_ex(key, value, seconds as u64)
                .await
                .map_err(|e| e.into())
        }
    }

    #[instrument(name = "redis set_nx_ex", fields(key, value, seconds))]
    pub async fn set_nx_ex(
        &mut self,
        key: &str,
        value: &str,
        seconds: u64,
    ) -> Result<String, BoxError> {
        let options = redis::SetOptions::default()
            .conditional_set(redis::ExistenceCheck::NX)
            .with_expiration(redis::SetExpiry::EX(seconds));
        self.connect
            .set_options(key, value, options)
            .await
            .map_err(|e| e.into())
    }

    #[instrument(name = "redis delete", fields(key))]
    pub async fn delete(&mut self, key: &str) -> Result<i64, BoxError> {
        self.connect.del(key).await.map_err(|e| e.into())
    }

    #[instrument(name = "redis get_lock", fields(key, seconds))]
    pub async fn get_lock(&mut self, key: &str, seconds: u64) -> Result<Lock, BoxError> {
        let result = self.set_nx_ex(key, "lock", seconds).await?;
        if !result.to_uppercase().contains("OK") {
            debug!("get lock error");
            return Err("redis response is not ok".into());
        };
        Ok(Lock {
            key: key.to_string(),
            connect: self.connect.clone(),
        })
    }
}
