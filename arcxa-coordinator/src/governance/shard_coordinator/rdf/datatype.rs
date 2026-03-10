//! XSD Datatype Validation
//!
//! Validates and recognizes common XSD (XML Schema Definition) datatypes
//! used in RDF literals according to W3C XML Schema Part 2.
//!
//! ## Supported Datatypes
//!
//! ### Numeric Types
//! - `xsd:integer`, `xsd:int`, `xsd:long`, `xsd:short`, `xsd:byte`
//! - `xsd:decimal`, `xsd:float`, `xsd:double`
//! - `xsd:positiveInteger`, `xsd:negativeInteger`
//! - `xsd:nonPositiveInteger`, `xsd:nonNegativeInteger`
//!
//! ### String Types
//! - `xsd:string`, `xsd:normalizedString`, `xsd:token`
//!
//! ### Boolean
//! - `xsd:boolean`
//!
//! ### Date/Time Types
//! - `xsd:dateTime`, `xsd:date`, `xsd:time`
//! - `xsd:gYear`, `xsd:gYearMonth`, `xsd:gMonthDay`
//!
//! ### Binary
//! - `xsd:hexBinary`, `xsd:base64Binary`

/// Check if a datatype IRI is a recognized XSD datatype
///
/// Accepts both full IRIs and prefixed names:
/// - Full: `http://www.w3.org/2001/XMLSchema#integer`
/// - Prefixed: `xsd:integer`
///
/// ## Arguments
///
/// * `datatype` - Datatype IRI or prefixed name
///
/// ## Returns
///
/// `true` if recognized XSD datatype, `false` otherwise
///
/// ## Performance
///
/// - Time: O(n) where n = datatype string length
/// - Space: O(1)
/// - Typical: < 50ns
pub fn is_valid_xsd_datatype(datatype: &str) -> bool {
    // Extract local name from IRI or use prefixed name directly
    let local_name = if let Some(hash_pos) = datatype.rfind('#') {
        // Full IRI: http://www.w3.org/2001/XMLSchema#integer
        &datatype[hash_pos + 1..]
    } else if datatype.starts_with("xsd:") {
        // Prefixed name: xsd:integer
        &datatype[4..]
    } else {
        // Use as-is
        datatype
    };

    matches!(
        local_name,
        // Numeric types
        "integer" | "int" | "long" | "short" | "byte" |
        "decimal" | "float" | "double" |
        "positiveInteger" | "negativeInteger" |
        "nonPositiveInteger" | "nonNegativeInteger" |
        "unsignedLong" | "unsignedInt" | "unsignedShort" | "unsignedByte" |

        // String types
        "string" | "normalizedString" | "token" |

        // Boolean
        "boolean" |

        // Date/Time types
        "dateTime" | "date" | "time" |
        "gYear" | "gYearMonth" | "gMonthDay" | "gDay" | "gMonth" |
        "duration" |

        // Binary types
        "hexBinary" | "base64Binary" |

        // URI type
        "anyURI"
    )
}

/// Get the full XSD datatype IRI from a local name or prefixed name
///
/// ## Arguments
///
/// * `datatype` - Local name (e.g., "integer") or prefixed name (e.g., "xsd:integer")
///
/// ## Returns
///
/// Full IRI if valid XSD datatype, None otherwise
///
/// ## Example
///
/// ```ignore
/// use graphica_coordinator::governance::shard_coordinator::rdf::datatype::expand_xsd_datatype;
///
/// assert_eq!(
///     expand_xsd_datatype("xsd:integer"),
///     Some("http://www.w3.org/2001/XMLSchema#integer".to_string())
/// );
/// ```
pub fn expand_xsd_datatype(datatype: &str) -> Option<String> {
    let local_name = if datatype.starts_with("xsd:") {
        &datatype[4..]
    } else {
        datatype
    };

    if is_valid_xsd_datatype(local_name) {
        Some(format!("http://www.w3.org/2001/XMLSchema#{}", local_name))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_numeric_types() {
        assert!(is_valid_xsd_datatype("integer"));
        assert!(is_valid_xsd_datatype("int"));
        assert!(is_valid_xsd_datatype("long"));
        assert!(is_valid_xsd_datatype("short"));
        assert!(is_valid_xsd_datatype("byte"));
        assert!(is_valid_xsd_datatype("decimal"));
        assert!(is_valid_xsd_datatype("float"));
        assert!(is_valid_xsd_datatype("double"));
    }

    #[test]
    fn test_string_types() {
        assert!(is_valid_xsd_datatype("string"));
        assert!(is_valid_xsd_datatype("normalizedString"));
        assert!(is_valid_xsd_datatype("token"));
    }

    #[test]
    fn test_boolean_type() {
        assert!(is_valid_xsd_datatype("boolean"));
    }

    #[test]
    fn test_datetime_types() {
        assert!(is_valid_xsd_datatype("dateTime"));
        assert!(is_valid_xsd_datatype("date"));
        assert!(is_valid_xsd_datatype("time"));
        assert!(is_valid_xsd_datatype("gYear"));
        assert!(is_valid_xsd_datatype("gYearMonth"));
    }

    #[test]
    fn test_binary_types() {
        assert!(is_valid_xsd_datatype("hexBinary"));
        assert!(is_valid_xsd_datatype("base64Binary"));
    }

    #[test]
    fn test_full_iri() {
        assert!(is_valid_xsd_datatype(
            "http://www.w3.org/2001/XMLSchema#integer"
        ));
        assert!(is_valid_xsd_datatype(
            "http://www.w3.org/2001/XMLSchema#string"
        ));
        assert!(is_valid_xsd_datatype(
            "http://www.w3.org/2001/XMLSchema#boolean"
        ));
    }

    #[test]
    fn test_prefixed_name() {
        assert!(is_valid_xsd_datatype("xsd:integer"));
        assert!(is_valid_xsd_datatype("xsd:string"));
        assert!(is_valid_xsd_datatype("xsd:boolean"));
        assert!(is_valid_xsd_datatype("xsd:dateTime"));
    }

    #[test]
    fn test_invalid_datatypes() {
        assert!(!is_valid_xsd_datatype("notARealType"));
        assert!(!is_valid_xsd_datatype("Integer")); // Case sensitive
        assert!(!is_valid_xsd_datatype("xsd:notReal"));
        assert!(!is_valid_xsd_datatype(""));
    }

    #[test]
    fn test_expand_xsd_datatype() {
        assert_eq!(
            expand_xsd_datatype("integer"),
            Some("http://www.w3.org/2001/XMLSchema#integer".to_string())
        );
        assert_eq!(
            expand_xsd_datatype("xsd:integer"),
            Some("http://www.w3.org/2001/XMLSchema#integer".to_string())
        );
        assert_eq!(
            expand_xsd_datatype("string"),
            Some("http://www.w3.org/2001/XMLSchema#string".to_string())
        );
    }

    #[test]
    fn test_expand_invalid() {
        assert_eq!(expand_xsd_datatype("notReal"), None);
        assert_eq!(expand_xsd_datatype("xsd:notReal"), None);
    }
}
