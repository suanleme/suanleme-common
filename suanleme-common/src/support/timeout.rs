use bytes::Bytes;
use fusen_rs::{
    filter::ProceedingJoinPoint,
    fusen_common::{self, FusenContext, FusenRequest},
    fusen_procedural_macro::handler,
    handler::aspect::Aspect,
};
use std::{collections::HashMap, time::Duration};
use tracing::error;

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
