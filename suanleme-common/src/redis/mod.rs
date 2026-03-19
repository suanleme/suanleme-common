use crate::{error::BoxError, shutdown::Shutdown};
use redis::{aio::ConnectionManager, cmd, AsyncCommands, RedisError, ToRedisArgs};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;
use std::{collections::HashMap, fmt::Debug, str::FromStr, time::Duration};
use suanleme_macro::Data;
use tokio::sync::broadcast::{self, Sender};
use tracing::{debug, info, instrument};

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

    pub async fn search_keys(&mut self, pattern: &str) -> Result<Vec<String>, BoxError> {
        let mut keys = Vec::new();
        let mut iter = self.connect.scan_match(pattern).await?;
        while let Some(key) = iter.next_item().await {
            keys.push(key);
        }
        Ok(keys)
    }

    #[instrument(name = "redis get_str", skip(self))]
    pub async fn get_str(&mut self, key: &str) -> Result<Option<String>, RedisError> {
        self.connect.get(key).await
    }

    #[instrument(name = "redis get_ttl", skip(self))]
    pub async fn get_ttl(&mut self, key: &str) -> Result<Option<Option<i64>>, RedisError> {
        let result: i64 = self.connect.ttl(key).await?;
        Ok(match result {
            -2 => None,
            -1 => Some(None),
            other => Some(Some(other)),
        })
    }

    #[instrument(name = "redis set_ex", skip(self))]
    pub async fn set_ex(
        &mut self,
        key: &str,
        value: &str,
        seconds: i64,
    ) -> Result<String, RedisError> {
        if seconds < 0 {
            self.connect.set(key, value).await
        } else {
            self.connect.set_ex(key, value, seconds as u64).await
        }
    }

    #[instrument(name = "redis set_nx_ex", skip(self))]
    pub async fn set_nx_ex(
        &mut self,
        key: &str,
        value: &str,
        seconds: u64,
    ) -> Result<String, RedisError> {
        let options = redis::SetOptions::default()
            .conditional_set(redis::ExistenceCheck::NX)
            .with_expiration(redis::SetExpiry::EX(seconds));
        self.connect.set_options(key, value, options).await
    }

    #[instrument(name = "redis delete", skip(self))]
    pub async fn delete(&mut self, key: &str) -> Result<i64, RedisError> {
        self.connect.del(key).await
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
    ) -> Result<i64, RedisError>
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
                .map(|e| e.first().cloned().unwrap_or(0))
        } else {
            self.connect.hset(key, field, value).await
        }
    }

    const HSETNXEX: &str = r#"
    if redis.call('EXISTS', KEYS[1]) == 1 then 
        redis.call('HSET', KEYS[1], ARGV[1], ARGV[2])
        return redis.call('HEXPIRE', KEYS[1], ARGV[3], 'FIELDS', '1' , ARGV[1])
    else 
       return 0
    end
    "#;

    const HSETNX: &str = r#"
    if redis.call('EXISTS', KEYS[1]) == 1 then 
        redis.call('HSET', KEYS[1], ARGV[1], ARGV[2])
        return 1
    else 
       return 0
    end
    "#;

    #[instrument(name = "redis set_hash_xx", skip(self))]
    pub async fn set_hash_xx<K, F, V>(
        &mut self,
        key: K,
        field: F,
        value: V,
        seconds: i64,
    ) -> Result<i64, RedisError>
    where
        K: redis::ToRedisArgs + Debug,
        F: redis::ToRedisArgs + Debug,
        V: redis::ToRedisArgs + Debug,
    {
        if seconds >= 0 {
            redis::Script::new(Self::HSETNXEX)
                .key(key)
                .arg(field)
                .arg(value)
                .arg(seconds)
                .invoke_async::<Vec<i64>>(&mut self.connect)
                .await
                .map(|e| e.first().cloned().unwrap_or(0))
        } else {
            redis::Script::new(Self::HSETNX)
                .key(key)
                .arg(field)
                .arg(value)
                .invoke_async::<i64>(&mut self.connect)
                .await
        }
    }

    #[instrument(name = "redis set_all_hash", skip(self))]
    pub async fn set_all_hash<K, F, V>(
        &mut self,
        key: K,
        items: &Vec<(F, V)>,
        seconds: i64,
    ) -> Result<(), BoxError>
    where
        K: redis::ToRedisArgs + Debug + std::marker::Sync + std::marker::Send + std::marker::Copy,
        F: redis::ToRedisArgs + Debug + std::marker::Sync + std::marker::Send,
        V: redis::ToRedisArgs + Debug + std::marker::Sync + std::marker::Send,
    {
        if seconds < 0 {
            let result: String = self.connect.hset_multiple(key, items).await?;
            if result.eq_ignore_ascii_case("OK") {
                Ok(())
            } else {
                Err(format!("set_all_hash error : {result:?}").into())
            }
        } else {
            let mut pip = redis::pipe();
            let mut cmd = cmd("HSET");
            cmd.arg(key);
            for (key, value) in items {
                cmd.arg(key).arg(value);
            }
            pip.atomic().add_command(cmd).expire(key, seconds);
            let result: Vec<i32> = pip.query_async(&mut self.connect).await?;
            if result[1] == 1 {
                Ok(())
            } else {
                Err(format!("set_all_hash error : {result:?}").into())
            }
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

    #[instrument(name = "redis get_hash_fields", skip(self))]
    pub async fn get_hash_fields<V, F>(
        &mut self,
        key: &str,
        fields: F,
    ) -> Result<Vec<Option<V>>, BoxError>
    where
        V: redis::FromRedisValue,
        F: ToRedisArgs + std::marker::Send + std::marker::Sync + Debug,
    {
        self.connect.hget(key, fields).await.map_err(|e| e.into())
    }

    #[instrument(name = "redis get_hash_field_ttl", skip(self))]
    pub async fn get_hash_field_ttl(
        &mut self,
        key: &str,
        field: &str,
    ) -> Result<Option<Option<i64>>, BoxError> {
        let result: Vec<i64> = self.connect.httl(key, field).await?;
        Ok(match result.first().unwrap() {
            -2 => None,
            -1 => Some(None),
            other => Some(Some(*other)),
        })
    }

    #[instrument(name = "redis del_hash_field", skip(self))]
    pub async fn del_hash_field(&mut self, key: &str, field: &str) -> Result<i64, BoxError> {
        self.connect.hdel(key, field).await.map_err(|e| e.into())
    }

    const INCR: &str = r#"
        local key = KEYS[1]
        local ttl = tonumber(ARGV[1])
        local currentValue = redis.call("INCR", key)
        redis.call("EXPIRE", key, ttl)
        return currentValue
    "#;

    #[instrument(name = "redis incr", skip(self))]
    pub async fn incr(&mut self, key: &str, seconds: i64) -> Result<i64, BoxError> {
        if seconds >= 0 {
            redis::Script::new(Self::INCR)
                .key(key)
                .arg(seconds)
                .invoke_async::<i64>(&mut self.connect)
                .await
                .map_err(|e| e.into())
        } else {
            self.connect.incr(key, 1).await.map_err(|e| e.into())
        }
    }

    const INCR_V2: &str = r#"
        local key = KEYS[1]
        local ttl = tonumber(ARGV[1])
        local currentValue = redis.call("INCR", key)
        if currentValue == 1 then
            redis.call("EXPIRE", key, ttl)
        end
        return currentValue
    "#;

    #[instrument(name = "redis incr", skip(self))]
    pub async fn incr_v2(&mut self, key: &str, seconds: i64) -> Result<i64, BoxError> {
        if seconds >= 0 {
            redis::Script::new(Self::INCR_V2)
                .key(key)
                .arg(seconds)
                .invoke_async::<i64>(&mut self.connect)
                .await
                .map_err(|e| e.into())
        } else {
            self.connect.incr(key, 1).await.map_err(|e| e.into())
        }
    }
}
