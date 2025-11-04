use bytes::Bytes;
use fusen_rs::{
    filter::ProceedingJoinPoint,
    fusen_common::{self, date_util::get_now_date_time_as_millis, FusenContext, FusenRequest},
    fusen_procedural_macro::handler,
    handler::aspect::Aspect,
};
use std::{collections::HashMap, time::Duration};
use tracing::{error, info};

#[allow(dead_code)]
#[derive(Default)]
pub struct TimeOutAspect {
    pub timeout: Option<Duration>,
}

#[handler(id = "TimeOutAspect")]
impl Aspect for TimeOutAspect {
    async fn aroud(
        &self,
        join_point: ProceedingJoinPoint,
    ) -> Result<fusen_common::FusenContext, fusen_rs::Error> {
        let context = if let Some(timeout) = self.timeout {
            let context = tokio::select! {
                _ = tokio::time::sleep(timeout) => {
                    error!("TimeOutAspect:time_out");
                    let mut context = FusenContext::new(
                    "unique_identifier".to_owned(),
                    Default::default(),
                    FusenRequest::new("", HashMap::new(), Bytes::new()),
                    Default::default(),
                );
                *context.get_mut_response().get_mut_response() = Ok(Bytes::copy_from_slice(
                    b"{\"code\":\"C998\",\"message\":\"time out\"}",
                ));
                Ok(context)
                },
                context = join_point.proceed() => context
            };
            context
        } else {
            join_point.proceed().await
        };
        context
    }
}

#[derive(Default)]
pub struct TimeOutAspectV2 {
    pub timeout: Option<Duration>,
}

#[handler(id = "TimeOutAspectV2")]
impl Aspect for TimeOutAspectV2 {
    async fn aroud(
        &self,
        join_point: ProceedingJoinPoint,
    ) -> Result<fusen_common::FusenContext, fusen_rs::Error> {
        let start_time = get_now_date_time_as_millis();
        let path = join_point
            .get_context()
            .get_context_info()
            .get_path()
            .clone();
        let context = if let Some(timeout) = self.timeout {
            let context = tokio::select! {
                _ = tokio::time::sleep(timeout) => {
                    error!("TimeOutAspectV2:time_out");
                    let mut context = FusenContext::new(
                    "unique_identifier".to_owned(),
                    Default::default(),
                    FusenRequest::new("", HashMap::new(), Bytes::new()),
                    Default::default(),
                );
                *context.get_mut_response().get_mut_response_ty() = Some("String");
                *context.get_mut_response().get_mut_response() = Err(fusen_common::error::FusenError::Info("time out".to_string()));
                Ok(context)
                },
                context = join_point.proceed() => context
            };
            context
        } else {
            join_point.proceed().await
        };
        info!(
            "Path : {path:?} 耗时 : {}",
            get_now_date_time_as_millis() - start_time
        );
        context
    }
}
