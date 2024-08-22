use serde::{Deserialize, Serialize};
use std::str::FromStr;
use suanleme_macro::Data;
use tracing_appender::{
    non_blocking::WorkerGuard,
    rolling::{RollingFileAppender, Rotation},
};

#[derive(Clone, Data, Debug, Default, Serialize, Deserialize)]
pub struct LogConfig {
    pub level: String,
    pub path: String,
    pub devmode: Option<bool>,
}

pub fn init_log(log_config: &LogConfig, app_name: &str) -> Option<WorkerGuard> {
    let tracing = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::from_str(&log_config.level).expect("logging init error"))
        .with_line_number(true)
        .with_thread_ids(true);
    if log_config.devmode.is_some_and(|e| !e) || log_config.devmode.is_none() {
        let file_appender = RollingFileAppender::new(Rotation::DAILY, &log_config.path, app_name);
        let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
        tracing.json().with_writer(non_blocking).init();
        Some(guard)
    } else {
        tracing.init();
        None
    }
}

pub fn get_uuid() -> String {
    uuid::Uuid::new_v4().to_string()
}

pub fn mask_str(str: &str) -> String {
    let len = str.len();
    let split = len / 2;
    let split2 = split / 2;
    let mut res = String::new();
    res.push_str(&str[..split2]);
    res.push_str(&"*".repeat(split));
    res.push_str(&str[split2 + split..]);
    res
}

pub fn limit_str(str: &str, limit: usize) -> &str {
    if str.len() > limit {
        &str[..limit]
    } else {
        str
    }
}
