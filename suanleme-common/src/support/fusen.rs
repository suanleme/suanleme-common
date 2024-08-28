use crate::{
    log::get_trace_id,
    utils::{async_map::AsyncMap, date_util::get_now_date_time_as_millis},
};
use bytes::Bytes;
use fusen_rs::{
    fusen_common::{self, FusenContext, FusenRequest},
    fusen_procedural_macro::handler,
    handler::aspect::Aspect,
};
use suanleme_macro::Data;
use tracing::{debug_span, error, error_span, info, info_span, warn_span, Instrument, Span};

#[allow(dead_code)]
#[derive(Default, Data)]
pub struct LogAspect {
    level: String,
    async_map: AsyncMap<u64, String>,
}

impl LogAspect {
    pub fn new(level: &str) -> Self {
        Self {
            level: level.to_owned(),
            async_map: AsyncMap::new(),
        }
    }
}

impl LogAspect {
    fn get_span(&self, trace_id: String, path: &str) -> Span {
        match self.get_level().as_str() {
            "info" => info_span!("trace_span", trace_id = trace_id, path = path),
            "debug" => debug_span!("trace_span", trace_id = trace_id, path = path),
            "warn" => warn_span!("trace_span", trace_id = trace_id, path = path),
            "error" => error_span!("trace_span", trace_id = trace_id, path = path),
            _ => tracing::trace_span!("trace_span", trace_id = trace_id, path = path),
        }
    }
}

#[handler(id = "LogAspect")]
impl Aspect for LogAspect {
    async fn aroud(
        &self,
        filter: &'static dyn fusen_rs::filter::FusenFilter,
        mut context: fusen_common::FusenContext,
    ) -> Result<fusen_common::FusenContext, fusen_rs::Error> {
        let span = tracing::Span::current();
        let trace_id = match context.get_meta_data().get_value("trace_id") {
            Some(trace_id) => trace_id.to_owned(),
            None => {
                let trace_id = if span.metadata().is_some_and(|e| e.name() == "trace_span") {
                    self.async_map
                        .get(span.id().unwrap().into_u64())
                        .await?
                        .map_or(get_trace_id(), |e| e)
                } else {
                    get_trace_id()
                };
                context
                    .get_mut_request()
                    .get_mut_headers()
                    .insert("trace_id".to_string(), trace_id.clone());
                trace_id
            }
        };
        let mut new_span = None;
        if span.is_none() {
            let _ = new_span
                .insert(self.get_span(trace_id, &context.get_context_info().get_path().get_key()));
        }

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
        let result = if let Some(span) = new_span {
            tokio::spawn(future.instrument(span)).await
        } else {
            tokio::spawn(future).await
        };
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
