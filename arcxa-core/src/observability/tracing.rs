//! # Distributed Tracing Module
//!
//! Structured tracing across pipeline stages with correlation ID propagation.

use super::CorrelationId;
use std::time::Instant;
use tracing::{debug, error, info, warn, Span};

/// Tracing context for pipeline stages
#[derive(Debug, Clone)]
pub struct TracingContext {
    pub correlation_id: CorrelationId,
    pub stage: String,
    pub start_time: Instant,
}

impl TracingContext {
    /// Create new tracing context for stage
    pub fn new(correlation_id: CorrelationId, stage: &str) -> Self {
        Self {
            correlation_id,
            stage: stage.to_string(),
            start_time: Instant::now(),
        }
    }

    /// Create child context for nested operation
    pub fn child(&self, stage: &str) -> Self {
        Self {
            correlation_id: self.correlation_id.child(),
            stage: stage.to_string(),
            start_time: Instant::now(),
        }
    }

    /// Record stage completion
    pub fn finish(&self) {
        let elapsed = self.start_time.elapsed();
        info!(
            stage = %self.stage,
            correlation_id = %self.correlation_id.context(),
            duration_ms = elapsed.as_millis(),
            "Pipeline stage completed"
        );
    }

    /// Record stage error
    pub fn error(&self, error: &str) {
        let elapsed = self.start_time.elapsed();
        error!(
            stage = %self.stage,
            correlation_id = %self.correlation_id.context(),
            duration_ms = elapsed.as_millis(),
            error = %error,
            "Pipeline stage failed"
        );
    }

    /// Record stage warning
    pub fn warn(&self, message: &str) {
        warn!(
            stage = %self.stage,
            correlation_id = %self.correlation_id.context(),
            message = %message,
            "Pipeline stage warning"
        );
    }

    /// Get tracing span for instrumentation
    pub fn span(&self) -> Span {
        tracing::info_span!(
            "pipeline_stage",
            stage = %self.stage,
            trace_id = %self.correlation_id.trace_id,
            span_id = %self.correlation_id.span_id,
        )
    }
}

/// Instrument a pipeline stage with tracing
pub async fn instrument_pipeline_stage<F, T>(
    correlation_id: CorrelationId,
    stage: &str,
    operation: F,
) -> Result<T, anyhow::Error>
where
    F: std::future::Future<Output = Result<T, anyhow::Error>>,
{
    let ctx = TracingContext::new(correlation_id, stage);
    let span = ctx.span();
    let _guard = span.enter();

    debug!(
        stage = %stage,
        correlation_id = %ctx.correlation_id.context(),
        "Starting pipeline stage"
    );

    match operation.await {
        Ok(result) => {
            ctx.finish();
            Ok(result)
        }
        Err(e) => {
            ctx.error(&e.to_string());
            Err(e)
        }
    }
}

/// Macro for instrumenting synchronous functions
#[macro_export]
macro_rules! trace_stage {
    ($ctx:expr, $stage:expr, $block:block) => {{
        let _span = $ctx.span();
        let _guard = _span.enter();
        tracing::debug!(
            stage = $stage,
            correlation_id = %$ctx.correlation_id.context(),
            "Starting pipeline stage"
        );
        let result = $block;
        $ctx.finish();
        result
    }};
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_tracing_context() {
        let id = CorrelationId::new();
        let ctx = TracingContext::new(id, "test_stage");

        std::thread::sleep(std::time::Duration::from_millis(10));
        ctx.finish();

        assert!(ctx.start_time.elapsed().as_millis() >= 10);
    }

    #[tokio::test]
    async fn test_instrument_pipeline_stage_success() {
        let id = CorrelationId::new();

        let result =
            instrument_pipeline_stage(id, "test", async { Ok::<_, anyhow::Error>(42) }).await;

        assert_eq!(result.unwrap(), 42);
    }

    #[tokio::test]
    async fn test_instrument_pipeline_stage_error() {
        let id = CorrelationId::new();

        let result = instrument_pipeline_stage(id, "test", async {
            Err::<i32, _>(anyhow::anyhow!("test error"))
        })
        .await;

        assert!(result.is_err());
    }
}
