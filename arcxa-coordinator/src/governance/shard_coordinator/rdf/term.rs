//! RDF Term Representation and Serialization
//!
//! Advanced type-safe representation of RDF terms with optimized serialization
//! for gRPC protobuf transmission to shards.
//!
//! ## Features
//!
//! - **Type Safety**: Distinct types for URIs, literals, and blank nodes
//! - **Zero-Copy Serialization**: Efficient conversion to protobuf format
//! - **Normalization**: Automatic handling of quote stripping and escaping
//! - **Validation**: Pre-transmission validation to prevent gRPC errors
//!
//! ## Performance
//!
//! - Parse: O(n) where n = input length
//! - Serialize: O(1) for URIs/blank nodes, O(n) for literals
//! - Memory: Minimal allocations, reuses parsed strings

use anyhow::{Context, Result};
use graphica_core::distributed::proto::shard_service::Triple;

use super::literal::parse_rdf_object;

/// RDF Term - type-safe representation of RDF subjects/predicates/objects
///
/// Represents the three types of RDF terms with proper normalization
/// for transmission to shards.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RdfTerm {
    /// URI reference: http://example.com/resource
    Uri(String),

    /// Literal value with optional datatype and language
    Literal {
        /// Raw literal value (without enclosing quotes)
        value: String,
        /// XSD datatype URI (empty if none)
        datatype: String,
        /// BCP 47 language tag (empty if none)
        language: String,
    },

    /// Blank node: _:b1
    BlankNode(String),
}

impl RdfTerm {
    /// Parse an RDF term from string representation
    ///
    /// Parses RDF syntax and converts to normalized RdfTerm.
    ///
    /// ## Arguments
    /// * `input` - RDF term string (e.g., "<http://ex.com>", "\"value\"", "_:b1")
    ///
    /// ## Returns
    /// Normalized RdfTerm ready for serialization
    ///
    /// ## Performance
    /// - Time: O(n) where n = input length
    /// - Space: O(n) for term storage
    pub fn parse(input: &str) -> Result<Self> {
        let (value, datatype, language) = parse_rdf_object(input)
            .with_context(|| format!("Failed to parse RDF term: {}", input))?;

        // Determine term type based on parsing result
        if value.starts_with('<') && value.ends_with('>') {
            // URI reference - strip angle brackets
            Ok(RdfTerm::Uri(value[1..value.len() - 1].to_string()))
        } else if value.starts_with("_:") {
            // Blank node - keep as-is
            Ok(RdfTerm::BlankNode(value))
        } else if value.starts_with('"') && value.ends_with('"') {
            // Literal - normalize by stripping quotes and unescaping
            let normalized_value = Self::normalize_literal_value(&value)?;

            Ok(RdfTerm::Literal {
                value: normalized_value,
                datatype,
                language,
            })
        } else {
            // Unquoted literal (technically invalid RDF, but accept gracefully)
            Ok(RdfTerm::Literal {
                value: value.clone(),
                datatype,
                language,
            })
        }
    }

    /// Normalize literal value by stripping quotes and unescaping
    ///
    /// Converts RDF-quoted literal to raw value for storage:
    /// - `"hello"` -> `hello`
    /// - `"John \"The Boss\""` -> `John "The Boss"`
    /// - `"line1\nline2"` -> `line1<newline>line2`
    ///
    /// ## Performance
    /// - Time: O(n) single pass
    /// - Space: O(n) for result
    fn normalize_literal_value(quoted: &str) -> Result<String> {
        if !quoted.starts_with('"') || !quoted.ends_with('"') {
            anyhow::bail!("Literal must be enclosed in quotes");
        }

        // Strip enclosing quotes
        let inner = &quoted[1..quoted.len() - 1];

        // Unescape standard RDF escape sequences
        let mut result = String::with_capacity(inner.len());
        let mut chars = inner.chars();

        while let Some(ch) = chars.next() {
            if ch == '\\' {
                // Escape sequence
                match chars.next() {
                    Some('"') => result.push('"'),
                    Some('\\') => result.push('\\'),
                    Some('n') => result.push('\n'),
                    Some('t') => result.push('\t'),
                    Some('r') => result.push('\r'),
                    Some('b') => result.push('\u{0008}'),
                    Some('f') => result.push('\u{000C}'),
                    Some(c) => {
                        // Unknown escape - keep backslash and character
                        result.push('\\');
                        result.push(c);
                    }
                    None => result.push('\\'),
                }
            } else {
                result.push(ch);
            }
        }

        Ok(result)
    }

    /// Serialize term as gRPC triple object
    ///
    /// Converts normalized RdfTerm to protobuf format for transmission.
    ///
    /// ## Returns
    /// (object_value, object_datatype, object_language) tuple for Triple proto
    ///
    /// ## Performance
    /// - Time: O(1) for URIs/blank nodes, O(n) for literals
    /// - Space: Reuses internal strings
    pub fn as_proto_object(&self) -> (String, String, String) {
        match self {
            RdfTerm::Uri(uri) => {
                // URIs are sent as raw URIs (shard expects "http://..." not "<http://...>")
                (uri.clone(), String::new(), String::new())
            }
            RdfTerm::Literal {
                value,
                datatype,
                language,
            } => {
                // Literals are sent as raw values (without quotes)
                // Oxigraph will handle quoting internally
                (value.clone(), datatype.clone(), language.clone())
            }
            RdfTerm::BlankNode(label) => {
                // Blank nodes are sent as-is
                (label.clone(), String::new(), String::new())
            }
        }
    }

