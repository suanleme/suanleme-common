use crate::utils::date_util::get_now_date_time_as_millis;
use bytes::Bytes;
use fusen_rs::{
    filter::ProceedingJoinPoint,
    fusen_common::{self, FusenContext, FusenRequest},
    fusen_procedural_macro::handler,
    handler::aspect::Aspect,
};
use opentelemetry::propagation::text_map_propagator::TextMapPropagator;
use opentelemetry::trace::TraceContextExt;
use opentelemetry_sdk::propagation::TraceContextPropagator;
use std::collections::HashMap;
use tracing::{error, info, info_span, Instrument, Span};
use tracing_opentelemetry::OpenTelemetrySpanExt;

#[allow(dead_code)]
#[derive(Default)]
pub struct LogAspect {
    trace_context_propagator: TraceContextPropagator,
}

#[handler(id = "LogAspect")]
impl Aspect for LogAspect {
    async fn aroud(
        &self,
        mut join_point: ProceedingJoinPoint,
    ) -> Result<fusen_common::FusenContext, fusen_rs::Error> {
        let context = join_point.get_mut_context();
        let mut span_context = self.trace_context_propagator.extract_with_context(
            &Span::current().context(),
            context.get_meta_data().get_inner(),
        );
        let mut first_span = None;
        let path = context.get_context_info().get_path().get_key();
        if !span_context.has_active_span() {
            let span = info_span!("begin_span", path = &path);
            span_context = span.context();
            let _ = first_span.insert(span);
        }
        let span = info_span!(
            "trace_span",
            trace_id = span_context.span().span_context().trace_id().to_string(),
            path = path
        );
        let _ = span.set_parent(span_context);
        let trace_id = span.context().span().span_context().trace_id().to_string();
        span.set_attribute("trace_id", trace_id.to_owned());
        if context.get_meta_data().get_value("traceparent").is_none() {
            self.trace_context_propagator
                .inject_context(&span.context(), context.get_mut_request().get_mut_headers());
        };
        let future = async move {
            let start_time = get_now_date_time_as_millis();
            info!(message = "start LogAspect handler");
            let context = join_point.proceed().await;
            info!(
                message = "end LogAspect handler",
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
                    FusenRequest::new("", HashMap::new(), Bytes::new()),
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

#[allow(dead_code)]
#[derive(Default)]
pub struct LogAspectV2 {
    trace_context_propagator: TraceContextPropagator,
}

#[handler(id = "LogAspectV2")]
impl Aspect for LogAspectV2 {
    async fn aroud(
        &self,
        mut join_point: ProceedingJoinPoint,
    ) -> Result<fusen_common::FusenContext, fusen_rs::Error> {
        let context = &mut join_point.get_mut_context();
        let mut span_context = self.trace_context_propagator.extract_with_context(
            &Span::current().context(),
            context.get_request().get_headers(),
        );
        let mut first_span = None;
        let path = context.get_context_info().get_path().get_key();
        if !span_context.has_active_span() {
            let span = info_span!("begin_span", path = path);
            span_context = span.context();
            let _ = first_span.insert(span);
        }
        let span = info_span!(
            "trace_span",
            trace_id = span_context.span().span_context().trace_id().to_string(),
            path = path
        );
        let _ = span.set_parent(span_context);
        let trace_id = span.context().span().span_context().trace_id().to_string();
        span.set_attribute("trace_id", trace_id.to_owned());
        if !context
            .get_request()
            .get_headers()
            .contains_key("traceparent")
        {
            self.trace_context_propagator
                .inject_context(&span.context(), context.get_mut_request().get_mut_headers());
        };
        let future = async move { join_point.proceed().await };
        let result = tokio::spawn(future.instrument(span)).await;
        match result {
            Ok(context) => context,
            Err(error) => Err(error.into()),
        }
    }
}
