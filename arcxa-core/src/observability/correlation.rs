//! # Correlation ID Module
//!
//! Generates and propagates correlation IDs across the entire pipeline for distributed tracing.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use uuid::Uuid;

/// Correlation ID for tracing requests across pipeline stages
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CorrelationId {
    /// Unique trace ID
    pub trace_id: String,
    /// Span ID within trace
    pub span_id: String,
    /// Parent span ID (if nested)
    pub parent_span_id: Option<String>,
}

impl CorrelationId {
    /// Create new root correlation ID
    pub fn new() -> Self {
        Self {
            trace_id: Uuid::new_v4().to_string(),
            span_id: Uuid::new_v4().to_string(),
            parent_span_id: None,
        }
    }

    /// Create child span from parent
    pub fn child(&self) -> Self {
        Self {
            trace_id: self.trace_id.clone(),
            span_id: Uuid::new_v4().to_string(),
            parent_span_id: Some(self.span_id.clone()),
        }
    }

    /// Get trace context for logging
    pub fn context(&self) -> String {
        if let Some(ref parent) = self.parent_span_id {
            format!(
                "trace_id={} span_id={} parent_span_id={}",
                self.trace_id, self.span_id, parent
            )
        } else {
            format!("trace_id={} span_id={}", self.trace_id, self.span_id)
        }
    }
}

impl Default for CorrelationId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for CorrelationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.trace_id)
    }
}

/// High-performance correlation ID generator
pub struct CorrelationIdGenerator {
    node_id: u64,
    counter: AtomicU64,
}

impl CorrelationIdGenerator {
    /// Create new generator with node ID
    pub fn new(node_id: u64) -> Self {
        Self {
            node_id,
            counter: AtomicU64::new(0),
        }
    }

    /// Generate correlation ID using Snowflake-style ID
    /// Format: timestamp(41 bits) | node_id(10 bits) | counter(12 bits)
    pub fn generate(&self) -> CorrelationId {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let counter = self.counter.fetch_add(1, Ordering::Relaxed) & 0xFFF;
        let id = (timestamp << 22) | ((self.node_id & 0x3FF) << 12) | counter;

        CorrelationId {
            trace_id: format!("{:016x}", id),
            span_id: format!("{:08x}", counter),
            parent_span_id: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_correlation_id_creation() {
        let id = CorrelationId::new();
        assert!(!id.trace_id.is_empty());
        assert!(!id.span_id.is_empty());
        assert!(id.parent_span_id.is_none());
    }

    #[test]
    fn test_correlation_id_child() {
        let parent = CorrelationId::new();
        let child = parent.child();

        assert_eq!(child.trace_id, parent.trace_id);
        assert_ne!(child.span_id, parent.span_id);
        assert_eq!(child.parent_span_id, Some(parent.span_id));
    }

    #[test]
    fn test_generator() {
        let gen = CorrelationIdGenerator::new(1);
        let id1 = gen.generate();
        let id2 = gen.generate();

        assert_ne!(id1.trace_id, id2.trace_id);
    }
}
