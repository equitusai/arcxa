//! SPARQL UPDATE Query Builder
//!
//! Safe construction of SPARQL UPDATE operations with automatic
//! URI validation and injection prevention.
//!
//! ## Supported Operations
//!
//! - `CLEAR GRAPH <uri>` - Remove all triples from a named graph
//! - `DROP GRAPH <uri>` - Remove a named graph completely
//! - `CREATE GRAPH <uri>` - Create a new named graph
//! - `COPY GRAPH <uri1> TO <uri2>` - Copy graph contents
//! - `MOVE GRAPH <uri1> TO <uri2>` - Move graph contents
//!
//! ## Performance
//! - Time: O(n) where n = URI length (validation)
//! - Space: O(n) for result string
//! - Typical: < 100ns per operation

use anyhow::{Context, Result};

use super::validator::is_valid_sparql_uri;

/// SPARQL UPDATE Query Builder
///
/// Provides safe construction of SPARQL UPDATE operations
/// with automatic URI validation.
pub struct SparqlUpdateBuilder;

impl SparqlUpdateBuilder {
    /// Build a CLEAR GRAPH operation
    ///
    /// Removes all triples from the specified named graph.
    /// The graph itself remains (unlike DROP GRAPH).
    ///
    /// ## Arguments
    /// * `graph_uri` - URI of the graph to clear
    ///
    /// ## Returns
    /// SPARQL UPDATE query string
    ///
    /// ## Errors
    /// - If `graph_uri` is invalid or contains injection vectors
    ///
    /// ## Example
    ///
    /// ```rust,no_run
    /// use graphica_coordinator::governance::shard_coordinator::sparql::SparqlUpdateBuilder;
    ///
    /// let sparql = SparqlUpdateBuilder::clear_graph("http://example.com/graph")
    ///     .expect("Valid graph URI");
    ///
    /// assert_eq!(sparql, "CLEAR GRAPH <http://example.com/graph>");
    /// ```
    pub fn clear_graph(graph_uri: &str) -> Result<String> {
        if !is_valid_sparql_uri(graph_uri) {
            anyhow::bail!("Invalid graph URI: {}", graph_uri);
        }

        Ok(format!("CLEAR GRAPH <{}>", graph_uri))
    }

    /// Build a DROP GRAPH operation
    ///
    /// Removes a named graph completely, including all triples.
    ///
    /// ## Arguments
    /// * `graph_uri` - URI of the graph to drop
    /// * `silent` - If true, don't error if graph doesn't exist
    ///
    /// ## Returns
    /// SPARQL UPDATE query string
    ///
    /// ## Errors
    /// - If `graph_uri` is invalid or contains injection vectors
    ///
    /// ## Example
    ///
    /// ```rust,no_run
    /// use graphica_coordinator::governance::shard_coordinator::sparql::SparqlUpdateBuilder;
    ///
    /// let sparql = SparqlUpdateBuilder::drop_graph("http://example.com/graph", true)
    ///     .expect("Valid graph URI");
    ///
    /// assert_eq!(sparql, "DROP SILENT GRAPH <http://example.com/graph>");
    /// ```
    pub fn drop_graph(graph_uri: &str, silent: bool) -> Result<String> {
        if !is_valid_sparql_uri(graph_uri) {
            anyhow::bail!("Invalid graph URI: {}", graph_uri);
        }

        let silent_clause = if silent { " SILENT" } else { "" };
        Ok(format!("DROP{} GRAPH <{}>", silent_clause, graph_uri))
    }

    /// Build a CREATE GRAPH operation
    ///
    /// Creates a new empty named graph.
    ///
    /// ## Arguments
    /// * `graph_uri` - URI of the graph to create
    ///
    /// ## Returns
    /// SPARQL UPDATE query string
    ///
    /// ## Errors
    /// - If `graph_uri` is invalid or contains injection vectors
    ///
    /// ## Example
    ///
    /// ```rust,no_run
    /// use graphica_coordinator::governance::shard_coordinator::sparql::SparqlUpdateBuilder;
    ///
    /// let sparql = SparqlUpdateBuilder::create_graph("http://example.com/graph")
    ///     .expect("Valid graph URI");
    ///
    /// assert_eq!(sparql, "CREATE GRAPH <http://example.com/graph>");
    /// ```
    pub fn create_graph(graph_uri: &str) -> Result<String> {
        if !is_valid_sparql_uri(graph_uri) {
            anyhow::bail!("Invalid graph URI: {}", graph_uri);
        }

        Ok(format!("CREATE GRAPH <{}>", graph_uri))
    }

