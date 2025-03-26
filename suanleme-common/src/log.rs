use chrono::Local;
use opentelemetry::global::ObjectSafeSpan;
use opentelemetry::trace::TracerProvider;
use opentelemetry::{StringValue, Value};
use opentelemetry_otlp::{ExporterBuildError, SpanExporter, WithExportConfig};
use opentelemetry_sdk::runtime;
use opentelemetry_sdk::trace::{span_processor_with_async_runtime, Span};
use opentelemetry_sdk::{trace::SdkTracerProvider, Resource};
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use std::str::FromStr;
use suanleme_macro::Data;
use tokio::time::Instant;
use tracing::field::Field;
use tracing::{error, info, Subscriber};
use tracing_appender::{
    non_blocking::WorkerGuard,
    rolling::{RollingFileAppender, Rotation},
};
use tracing_opentelemetry::OpenTelemetryLayer;
use tracing_subscriber::fmt::format::Writer;
use tracing_subscriber::fmt::time::FormatTime;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, Layer};

#[derive(Clone, Data, Debug, Default, Serialize, Deserialize)]
pub struct LogConfig {
    pub level: String,
    pub path: Option<String>,
    pub endpoint: Option<String>,
    pub env_filter: Option<String>,
    pub devmode: Option<bool>,
}

#[derive(Default, Data)]
pub struct LogWorkGroup {
    work_guard: Option<WorkerGuard>,
    tracer_provider: Option<SdkTracerProvider>,
}

impl Drop for LogWorkGroup {
    fn drop(&mut self) {
        if let Some(tracer_provider) = &self.tracer_provider {
            let _ = tracer_provider.shutdown();
        }
    }
}

fn init_opentelemetry_trace(
    otlp_url: &str,
    app_name: &str,
) -> Result<SdkTracerProvider, ExporterBuildError> {
    let exporter = SpanExporter::builder()
        .with_tonic()
        .with_endpoint(otlp_url)
        .build()?;
    Ok(SdkTracerProvider::builder()
        .with_resource(
            Resource::builder()
                .with_service_name(Value::String(StringValue::from(app_name.to_owned())))
                .build(),
        )
        .with_span_processor(
            span_processor_with_async_runtime::BatchSpanProcessor::builder(
                exporter,
                runtime::Tokio,
            )
            .build(),
        )
        .build())
}

pub fn init_log(log_config: &LogConfig, app_name: &str) -> Option<LogWorkGroup> {
    std::panic::set_hook(Box::new(|error| {
        error!("panic : {:?}", error.to_string());
    }));
    let mut worker_guard = None;
    let mut tracer_guard = None;
    let mut layter_list = vec![];
    let env_filter = || {
        if let Some(env_filter) = &log_config.env_filter {
            let env_filter = env_filter.replace("{level}", log_config.get_level());
            EnvFilter::from_str(&env_filter).unwrap()
        } else {
            EnvFilter::from_default_env()
        }
    };
    if let Some(path) = &log_config.path {
        let file_appender = RollingFileAppender::builder()
            .rotation(Rotation::DAILY)
            .filename_prefix(app_name)
            .filename_suffix("log")
            .build(path)
            .expect("initializing rolling file appender failed");
        let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
        let _ = worker_guard.insert(guard);
        let tracing = tracing_subscriber::fmt::layer()
            .with_line_number(true)
            .with_thread_ids(true)
            .with_timer(LocalTimer);
        let json_tracing = tracing.json().with_writer(non_blocking);
        layter_list.push(json_tracing.boxed());
    };
    if log_config.devmode.is_some_and(|e| e) {
        let tracing = tracing_subscriber::fmt::layer()
            .with_line_number(true)
            .with_thread_ids(true)
            .with_timer(LocalTimer);
        layter_list.push(tracing.boxed());
    }
    if let Some(endpoint) = &log_config.endpoint {
        let provider = init_opentelemetry_trace(endpoint, app_name).unwrap();
        let _ = tracer_guard.insert(provider.clone());
        let opentelemetry = OpenTelemetryLayer::new(provider.tracer(app_name.to_owned()));
        layter_list.push(opentelemetry.boxed());
    }
    if layter_list.is_empty() {
        return None;
    }
    let mut layer = layter_list.remove(0);
    for item in layter_list {
        layer = Box::new(layer.and_then(item));
    }
    tracing_subscriber::registry()
        .with(env_filter())
        .with(layer)
        .with(TimingLayer)
        .init();
    Some(
        LogWorkGroup::default()
            .tracer_provider(tracer_guard)
            .work_guard(worker_guard),
    )
}

struct LocalTimer;

impl FormatTime for LocalTimer {
    fn format_time(&self, w: &mut Writer<'_>) -> std::fmt::Result {
        write!(w, "{}", Local::now().format("%FT%T%.3f"))
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
    let len = str.chars().by_ref().count();
    let split = len / 2;
    let split2 = split / 2;
    let mut res = String::new();
    let mut chars = str.chars();
    res.extend(chars.by_ref().take(split2));
    res.push_str(&"*".repeat(split));
    let _ = chars.by_ref().take(split).count();
    res.extend(chars);
    res
}

pub fn limit_str(str: &str, limit: usize) -> String {
    let len = str.chars().by_ref().count();
    if len > limit {
        let mut chars = str.chars();
        let mut string = chars.by_ref().take(limit).collect::<String>();
        string.push_str("..");
        string
    } else {
        str.to_owned()
    }
}

// 自定义 Layer 用于计时
struct TimingLayer;

struct TempStatus {
    time: Instant,
    span_type: Option<String>,
}

impl<S> Layer<S> for TimingLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_new_span(
        &self,
        attrs: &tracing::span::Attributes<'_>,
        id: &tracing::span::Id,
        ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let mut span_type = None;
        attrs.record(&mut |field: &Field, value: &dyn std::fmt::Debug| {
            if field.name() == "span_type" {
                let _ = span_type.insert(format!("{:?}", value).replace("\"", ""));
            }
        });
        if let Some(span) = ctx.span(id) {
            span.extensions_mut().insert(TempStatus {
                time: Instant::now(),
                span_type,
            })
        };
    }

    fn on_close(&self, id: tracing::span::Id, ctx: tracing_subscriber::layer::Context<'_, S>) {
        // 在 span 关闭时计算耗时
        ctx.span(&id).map(|span| {
            let tracing_id = span
                .extensions()
                .get::<Span>()
                .map(|span| span.span_context().trace_id().to_string());
            span.extensions().get::<TempStatus>().map(|temp| {
                let duration = temp.time.elapsed().as_millis();
                let span_name = span.name();
                let span_type_temp = &temp.span_type;
                let span_type = span_type_temp
                    .as_ref()
                    .map(|e| e.as_str())
                    .unwrap_or("unknown");
                let trace_id = tracing_id.as_deref().unwrap_or("unknown");
                //unknown
                info!(
                    trace_id = trace_id,
                    span_name = &span_name,
                    span_type = &span_type,
                    elapsed = &duration
                )
            })
        });
    }
}

#[test]
fn test() {
    let str = "瓦达是的";
    let mut chars = str.chars();
    println!("{:?}", chars);
    let mut res = String::new();
    res.extend(chars.by_ref().take(2));
    println!("{:?}", res);

    println!("{:?}", chars);
}