    /// Validate term is safe for gRPC transmission
    ///
    /// Checks for issues that could cause protobuf serialization errors:
    /// - Empty values
    /// - Invalid UTF-8 sequences
    /// - Excessively large values
    ///
    /// ## Returns
    /// Ok(()) if valid, Err with details if invalid
    pub fn validate_for_grpc(&self) -> Result<()> {
        match self {
            RdfTerm::Uri(uri) => {
                if uri.is_empty() {
                    anyhow::bail!("Empty URI");
                }
                if uri.len() > 65_536 {
                    anyhow::bail!("URI exceeds 64KB limit");
                }
                // URI should be valid UTF-8 (enforced by Rust String type)
                Ok(())
            }
            RdfTerm::Literal {
                value,
                datatype,
                language,
            } => {
                // Value can be empty (valid for empty strings)
                if value.len() > 1_048_576 {
                    anyhow::bail!("Literal value exceeds 1MB limit");
                }
                if datatype.len() > 1024 {
                    anyhow::bail!("Datatype URI exceeds 1KB limit");
                }
                if language.len() > 35 {
                    // BCP 47 max length
                    anyhow::bail!("Language tag exceeds 35 character limit");
                }
                Ok(())
            }
            RdfTerm::BlankNode(label) => {
                if label.len() < 3 {
                    // Minimum "_:x"
                    anyhow::bail!("Blank node label too short");
                }
                if label.len() > 1024 {
                    anyhow::bail!("Blank node label exceeds 1KB limit");
                }
                Ok(())
            }
        }
    }

    /// Get term type as string (for debugging/logging)
    pub fn term_type(&self) -> &'static str {
        match self {
            RdfTerm::Uri(_) => "URI",
            RdfTerm::Literal { .. } => "Literal",
            RdfTerm::BlankNode(_) => "BlankNode",
        }
    }

    /// Get inner value (for debugging)
    pub fn value(&self) -> &str {
        match self {
            RdfTerm::Uri(uri) => uri,
            RdfTerm::Literal { value, .. } => value,
            RdfTerm::BlankNode(label) => label,
        }
    }
}