    /// Build a COPY GRAPH operation
    ///
    /// Copies all triples from source graph to destination graph.
    /// Destination graph is cleared first.
    ///
    /// ## Arguments
    /// * `source_uri` - URI of the source graph
    /// * `dest_uri` - URI of the destination graph
    /// * `silent` - If true, don't error if source doesn't exist
    ///
    /// ## Returns
    /// SPARQL UPDATE query string
    ///
    /// ## Errors
    /// - If either URI is invalid or contains injection vectors
    ///
    /// ## Example
    ///
    /// ```rust,no_run
    /// use graphica_coordinator::governance::shard_coordinator::sparql::SparqlUpdateBuilder;
    ///
    /// let sparql = SparqlUpdateBuilder::copy_graph(
    ///     "http://example.com/source",
    ///     "http://example.com/dest",
    ///     false
    /// ).expect("Valid graph URIs");
    ///
    /// assert_eq!(
    ///     sparql,
    ///     "COPY GRAPH <http://example.com/source> TO <http://example.com/dest>"
    /// );
    /// ```
    pub fn copy_graph(source_uri: &str, dest_uri: &str, silent: bool) -> Result<String> {
        if !is_valid_sparql_uri(source_uri) {
            anyhow::bail!("Invalid source graph URI: {}", source_uri);
        }
        if !is_valid_sparql_uri(dest_uri) {
            anyhow::bail!("Invalid destination graph URI: {}", dest_uri);
        }

        let silent_clause = if silent { " SILENT" } else { "" };
        Ok(format!(
            "COPY{} GRAPH <{}> TO <{}>",
            silent_clause, source_uri, dest_uri
        ))
    }

    /// Build a MOVE GRAPH operation
    ///
    /// Moves all triples from source graph to destination graph.
    /// Source graph is removed after move.
    ///
    /// ## Arguments
    /// * `source_uri` - URI of the source graph
    /// * `dest_uri` - URI of the destination graph
    /// * `silent` - If true, don't error if source doesn't exist
    ///
    /// ## Returns
    /// SPARQL UPDATE query string
    ///
    /// ## Errors
    /// - If either URI is invalid or contains injection vectors
    ///
    /// ## Example
    ///
    /// ```rust,no_run
    /// use graphica_coordinator::governance::shard_coordinator::sparql::SparqlUpdateBuilder;
    ///
    /// let sparql = SparqlUpdateBuilder::move_graph(
    ///     "http://example.com/source",
    ///     "http://example.com/dest",
    ///     false
    /// ).expect("Valid graph URIs");
    ///
    /// assert_eq!(
    ///     sparql,
    ///     "MOVE GRAPH <http://example.com/source> TO <http://example.com/dest>"
    /// );
    /// ```
    pub fn move_graph(source_uri: &str, dest_uri: &str, silent: bool) -> Result<String> {
        if !is_valid_sparql_uri(source_uri) {
            anyhow::bail!("Invalid source graph URI: {}", source_uri);
        }
        if !is_valid_sparql_uri(dest_uri) {
            anyhow::bail!("Invalid destination graph URI: {}", dest_uri);
        }

        let silent_clause = if silent { " SILENT" } else { "" };
        Ok(format!(
            "MOVE{} GRAPH <{}> TO <{}>",
            silent_clause, source_uri, dest_uri
        ))
    }

