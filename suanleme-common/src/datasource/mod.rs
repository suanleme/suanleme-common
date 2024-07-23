use serde::{Deserialize, Serialize};
use sqlx::{query, Error, PgConnection, PgPool};

use crate::error::BoxError;

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

pub async fn set_constraints_all_immediate(connect: &mut PgConnection) -> Result<(), BoxError> {
    query("SET CONSTRAINTS ALL IMMEDIATE")
        .execute(connect)
        .await?;
    Ok(())
}
