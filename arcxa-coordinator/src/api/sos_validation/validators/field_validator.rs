//! Field validators for Interface entity fields
//!
//! Provides validation functions for interface enum fields

use anyhow::{bail, Result};

/// Valid interface directions
const VALID_DIRECTIONS: &[&str] = &["Provider", "Consumer", "Bidirectional"];

/// Valid protocols
const VALID_PROTOCOLS: &[&str] = &[
    "REST",
    "gRPC",
    "MQTT",
    "AMQP",
    "Kafka",
    "WebSocket",
    "HTTP",
    "HTTPS",
    "TCP",
    "UDP",
];

/// Valid data formats
const VALID_DATA_FORMATS: &[&str] = &[
    "JSON",
    "XML",
    "Protobuf",
    "Avro",
    "MessagePack",
    "Parquet",
    "CSV",
    "YAML",
];

/// Validate interface direction
pub fn validate_direction(direction: &str) -> Result<()> {
    if VALID_DIRECTIONS.contains(&direction) {
        Ok(())
    } else {
        bail!(
            "Invalid direction '{}'. Must be one of: {}",
            direction,
            VALID_DIRECTIONS.join(", ")
        )
    }
}

/// Validate protocol
pub fn validate_protocol(protocol: &str) -> Result<()> {
    if VALID_PROTOCOLS.contains(&protocol) {
        Ok(())
    } else {
        bail!(
            "Invalid protocol '{}'. Must be one of: {}",
            protocol,
            VALID_PROTOCOLS.join(", ")
        )
    }
}

/// Validate data format
pub fn validate_data_format(data_format: &str) -> Result<()> {
    if VALID_DATA_FORMATS.contains(&data_format) {
        Ok(())
    } else {
        bail!(
            "Invalid data format '{}'. Must be one of: {}",
            data_format,
            VALID_DATA_FORMATS.join(", ")
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_directions() {
        assert!(validate_direction("Provider").is_ok());
        assert!(validate_direction("Consumer").is_ok());
        assert!(validate_direction("Bidirectional").is_ok());
    }

    #[test]
    fn test_invalid_direction() {
        let result = validate_direction("Invalid");
        assert!(result.is_err());
    }

    #[test]
    fn test_valid_protocols() {
        assert!(validate_protocol("REST").is_ok());
        assert!(validate_protocol("gRPC").is_ok());
        assert!(validate_protocol("MQTT").is_ok());
    }

    #[test]
    fn test_invalid_protocol() {
        let result = validate_protocol("InvalidProtocol");
        assert!(result.is_err());
    }

    #[test]
    fn test_valid_data_formats() {
        assert!(validate_data_format("JSON").is_ok());
        assert!(validate_data_format("XML").is_ok());
        assert!(validate_data_format("Protobuf").is_ok());
    }

    #[test]
    fn test_invalid_data_format() {
        let result = validate_data_format("InvalidFormat");
        assert!(result.is_err());
    }
}