    /// Build a CLEAR ALL operation
    ///
    /// Removes all triples from all named graphs (but keeps the graphs).
    /// Use with extreme caution!
    ///
    /// ## Returns
    /// SPARQL UPDATE query string
    ///
    /// ## Example
    ///
    /// ```rust,no_run
    /// use graphica_coordinator::governance::shard_coordinator::sparql::SparqlUpdateBuilder;
    ///
    /// let sparql = SparqlUpdateBuilder::clear_all();
    /// assert_eq!(sparql, "CLEAR ALL");
    /// ```
    pub fn clear_all() -> String {
        "CLEAR ALL".to_string()
    }

    /// Build a DROP ALL operation
    ///
    /// Removes all named graphs and their triples.
    /// Use with extreme caution!
    ///
    /// ## Returns
    /// SPARQL UPDATE query string
    ///
    /// ## Example
    ///
    /// ```rust,no_run
    /// use graphica_coordinator::governance::shard_coordinator::sparql::SparqlUpdateBuilder;
    ///
    /// let sparql = SparqlUpdateBuilder::drop_all();
    /// assert_eq!(sparql, "DROP ALL");
    /// ```
    pub fn drop_all() -> String {
        "DROP ALL".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clear_graph() {
        let sparql =
            SparqlUpdateBuilder::clear_graph("http://example.com/graph").expect("Valid URI");
        assert_eq!(sparql, "CLEAR GRAPH <http://example.com/graph>");
    }

    #[test]
    fn test_clear_graph_invalid_uri() {
        let result = SparqlUpdateBuilder::clear_graph("http://example.com/> DROP ALL");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Invalid graph URI"));
    }

    #[test]
    fn test_drop_graph() {
        let sparql =
            SparqlUpdateBuilder::drop_graph("http://example.com/graph", false).expect("Valid URI");
        assert_eq!(sparql, "DROP GRAPH <http://example.com/graph>");
    }

    #[test]
    fn test_drop_graph_silent() {
        let sparql =
            SparqlUpdateBuilder::drop_graph("http://example.com/graph", true).expect("Valid URI");
        assert_eq!(sparql, "DROP SILENT GRAPH <http://example.com/graph>");
    }

    #[test]
    fn test_create_graph() {
        let sparql =
            SparqlUpdateBuilder::create_graph("http://example.com/graph").expect("Valid URI");
        assert_eq!(sparql, "CREATE GRAPH <http://example.com/graph>");
    }

    #[test]
    fn test_copy_graph() {
        let sparql = SparqlUpdateBuilder::copy_graph(
            "http://example.com/source",
            "http://example.com/dest",
            false,
        )
        .expect("Valid URIs");
        assert_eq!(
            sparql,
            "COPY GRAPH <http://example.com/source> TO <http://example.com/dest>"
        );
    }

    #[test]
    fn test_copy_graph_silent() {
        let sparql = SparqlUpdateBuilder::copy_graph(
            "http://example.com/source",
            "http://example.com/dest",
            true,
        )
        .expect("Valid URIs");
        assert_eq!(
            sparql,
            "COPY SILENT GRAPH <http://example.com/source> TO <http://example.com/dest>"
        );
    }

    #[test]
    fn test_copy_graph_invalid_source() {
        let result = SparqlUpdateBuilder::copy_graph(
            "http://example.com/> DROP",
            "http://example.com/dest",
            false,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_copy_graph_invalid_dest() {
        let result = SparqlUpdateBuilder::copy_graph(
            "http://example.com/source",
            "http://example.com/> DROP",
            false,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_move_graph() {
        let sparql = SparqlUpdateBuilder::move_graph(
            "http://example.com/source",
            "http://example.com/dest",
            false,
        )
        .expect("Valid URIs");
        assert_eq!(
            sparql,
            "MOVE GRAPH <http://example.com/source> TO <http://example.com/dest>"
        );
    }

    #[test]
    fn test_clear_all() {
        let sparql = SparqlUpdateBuilder::clear_all();
        assert_eq!(sparql, "CLEAR ALL");
    }

    #[test]
    fn test_drop_all() {
        let sparql = SparqlUpdateBuilder::drop_all();
        assert_eq!(sparql, "DROP ALL");
    }
}
