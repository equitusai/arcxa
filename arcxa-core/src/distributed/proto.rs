//! Generated gRPC Protocol Definitions
//!
//! This module includes the generated Rust code from all proto files

pub mod shard_service {
    tonic::include_proto!("graphica.shard.v1");
}

// lineage.proto and quality.proto use package "graphica.v1"
pub mod graphica_v1 {
    tonic::include_proto!("graphica.v1");
}

// Coordinator service for shard auto-registration
pub mod coordinator_service {
    tonic::include_proto!("graphica.coordinator.v1");
}

// Prediction service for ML model invocation
pub mod prediction_service {
    tonic::include_proto!("graphica.ml");
}

// Re-export for convenience
pub use graphica_v1::*;

// File descriptor set for gRPC reflection
pub const FILE_DESCRIPTOR_SET: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/graphica_descriptor.bin"));

// Coordinator service file descriptor for reflection
pub const COORDINATOR_FILE_DESCRIPTOR_SET: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/coordinator_descriptor.bin"));

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_proto_compiled() {
        // Verify proto compiled successfully
        use shard_service::*;

        // Test Triple creation
        let triple = Triple {
            subject: "http://example.com/subject".to_string(),
            predicate: "http://example.com/predicate".to_string(),
            object: "value".to_string(),
            object_datatype: String::new(),
            object_language: String::new(),
            graph: String::new(),
        };

        assert_eq!(triple.subject, "http://example.com/subject");

        // Test QueryRequest creation
        let query = QueryRequest {
            sparql: "SELECT * WHERE { ?s ?p ?o } LIMIT 10".to_string(),
            limit: 10,
            offset: 0,
            projections: vec![],
            query_plan_hint: vec![],
            timeout_ms: 5000,
            request_id: "test-123".to_string(),
            timestamp: 0,
        };

        assert_eq!(query.limit, 10);
        assert_eq!(query.timeout_ms, 5000);
    }

    #[test]
    fn test_health_response() {
        use shard_service::*;

        let health = HealthResponse {
            status: health_response::Status::Healthy as i32,
            message: "All systems operational".to_string(),
            shard_id: 0,
            triple_count: 1000,
            disk_usage_bytes: 5000000,
            memory_usage_bytes: 2000000,
            last_query_timestamp: 0,
            uptime_seconds: 3600,
        };

        assert_eq!(health.shard_id, 0);
        assert_eq!(health.triple_count, 1000);
    }

    #[test]
    fn test_insert_batch_request() {
        use shard_service::*;

        let triples = vec![
            Triple {
                subject: "http://ex.com/1".to_string(),
                predicate: "http://ex.com/p1".to_string(),
                object: "value1".to_string(),
                object_datatype: String::new(),
                object_language: String::new(),
                graph: String::new(),
            },
            Triple {
                subject: "http://ex.com/2".to_string(),
                predicate: "http://ex.com/p2".to_string(),
                object: "value2".to_string(),
                object_datatype: String::new(),
                object_language: String::new(),
                graph: String::new(),
            },
        ];

        let request = InsertBatchRequest {
            triples,
            transactional: false,
            default_graph: String::new(),
            request_id: "insert-123".to_string(),
        };

        assert_eq!(request.triples.len(), 2);
        assert!(!request.transactional);
    }
}
