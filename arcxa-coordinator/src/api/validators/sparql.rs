//! SPARQL Validation
//!
//! Validation logic for SPARQL queries and updates.

/// Validate SPARQL query is safe and well-formed
pub fn validate_sparql_query(sparql: &str) -> Result<(), String> {
    // Check for empty query
    if sparql.trim().is_empty() {
        return Err("Query cannot be empty".to_string());
    }

    // Check query length (prevent extremely long queries)
    const MAX_QUERY_LENGTH: usize = 10_000;
    if sparql.len() > MAX_QUERY_LENGTH {
        return Err(format!(
            "Query too long ({} chars). Max: {}",
            sparql.len(),
            MAX_QUERY_LENGTH
        ));
    }

    // Disallow dangerous operations (DELETE, DROP, CLEAR, LOAD)
    let sparql_upper = sparql.to_uppercase();
    let dangerous_ops = [
        "DELETE WHERE",
        "DELETE DATA",
        "DROP GRAPH",
        "DROP ALL",
        "CLEAR GRAPH",
        "CLEAR ALL",
        "LOAD ",
        "CREATE GRAPH",
    ];

    for op in &dangerous_ops {
        if sparql_upper.contains(op) {
            return Err(format!(
                "Operation '{}' not allowed in queries. Use UPDATE endpoint instead.",
                op
            ));
        }
    }

    // Ensure it's a valid SPARQL operation (allow INSERT for testing)
    // Note: SPARQL queries may have PREFIX declarations before the query verb
    if !sparql_upper.contains("SELECT")
        && !sparql_upper.contains("CONSTRUCT")
        && !sparql_upper.contains("ASK")
        && !sparql_upper.contains("DESCRIBE")
        && !sparql_upper.contains("INSERT")
    {
        return Err("Query must contain SELECT, CONSTRUCT, ASK, DESCRIBE, or INSERT".to_string());
    }

    Ok(())
}

/// Check if query is too complex (heuristic-based)
pub fn is_query_too_complex(sparql: &str) -> bool {
    let sparql_upper = sparql.to_uppercase();

    // Flag queries without LIMIT clause (unbounded)
    // Exception: ASK, INSERT, and aggregate queries (COUNT, SUM, etc.) don't need LIMIT
    let is_aggregate = sparql_upper.contains("COUNT(")
        || sparql_upper.contains("SUM(")
        || sparql_upper.contains("AVG(")
        || sparql_upper.contains("MIN(")
        || sparql_upper.contains("MAX(");

    if !sparql_upper.contains("ASK")
        && !sparql_upper.contains("INSERT")
        && !is_aggregate
        && !sparql_upper.contains("LIMIT")
    {
        tracing::warn!("Query without LIMIT clause detected");
        return true;
    }

    // Flag queries with very high LIMIT
    if let Some(limit_pos) = sparql_upper.find("LIMIT") {
        let after_limit = &sparql[limit_pos + 5..];
        if let Some(num_str) = after_limit.split_whitespace().next() {
            if let Ok(limit_value) = num_str.parse::<usize>() {
                const MAX_LIMIT: usize = 10_000;
                if limit_value > MAX_LIMIT {
                    tracing::warn!("LIMIT {} exceeds maximum {}", limit_value, MAX_LIMIT);
                    return true;
                }
            }
        }
    }

    // Flag queries with excessive OPTIONAL clauses (can be expensive)
    let optional_count = sparql_upper.matches("OPTIONAL").count();
    if optional_count > 10 {
        tracing::warn!(
            "Query has {} OPTIONAL clauses, may be too complex",
            optional_count
        );
        return true;
    }

    false
}
