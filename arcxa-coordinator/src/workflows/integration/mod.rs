//! Workflow Integration Layer
//!
//! External service integrations for workflow actions:
//! - Kafka producer for SendToKafka
//! - HTTP client for SendToHttp
//! - Metrics for observability
//! - Circuit breakers for resilience

mod http_client;
mod kafka_producer;

pub use http_client::{HttpClient, HttpClientConfig, HttpResponse};
pub use kafka_producer::{DeliveryResult, KafkaProducer, KafkaProducerConfig};
