use chrono::Local;
use opentelemetry::trace::TracerProvider;
use opentelemetry::{trace::TraceError, StringValue, Value};
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::{
    runtime,
    trace::{Config, TracerProvider as Tracer},
    Resource,
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::str::FromStr;
use suanleme_macro::Data;
use tracing::Level;
use tracing_appender::{
    non_blocking::WorkerGuard,
    rolling::{RollingFileAppender, Rotation},
};
use tracing_opentelemetry::OpenTelemetryLayer;
use tracing_subscriber::filter::filter_fn;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, Layer};

#[derive(Clone, Data, Debug, Default, Serialize, Deserialize)]
pub struct LogConfig {
    pub level: String,
    pub path: String,
    pub endpoint: Option<String>,
    pub span_filter: Option<HashSet<String>>,
    pub devmode: Option<bool>,
}

fn init_opentelemetry_trace(otlp_url: &str, app_name: &str) -> Result<Tracer, TraceError> {
    opentelemetry_otlp::new_pipeline()
        .tracing()
        .with_exporter(
            opentelemetry_otlp::new_exporter()
                .tonic()
                .with_endpoint(otlp_url),
        )
        .with_trace_config(Config::default().with_resource(Resource::new(vec![
            opentelemetry::KeyValue::new(
                "service.name",
                Value::String(StringValue::from(app_name.to_owned())),
            ),
        ])))
        .install_batch(runtime::Tokio)
}

pub fn init_log(log_config: &LogConfig, app_name: &str) -> Option<WorkerGuard> {
    if log_config.devmode.is_some_and(|e| !e) || log_config.devmode.is_none() {
        let tracing = tracing_subscriber::fmt::layer()
            .with_line_number(true)
            .with_thread_ids(true);
        let file_appender = RollingFileAppender::new(Rotation::DAILY, &log_config.path, app_name);
        let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
        let json_tracing = tracing.json().with_writer(non_blocking);
        let tracing_subscriber = tracing_subscriber::registry().with(json_tracing.with_filter(
            tracing_subscriber::filter::LevelFilter::from_level(
                Level::from_str(log_config.get_level()).unwrap(),
            ),
        ));
        if let Some(otlp_url) = &log_config.endpoint {
            let provider = init_opentelemetry_trace(otlp_url, app_name).unwrap();
            let opentelemetry_layer = OpenTelemetryLayer::new(provider.tracer(app_name.to_owned()));
            if log_config
                .get_span_filter()
                .as_ref()
                .is_some_and(|e| !e.is_empty())
            {
                let set = log_config.get_span_filter().clone().unwrap();
                let name_filter = filter_fn(move |metadata| set.contains(metadata.name()));
                tracing_subscriber
                    .with(opentelemetry_layer.with_filter(name_filter))
                    .init();
            } else {
                tracing_subscriber.with(opentelemetry_layer).init();
            }
        } else {
            tracing_subscriber.init();
        }
        Some(guard)
    } else {
        let tracing = tracing_subscriber::fmt::layer()
            .with_line_number(true)
            .with_thread_ids(true)
            .with_filter(tracing_subscriber::filter::LevelFilter::from_level(
                Level::from_str(log_config.get_level()).unwrap(),
            ));
        tracing_subscriber::registry().with(tracing).init();
        None
    }
}

pub fn get_uuid() -> String {
    uuid::Uuid::new_v4().to_string()
}

pub fn get_trace_id() -> String {
    format!(
        "{}-{}",
        uuid::Uuid::new_v4(),
        Local::now().format("%Y%m%d%H%M%S")
    )
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

pub fn limit_str(str: &str, limit: usize) -> String {
    if str.len() > limit {
        format!("{}..", &str[..limit])
    } else {
        str.to_owned()
    }
}
