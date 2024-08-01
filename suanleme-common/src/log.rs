use std::str::FromStr;

use serde::{Deserialize, Serialize};
use suanleme_macro::Data;
use tracing_appender::rolling::{RollingFileAppender, Rotation};

#[derive(Clone, Data, Debug, Default, Serialize, Deserialize)]
pub struct LogConfig {
    pub level: String,
    pub path: String,
    pub devmode: Option<bool>,
}

pub fn init_log(log_config: &LogConfig, app_name: &str) {
    let tracing = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::from_str(&log_config.level).expect("logging init error"))
        .with_line_number(true)
        .with_thread_ids(true);
    if log_config.devmode.is_some_and(|e| !e) || log_config.devmode.is_none() {
        let file_appender = RollingFileAppender::new(Rotation::DAILY, &log_config.path, app_name);
        tracing.json().with_writer(file_appender).init();
    } else {
        tracing.init();
    };
}

pub fn get_uuid() -> String {
    uuid::Uuid::new_v4().to_string()
}
