use std::str::FromStr;

use serde::{Deserialize, Serialize};
use tracing_appender::rolling::{RollingFileAppender, Rotation};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct LogConfig {
    pub level: String,
    pub path: String,
}

impl LogConfig {
    pub fn level(mut self, level: &str) -> Self {
        level.clone_into(&mut self.level);
        self
    }
    pub fn path(mut self, path: &str) -> Self {
        path.clone_into(&mut self.path);
        self
    }
}

pub fn init_log(log_config: &LogConfig, app_name: &str) {
    let file_appender = RollingFileAppender::new(Rotation::DAILY, &log_config.path, app_name);
    tracing_subscriber::fmt()
        .json()
        .with_max_level(tracing::Level::from_str(&log_config.level).expect("logging init error"))
        .with_writer(file_appender)
        .with_line_number(true)
        .with_thread_ids(true)
        .init();
}

pub fn get_uuid() -> String {
    uuid::Uuid::new_v4().to_string()
}
