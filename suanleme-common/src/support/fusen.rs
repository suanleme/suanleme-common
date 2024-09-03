use crate::{
    suanleme_macro::Data,
    utils::{async_map::AsyncMap, date_util::get_now_date_time_as_millis},
};
use bytes::Bytes;
use fusen_rs::{
    fusen_common::{self, FusenContext, FusenRequest},
    fusen_procedural_macro::handler,
    handler::aspect::Aspect,
};
use opentelemetry::propagation::text_map_propagator::TextMapPropagator;
use opentelemetry::{trace::TraceContextExt, Context};
use opentelemetry_sdk::propagation::TraceContextPropagator;
use tracing::{debug_span, error, error_span, info, info_span, warn_span, Instrument, Span};
use tracing_opentelemetry::OpenTelemetrySpanExt;

#[allow(dead_code)]
#[derive(Default, Data)]
pub struct LogAspect {
    name: String,
    level: String,
    async_map: AsyncMap<u64, String>,
    trace_context_propagator: TraceContextPropagator,
}

impl LogAspect {
    pub fn new(name: &str, level: &str) -> Self {
        Self {
            name: name.to_owned(),
            level: level.to_owned(),
            async_map: AsyncMap::new(),
            trace_context_propagator: TraceContextPropagator::new(),
        }
    }
}

impl LogAspect {
    fn get_new_span(&self, context: Context, path: &str) -> Span {
        let span = match self.get_level().as_str() {
            "info" => info_span!("trace_span", path = path),
            "debug" => debug_span!("trace_span", path = path),
            "warn" => warn_span!("trace_span", path = path),
            "error" => error_span!("trace_span", path = path),
            _ => tracing::trace_span!("trace_span", path = path),
        };
        span.set_parent(context);
        span
    }
}

#[handler(id = "LogAspect")]
impl Aspect for LogAspect {
    async fn aroud(
        &self,
        filter: &'static dyn fusen_rs::filter::FusenFilter,
        mut context: fusen_common::FusenContext,
    ) -> Result<fusen_common::FusenContext, fusen_rs::Error> {
        let span_context = self.get_trace_context_propagator().extract_with_context(
            &Span::current().context(),
            context.get_meta_data().get_inner(),
        );
        let span = self.get_new_span(
            span_context,
            &context.get_context_info().get_path().get_key(),
        );
        let trace_id = span.context().span().span_context().trace_id().to_string();
        span.set_attribute("trace_id", trace_id.to_owned());
        if context.get_meta_data().get_value("traceparent").is_none() {
            self.get_trace_context_propagator()
                .inject_context(&span.context(), context.get_mut_request().get_mut_headers());
        };
        let future = async move {
            let start_time = get_now_date_time_as_millis();
            info!(message = "start handler");
            let context = filter.call(context).await;
            info!(
                message = "end handler",
                elapsed = get_now_date_time_as_millis() - start_time,
            );
            context
        };
        let result = tokio::spawn(future.instrument(span)).await;
        let context = match result {
            Ok(context) => context,
            Err(error) => {
                error!(message = format!("{:?}", error));
                let mut context = FusenContext::new(
                    "unique_identifier".to_owned(),
                    Default::default(),
                    FusenRequest::new(None, Bytes::new()),
                    Default::default(),
                );
                *context.get_mut_response().get_mut_response() = Ok(Bytes::copy_from_slice(
                    b"{\"code\":\"9999\",\"message\":\"service error\"}",
                ));
                Ok(context)
            }
        };
        context
    }
}