/// Build a validated Triple protobuf message from RDF term strings
///
/// Advanced triple builder with:
/// - Automatic term parsing and normalization
/// - Pre-transmission validation
/// - Optimized for gRPC serialization
///
/// ## Arguments
/// * `subject` - RDF subject string
/// * `predicate` - RDF predicate string
/// * `object` - RDF object string
/// * `graph` - Optional named graph URI
///
/// ## Returns
/// Validated Triple protobuf message ready for transmission
///
/// ## Errors
/// - Parse errors for invalid RDF syntax
/// - Validation errors for gRPC incompatible values
///
/// ## Performance
/// - Time: O(n + m + p) where n,m,p are term lengths
/// - Space: O(n + m + p) for parsed terms
pub fn build_validated_triple(
    subject: &str,
    predicate: &str,
    object: &str,
    graph: Option<&str>,
) -> Result<Triple> {
    // Parse and normalize each term
    let subject_term =
        RdfTerm::parse(subject).with_context(|| format!("Invalid subject: {}", subject))?;

    let predicate_term =
        RdfTerm::parse(predicate).with_context(|| format!("Invalid predicate: {}", predicate))?;

    let object_term =
        RdfTerm::parse(object).with_context(|| format!("Invalid object: {}", object))?;

    // Validate terms for gRPC transmission
    subject_term
        .validate_for_grpc()
        .with_context(|| format!("Subject validation failed: {}", subject))?;

    predicate_term
        .validate_for_grpc()
        .with_context(|| format!("Predicate validation failed: {}", predicate))?;

    object_term
        .validate_for_grpc()
        .with_context(|| format!("Object validation failed: {}", object))?;

    // Serialize terms to proto format
    // Shard expects raw URIs (e.g., "http://..." not "<http://...>")
    let subject_value = match subject_term {
        RdfTerm::Uri(uri) => uri,
        RdfTerm::BlankNode(label) => label,
        RdfTerm::Literal { .. } => {
            anyhow::bail!("Subject cannot be a literal");
        }
    };

    let predicate_value = match predicate_term {
        RdfTerm::Uri(uri) => uri,
        _ => {
            anyhow::bail!("Predicate must be a URI");
        }
    };

    let (object_value, object_datatype, object_language) = object_term.as_proto_object();

    Ok(Triple {
        subject: subject_value,
        predicate: predicate_value,
        object: object_value,
        object_datatype,
        object_language,
        graph: graph.unwrap_or_default().to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_uri() {
        let term = RdfTerm::parse("<http://example.com/resource>").unwrap();
        assert_eq!(
            term,
            RdfTerm::Uri("http://example.com/resource".to_string())
        );
    }

    #[test]
    fn test_parse_literal_plain() {
        let term = RdfTerm::parse("\"hello world\"").unwrap();
        match term {
            RdfTerm::Literal {
                value,
                datatype,
                language,
            } => {
                assert_eq!(value, "hello world");
                assert_eq!(datatype, "");
                assert_eq!(language, "");
            }
            _ => panic!("Expected Literal"),
        }
    }

    #[test]
    fn test_parse_literal_with_escapes() {
        let term = RdfTerm::parse(r#""John \"The Boss\" Smith""#).unwrap();
        match term {
            RdfTerm::Literal { value, .. } => {
                assert_eq!(value, "John \"The Boss\" Smith");
            }
            _ => panic!("Expected Literal"),
        }
    }

    #[test]
    fn test_parse_literal_typed() {
        let term = RdfTerm::parse(r#""42"^^<http://www.w3.org/2001/XMLSchema#integer>"#).unwrap();
        match term {
            RdfTerm::Literal {
                value,
                datatype,
                language,
            } => {
                assert_eq!(value, "42");
                assert_eq!(datatype, "http://www.w3.org/2001/XMLSchema#integer");
                assert_eq!(language, "");
            }
            _ => panic!("Expected Literal"),
        }
    }

    #[test]
    fn test_parse_literal_language_tagged() {
        let term = RdfTerm::parse(r#""hello"@en"#).unwrap();
        match term {
            RdfTerm::Literal {
                value,
                datatype,
                language,
            } => {
                assert_eq!(value, "hello");
                assert_eq!(datatype, "");
                assert_eq!(language, "en");
            }
            _ => panic!("Expected Literal"),
        }
    }

    #[test]
    fn test_parse_blank_node() {
        let term = RdfTerm::parse("_:b1").unwrap();
        assert_eq!(term, RdfTerm::BlankNode("_:b1".to_string()));
    }

    #[test]
    fn test_proto_serialization_uri() {
        let term = RdfTerm::Uri("http://example.com/resource".to_string());
        let (value, dt, lang) = term.as_proto_object();
        assert_eq!(value, "http://example.com/resource"); // Raw URI without angle brackets
        assert_eq!(dt, "");
        assert_eq!(lang, "");
    }

    #[test]
    fn test_proto_serialization_literal() {
        let term = RdfTerm::Literal {
            value: "hello world".to_string(),
            datatype: String::new(),
            language: String::new(),
        };
        let (value, dt, lang) = term.as_proto_object();
        assert_eq!(value, "hello world"); // No quotes!
        assert_eq!(dt, "");
        assert_eq!(lang, "");
    }

    #[test]
    fn test_validate_empty_uri() {
        let term = RdfTerm::Uri(String::new());
        assert!(term.validate_for_grpc().is_err());
    }

    #[test]
    fn test_validate_oversized_literal() {
        let term = RdfTerm::Literal {
            value: "x".repeat(2_000_000),
            datatype: String::new(),
            language: String::new(),
        };
        assert!(term.validate_for_grpc().is_err());
    }

    #[test]
    fn test_build_validated_triple() {
        let triple = build_validated_triple(
            "<http://example.com/alice>",
            "<http://example.com/knows>",
            "<http://example.com/bob>",
            None,
        )
        .unwrap();

        assert_eq!(triple.subject, "http://example.com/alice"); // Angle brackets stripped
        assert_eq!(triple.predicate, "http://example.com/knows");
        assert_eq!(triple.object, "http://example.com/bob");
    }

    #[test]
    fn test_build_triple_with_literal_object() {
        let triple = build_validated_triple(
            "<http://example.com/alice>",
            "<http://example.com/name>",
            "\"Alice Smith\"",
            None,
        )
        .unwrap();

        // URIs are stored without angle brackets (raw format for proto)
        assert_eq!(triple.subject, "http://example.com/alice");
        assert_eq!(triple.predicate, "http://example.com/name");
        assert_eq!(triple.object, "Alice Smith"); // Quotes stripped!
    }

    #[test]
    fn test_build_triple_with_typed_literal() {
        let triple = build_validated_triple(
            "<http://example.com/alice>",
            "<http://example.com/age>",
            "\"30\"^^<http://www.w3.org/2001/XMLSchema#integer>",
            None,
        )
        .unwrap();

        assert_eq!(triple.object, "30");
        assert_eq!(
            triple.object_datatype,
            "http://www.w3.org/2001/XMLSchema#integer"
        );
    }

    #[test]
    fn test_literal_subject_rejected() {
        let result = build_validated_triple(
            "\"literal subject\"",
            "<http://example.com/pred>",
            "<http://example.com/obj>",
            None,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_normalize_newline_escape() {
        let normalized = RdfTerm::normalize_literal_value(r#""line1\nline2""#).unwrap();
        assert_eq!(normalized, "line1\nline2");
    }

    #[test]
    fn test_normalize_tab_escape() {
        let normalized = RdfTerm::normalize_literal_value(r#""col1\tcol2""#).unwrap();
        assert_eq!(normalized, "col1\tcol2");
    }
}
