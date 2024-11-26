use crate::{error::BoxError, shutdown::Shutdown};
use log::info;
use redis::{aio::ConnectionManager, cmd, AsyncCommands, RedisError};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;
use std::{collections::HashMap, fmt::Debug, str::FromStr, time::Duration};
use suanleme_macro::Data;
use tokio::sync::broadcast::{self, Sender};
use tracing::{debug, instrument};

pub struct Lock {
    key: String,
    connect: ConnectionManager,
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
    let connection_manager = redis::Client::open(redis_url)?
        .get_connection_manager()
        .await?;
    //进行心跳检测
    let mut connect_clone = connection_manager.clone();
    let (s, _) = broadcast::channel::<()>(1);
    let mut shutdown = Shutdown::new(s.subscribe());
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(60)) => {},
                _ = shutdown.recv() => {
                    info!("redis client close !");
                    return ;
                }
            }
            let result = cmd("PING").query_async::<String>(&mut connect_clone).await;
            debug!("redis client ping ~ : {:?}", result);
        }
    });
    Ok(RedisClient {
        connect: connection_manager,
        _ref: s,
    })
}

#[derive(Clone)]
pub struct RedisClient {
    connect: ConnectionManager,
    _ref: Sender<()>,
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

    #[instrument(name = "redis get_str", skip(self))]
    pub async fn get_str(&mut self, key: &str) -> Result<Option<String>, BoxError> {
        self.connect.get(key).await.map_err(|e| e.into())
    }

    #[instrument(name = "redis set_ex", skip(self))]
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

    #[instrument(name = "redis set_nx_ex", skip(self))]
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

    #[instrument(name = "redis delete", skip(self))]
    pub async fn delete(&mut self, key: &str) -> Result<i64, BoxError> {
        self.connect.del(key).await.map_err(|e| e.into())
    }

    #[instrument(name = "redis get_lock", skip(self))]
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

    const HSETEX: &str = r#"
        redis.call('HSET', KEYS[1], ARGV[1], ARGV[2])
        return redis.call('HEXPIRE', KEYS[1], ARGV[3], 'FIELDS', '1' , ARGV[1])
    "#;

    #[instrument(name = "redis set_hash", skip(self))]
    pub async fn set_hash<V>(
        &mut self,
        key: &str,
        field: &str,
        value: V,
        seconds: i64,
    ) -> Result<i64, BoxError>
    where
        V: redis::FromRedisValue
            + std::cmp::Eq
            + std::hash::Hash
            + redis::ToRedisArgs
            + std::marker::Send
            + std::marker::Sync
            + Debug,
    {
        if seconds >= 0 {
            redis::Script::new(Self::HSETEX)
                .key(key)
                .arg(field)
                .arg(value)
                .arg(seconds)
                .invoke_async::<Vec<i64>>(&mut self.connect)
                .await
                .map_err(|e| e.into())
                .map(|e| e.first().cloned().unwrap_or(0))
        } else {
            self.connect
                .hset(key, field, value)
                .await
                .map_err(|e| e.into())
        }
    }

    #[instrument(name = "redis get_hash_all", skip(self))]
    pub async fn get_hash_all<V>(&mut self, key: &str) -> Result<HashMap<String, V>, BoxError>
    where
        V: redis::FromRedisValue,
    {
        self.connect.hgetall(key).await.map_err(|e| e.into())
    }

    #[instrument(name = "redis get_hash_field", skip(self))]
    pub async fn get_hash_field<V>(&mut self, key: &str, field: &str) -> Result<Option<V>, BoxError>
    where
        V: redis::FromRedisValue,
    {
        self.connect.hget(key, field).await.map_err(|e| e.into())
    }
}
