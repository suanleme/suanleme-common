use crate::FusenFuture;
use fusen_rs::fusen_common::date_util::get_now_date_time_as_millis;
use opentelemetry::trace::TraceContextExt;
use std::time::Duration;
use tokio::task;
use tracing::{error, info, info_span};
use tracing_futures::Instrument;
use tracing_opentelemetry::OpenTelemetrySpanExt;

pub async fn cron_job(
    interval: u64,
    job_name: &'static str,
    future: impl Fn() -> FusenFuture<()> + Clone + Send + 'static,
) {
    tokio::spawn(async move {
        loop {
            let span = info_span!("base_span");
            let span_context = span.context();
            let new_span = info_span!(
                "corn_job",
                name = job_name.to_owned(),
                trace_id = span_context.span().span_context().trace_id().to_string()
            );
            let _ = new_span.set_parent(span_context);
            let trace_id = new_span
                .context()
                .span()
                .span_context()
                .trace_id()
                .to_string();
            new_span.set_attribute("trace_id", trace_id.to_owned());
            let future = future.clone();
            match task::spawn(
                async move {
                    let start_time = get_now_date_time_as_millis();
                    future().await;
                    info!(ttl = get_now_date_time_as_millis() - start_time, "done");
                }
                .instrument(new_span),
            )
            .await
            {
                Ok(_) => {}
                Err(error) => {
                    error!("cron_job:error:{} error: {}", job_name, error);
                }
            };
            tokio::time::sleep(Duration::from_secs(interval)).await;
        }
    });
}

pub type JobFuture<T> = std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send>>;

pub fn cron_job_v2<F, R>(
    job_name: &'static str,
    future: impl Fn() -> F + Clone + 'static + Send,
    interval: u64,
    time_out: Option<Duration>,
) where
    F: std::future::Future<Output = R> + Send,
{
    tokio::spawn(async move {
        loop {
            let span = info_span!("base_span");
            let span_context = span.context();
            let new_span = info_span!(
                "corn_job",
                name = job_name.to_owned(),
                trace_id = span_context.span().span_context().trace_id().to_string()
            );
            let _ = new_span.set_parent(span_context);
            let trace_id = new_span
                .context()
                .span()
                .span_context()
                .trace_id()
                .to_string();
            new_span.set_attribute("trace_id", trace_id.to_owned());
            let future = future.clone();
            match task::spawn(
                async move {
                    let start_time = get_now_date_time_as_millis();
                    if let Some(time_out) = time_out {
                        if let Err(_error) = tokio::time::timeout(time_out, async move {
                            future().await;
                        })
                        .await
                        {
                            error!("job_name : {job_name} time_out");
                        }
                    } else {
                        future().await;
                    }
                    info!(ttl = get_now_date_time_as_millis() - start_time, "done");
                }
                .instrument(new_span),
            )
            .await
            {
                Ok(_) => {}
                Err(error) => {
                    error!("cron_job:error:{} error: {}", job_name, error);
                }
            };
            tokio::time::sleep(Duration::from_secs(interval)).await;
        }
    });
}
