use redis::{aio::MultiplexedConnection, AsyncCommands, RedisError};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;
use std::str::FromStr;

use crate::error::BoxError;

#[derive(Debug, Serialize, Deserialize)]
pub struct RedisConfig {
    pub db: u16,
    pub host: String,
    pub username: Option<String>,
    pub password: Option<String>,
}

pub async fn init_redis_pool(config: &RedisConfig) -> Result<MultiplexedConnection, RedisError> {
    let redis_url = if let Some(password) = &config.password {
        format!("redis://:{}@{}/{}", password, config.host, config.db)
    } else {
        format!("redis://{}/{}", config.host, config.db)
    };
    redis::Client::open(redis_url)?
        .get_multiplexed_tokio_connection()
        .await
}

#[derive(Clone)]
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

    pub async fn get_str(&mut self, key: &str) -> Result<Option<String>, BoxError> {
        self.connect.get(key).await.map_err(|e| e.into())
    }

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
}
