use crate::{log::get_trade_id, utils::date_util::get_now_date_time_as_millis};
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
}

impl LogAspect {
    fn get_span(&self, trade_id: String, path: &str) -> Span {
        match self.get_level().as_str() {
            "info" => info_span!("trade_span", trade_id = trade_id, path = path),
            "debug" => debug_span!("trade_span", trade_id = trade_id, path = path),
            "warn" => warn_span!("trade_span", trade_id = trade_id, path = path),
            "error" => error_span!("trade_span", trade_id = trade_id, path = path),
            _ => tracing::trace_span!("trade_span", trade_id = trade_id, path = path),
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
        let mut span = tracing::Span::current();
        let trade_id = match context.get_meta_data().get_value("trade_id") {
            Some(trade_id) => trade_id.to_owned(),
            None => {
                let trade_id = get_trade_id();
                context
                    .get_mut_request()
                    .get_mut_headers()
                    .insert("trade_id".to_string(), trade_id.clone());
                trade_id
            }
        };
        let mut enter = None;
        if !span.metadata().is_some_and(|e| e.name() == "trade_span") {
            span = self.get_span(
                trade_id.clone(),
                &context.get_context_info().get_path().get_key(),
            );
            let _ = enter.insert(span.enter());
        }
        let start_time = get_now_date_time_as_millis();
        info!(message = "start handler");
        let result =
            tokio::spawn(async move { filter.call(context).await }.instrument(span.clone())).await;
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
        info!(
            message = "end handler",
            elapsed = get_now_date_time_as_millis() - start_time,
        );
        drop(enter);
        context
    }
}
