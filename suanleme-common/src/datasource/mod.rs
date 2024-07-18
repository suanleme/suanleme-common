use serde::{Deserialize, Serialize};
use sqlx::{Error, PgPool};

#[derive(Debug, Serialize, Deserialize)]
pub struct DataSourceConfig {
    pub url: String,
    pub username: String,
    pub password: String,
}

pub async fn init_data_source(config: &DataSourceConfig) -> Result<PgPool, Error> {
    let mut url = config.url.to_string();
    url = url.replace("{username}", &config.username);
    url = url.replace("{password}", &config.password);
    PgPool::connect(&url).await
}
