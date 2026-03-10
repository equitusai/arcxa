//! In-Memory RDF Store for Testing
//!
//! Provides a simple in-memory RDF store implementation for unit and integration tests.
//! Not suitable for production use.

use anyhow::{anyhow, Result};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use super::rdf_store::{NamedGraph, RdfStore};

/// A triple in the RDF store
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Triple {
    pub subject: String,
    pub predicate: String,
    pub object: String,
    pub graph: String,
}

/// In-memory RDF store for testing
pub struct InMemoryRdfStore {
    /// Triples stored by graph URI
    triples: Arc<RwLock<HashMap<String, Vec<Triple>>>>,
}

impl InMemoryRdfStore {
    /// Create a new in-memory RDF store
    pub fn new() -> Self {
        Self {
            triples: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Get all triples in the store (for debugging)
    pub fn get_all_triples(&self) -> Result<Vec<Triple>> {
        let store = self
            .triples
            .read()
            .map_err(|e| anyhow!("Failed to acquire read lock: {}", e))?;

        Ok(store.values().flatten().cloned().collect())
    }

    /// Get triples from a specific graph
    pub fn get_triples_in_graph(&self, graph: &NamedGraph) -> Result<Vec<Triple>> {
        let store = self
            .triples
            .read()
            .map_err(|e| anyhow!("Failed to acquire read lock: {}", e))?;

        Ok(store.get(&graph.uri).cloned().unwrap_or_default())
    }

    /// Find triples matching a pattern (None means wildcard)
    pub fn find_triples(
        &self,
        subject: Option<&str>,
        predicate: Option<&str>,
        object: Option<&str>,
        graph: Option<&NamedGraph>,
    ) -> Result<Vec<Triple>> {
        let store = self
            .triples
            .read()
            .map_err(|e| anyhow!("Failed to acquire read lock: {}", e))?;

        let mut results = Vec::new();

        // Iterate over graphs
        let graphs_to_search: Vec<String> = if let Some(g) = graph {
            vec![g.uri.clone()]
        } else {
            store.keys().cloned().collect()
        };

        for graph_uri in graphs_to_search {
            if let Some(graph_triples) = store.get(&graph_uri) {
                for triple in graph_triples {
                    let subject_match = subject.map(|s| s == triple.subject).unwrap_or(true);
                    let predicate_match = predicate.map(|p| p == triple.predicate).unwrap_or(true);
                    let object_match = object.map(|o| o == triple.object).unwrap_or(true);

                    if subject_match && predicate_match && object_match {
                        results.push(triple.clone());
                    }
                }
            }
        }

        Ok(results)
    }

    /// Parse a simple SPARQL query and return results
    /// This is a very basic implementation for testing purposes only
    fn parse_simple_sparql(&self, sparql: &str) -> Result<Vec<JsonValue>> {
        // Very basic SPARQL parser for testing
        // Supports simple SELECT queries like:
        // SELECT ?s ?p ?o WHERE { ?s ?p ?o }

        let trimmed = sparql.trim();

        // Count triples queries
        if trimmed.contains("COUNT") {
            let count = self.count_all_triples()?;
            return Ok(vec![serde_json::json!({
                "count": count
            })]);
        }

        // Simple triple pattern matching
        // For now, just return all triples as JSON
        let all_triples = self.get_all_triples()?;

        let results: Vec<JsonValue> = all_triples
            .iter()
            .map(|t| {
                serde_json::json!({
                    "subject": t.subject,
                    "predicate": t.predicate,
                    "object": t.object,
                    "graph": t.graph,
                })
            })
            .collect();

        Ok(results)
    }

    /// Count all triples across all graphs
    fn count_all_triples(&self) -> Result<u64> {
        let store = self
            .triples
            .read()
            .map_err(|e| anyhow!("Failed to acquire read lock: {}", e))?;

        let total: usize = store.values().map(|v| v.len()).sum();
        Ok(total as u64)
    }

    /// Parse basic Turtle format (very simplified for testing)
    fn parse_turtle(&self, turtle: &str, graph_uri: &str) -> Result<Vec<Triple>> {
        let mut triples = Vec::new();
        let mut prefixes: HashMap<String, String> = HashMap::new();
        let mut current_subject: Option<String> = None;
        let mut current_blank: Option<String> = None;
        let mut blank_counter: usize = 0;

        prefixes.insert(
            "rdf".to_string(),
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#".to_string(),
        );
        prefixes.insert(
            "rdfs".to_string(),
            "http://www.w3.org/2000/01/rdf-schema#".to_string(),
        );
        prefixes.insert(
            "owl".to_string(),
            "http://www.w3.org/2002/07/owl#".to_string(),
        );
        prefixes.insert(
            "xsd".to_string(),
            "http://www.w3.org/2001/XMLSchema#".to_string(),
        );

        let expand_term = |term: &str, prefixes: &HashMap<String, String>| -> String {
            let term = term.trim().trim_end_matches(',');
            if term == "a" {
                return "http://www.w3.org/1999/02/22-rdf-syntax-ns#type".to_string();
            }
            if term.starts_with('<') && term.ends_with('>') && term.len() > 2 {
                return term[1..term.len() - 1].to_string();
            }
            if let Some(idx) = term.find(':') {
                let prefix = &term[..idx];
                let local = &term[idx + 1..];
                if let Some(base) = prefixes.get(prefix) {
                    return format!("{}{}", base, local);
                }
            }
            term.to_string()
        };

        let parse_object = |term: &str, prefixes: &HashMap<String, String>| -> String {
            let term = term.trim().trim_end_matches(',');
            if term.starts_with('"') {
                if let Some(end_quote) = term[1..].find('"') {
                    return term[1..1 + end_quote].to_string();
                }
                return term.trim_matches('"').to_string();
            }
            expand_term(term, prefixes)
        };

        let tokenize = |segment: &str| -> Vec<String> {
            let mut tokens = Vec::new();
            let mut current = String::new();
            let mut in_quotes = false;

            for ch in segment.chars() {
                if ch == '"' {
                    in_quotes = !in_quotes;
                    current.push(ch);
                    continue;
                }

                if ch.is_whitespace() && !in_quotes {
                    if !current.is_empty() {
                        tokens.push(current.clone());
                        current.clear();
                    }
                } else {
                    current.push(ch);
                }
            }

            if !current.is_empty() {
                tokens.push(current);
            }

            tokens
        };

        let detect_brackets = |segment: &str| -> (bool, bool) {
            let mut in_quotes = false;
            let mut has_start = false;
            let mut has_end = false;
            for ch in segment.chars() {
                if ch == '"' {
                    in_quotes = !in_quotes;
                    continue;
                }
                if !in_quotes {
                    if ch == '[' {
                        has_start = true;
                    } else if ch == ']' {
                        has_end = true;
                    }
                }
            }
            (has_start, has_end)
        };

        let strip_brackets = |segment: &str| -> String {
            let mut in_quotes = false;
            let mut cleaned = String::new();
            for ch in segment.chars() {
                if ch == '"' {
                    in_quotes = !in_quotes;
                    cleaned.push(ch);
                    continue;
                }
                if !in_quotes && (ch == '[' || ch == ']') {
                    continue;
                }
                cleaned.push(ch);
            }
            cleaned
        };

        for raw_line in turtle.lines() {
            let mut line = raw_line.trim();

            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            let lower = line.to_ascii_lowercase();
            if lower.starts_with("prefix") || lower.starts_with("@prefix") {
                let trimmed = line.trim_end_matches('.');
                let parts: Vec<&str> = trimmed.split_whitespace().collect();
                if parts.len() >= 3 {
                    let prefix = parts[1].trim_end_matches(':');
                    let uri = parts[2]
                        .trim()
                        .trim_start_matches('<')
                        .trim_end_matches('>');
                    if !prefix.is_empty() && !uri.is_empty() {
                        prefixes.insert(prefix.to_string(), uri.to_string());
                    }
                }
                continue;
            }

            if line.starts_with(']') {
                current_blank = None;
                line = line.trim_start_matches(']').trim();
                if line.is_empty() {
                    continue;
                }
            }

            let segments: Vec<&str> = line.split(';').collect();
            for (idx, segment) in segments.iter().enumerate() {
                let segment = segment.trim().trim_end_matches('.');
                if segment.is_empty() {
                    continue;
                }

                let (has_blank_start, has_blank_end) = detect_brackets(segment);
                let cleaned_segment = strip_brackets(segment);
                let cleaned_segment = cleaned_segment.trim();

                if cleaned_segment.is_empty() {
                    if has_blank_end {
                        current_blank = None;
                    }
                    continue;
                }

                let tokens = tokenize(cleaned_segment);
                if tokens.is_empty() {
                    if has_blank_end {
                        current_blank = None;
                    }
                    continue;
                }

                if has_blank_start {
                    let (subject, pred_idx) = if tokens.len() >= 2 {
                        let subject = expand_term(&tokens[0], &prefixes);
                        current_subject = Some(subject.clone());
                        (subject, 1)
                    } else if let Some(subject) = current_subject.clone() {
                        (subject, 0)
                    } else {
                        continue;
                    };

                    if pred_idx >= tokens.len() {
                        continue;
                    }

                    let predicate = expand_term(&tokens[pred_idx], &prefixes);
                    let blank_id = format!("_:b{}", blank_counter);
                    blank_counter += 1;
                    current_blank = Some(blank_id.clone());

                    triples.push(Triple {
                        subject,
                        predicate,
                        object: blank_id.clone(),
                        graph: graph_uri.to_string(),
                    });

                    if tokens.len() >= pred_idx + 3 {
                        let predicate2 = expand_term(&tokens[pred_idx + 1], &prefixes);
                        let object2 = parse_object(&tokens[pred_idx + 2], &prefixes);
                        triples.push(Triple {
                            subject: blank_id,
                            predicate: predicate2,
                            object: object2,
                            graph: graph_uri.to_string(),
                        });
                    }

                    if has_blank_end {
                        current_blank = None;
                    }
                    continue;
                }

                let (subject, pred_idx, obj_idx) = if let Some(blank) = current_blank.clone() {
                    if tokens.len() >= 2 {
                        (blank, 0, 1)
                    } else {
                        continue;
                    }
                } else if tokens.len() >= 3 {
                    let subject = expand_term(&tokens[0], &prefixes);
                    current_subject = Some(subject.clone());
                    (subject, 1, 2)
                } else if let Some(subject) = current_subject.clone() {
                    (subject, 0, 1)
                } else {
                    continue;
                };

                if pred_idx >= tokens.len() || obj_idx >= tokens.len() {
                    continue;
                }

                let predicate = expand_term(&tokens[pred_idx], &prefixes);
                let object = parse_object(&tokens[obj_idx], &prefixes);

                triples.push(Triple {
                    subject,
                    predicate,
                    object,
                    graph: graph_uri.to_string(),
                });

                if has_blank_end {
                    current_blank = None;
                }
            }
        }

        Ok(triples)
    }
}

impl Default for InMemoryRdfStore {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for InMemoryRdfStore {
    fn clone(&self) -> Self {
        Self {
            triples: Arc::clone(&self.triples),
        }
    }
}

impl RdfStore for InMemoryRdfStore {
    fn insert_triple(
        &self,
        subject: &str,
        predicate: &str,
        object: &str,
        graph: Option<&NamedGraph>,
    ) -> Result<()> {
        let graph_uri = graph
            .map(|g| g.uri.clone())
            .unwrap_or_else(|| NamedGraph::current().uri);

        let triple = Triple {
            subject: subject.to_string(),
            predicate: predicate.to_string(),
            object: object.to_string(),
            graph: graph_uri.clone(),
        };

        let mut store = self
            .triples
            .write()
            .map_err(|e| anyhow!("Failed to acquire write lock: {}", e))?;

        store.entry(graph_uri).or_insert_with(Vec::new).push(triple);

        Ok(())
    }

    fn insert_triples(
        &self,
        triples: Vec<(String, String, String)>,
        graph: Option<&NamedGraph>,
    ) -> Result<()> {
        let graph_uri = graph
            .map(|g| g.uri.clone())
            .unwrap_or_else(|| NamedGraph::current().uri);

        let mut store = self
            .triples
            .write()
            .map_err(|e| anyhow!("Failed to acquire write lock: {}", e))?;

        let graph_triples = store.entry(graph_uri.clone()).or_insert_with(Vec::new);

        for (subject, predicate, object) in triples {
            graph_triples.push(Triple {
                subject,
                predicate,
                object,
                graph: graph_uri.clone(),
            });
        }

        Ok(())
    }

    fn query(&self, sparql: &str) -> Result<Vec<JsonValue>> {
        self.parse_simple_sparql(sparql)
    }

    fn update(&self, sparql_update: &str) -> Result<()> {
        // Very basic UPDATE support for testing
        // Just parse INSERT DATA blocks

        if sparql_update.contains("INSERT DATA") {
            // Extract the data block and parse as Turtle
            if let Some(start) = sparql_update.find('{') {
                if let Some(end) = sparql_update.rfind('}') {
                    let data = &sparql_update[start + 1..end];
                    let graph = NamedGraph::current();
                    return self.load_turtle(data, Some(&graph));
                }
            }
        }

        Ok(())
    }

    fn load_turtle(&self, turtle: &str, graph: Option<&NamedGraph>) -> Result<()> {
        // Use empty string as the default graph (for SPARQL compatibility)
        // This allows queries without GRAPH clauses to find triples
        let graph_uri = graph
            .map(|g| g.uri.clone())
            .unwrap_or_else(|| String::from(""));
        let triples = self.parse_turtle(turtle, &graph_uri)?;

        let mut store = self
            .triples
            .write()
            .map_err(|e| anyhow!("Failed to acquire write lock: {}", e))?;

        store
            .entry(graph_uri)
            .or_insert_with(Vec::new)
            .extend(triples);

        Ok(())
    }

    fn load_ontology(&self, turtle: &str) -> Result<()> {
        // Load into default graph
        self.load_turtle(turtle, None)
    }

    fn count_triples(&self, graph: Option<&NamedGraph>) -> Result<u64> {
        if let Some(g) = graph {
            let store = self
                .triples
                .read()
                .map_err(|e| anyhow!("Failed to acquire read lock: {}", e))?;

            Ok(store.get(&g.uri).map(|v| v.len() as u64).unwrap_or(0))
        } else {
            self.count_all_triples()
        }
    }

    fn clear_graph(&self, graph: &NamedGraph) -> Result<()> {
        let mut store = self
            .triples
            .write()
            .map_err(|e| anyhow!("Failed to acquire write lock: {}", e))?;

        store.remove(&graph.uri);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insert_and_retrieve_triple() {
        let store = InMemoryRdfStore::new();
        let graph = NamedGraph::current();

        store
            .insert_triple(
                "http://example.org/subject",
                "http://example.org/predicate",
                "http://example.org/object",
                Some(&graph),
            )
            .unwrap();

        let triples = store.get_triples_in_graph(&graph).unwrap();
        assert_eq!(triples.len(), 1);
        assert_eq!(triples[0].subject, "http://example.org/subject");
    }

    #[test]
    fn test_insert_batch() {
        let store = InMemoryRdfStore::new();
        let graph = NamedGraph::current();

        let batch = vec![
            ("s1".to_string(), "p1".to_string(), "o1".to_string()),
            ("s2".to_string(), "p2".to_string(), "o2".to_string()),
        ];

        store.insert_triples(batch, Some(&graph)).unwrap();

        let count = store.count_triples(Some(&graph)).unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn test_find_triples_with_pattern() {
        let store = InMemoryRdfStore::new();
        let graph = NamedGraph::current();

        store.insert_triple("s1", "p1", "o1", Some(&graph)).unwrap();
        store.insert_triple("s2", "p1", "o2", Some(&graph)).unwrap();
        store.insert_triple("s3", "p2", "o3", Some(&graph)).unwrap();

        // Find all triples with predicate "p1"
        let results = store
            .find_triples(None, Some("p1"), None, Some(&graph))
            .unwrap();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_clear_graph() {
        let store = InMemoryRdfStore::new();
        let graph = NamedGraph::current();

        store.insert_triple("s1", "p1", "o1", Some(&graph)).unwrap();
        assert_eq!(store.count_triples(Some(&graph)).unwrap(), 1);

        store.clear_graph(&graph).unwrap();
        assert_eq!(store.count_triples(Some(&graph)).unwrap(), 0);
    }

    #[test]
    fn test_multiple_graphs() {
        let store = InMemoryRdfStore::new();
        let graph1 = NamedGraph::current();
        let graph2 = NamedGraph::fusion();

        store
            .insert_triple("s1", "p1", "o1", Some(&graph1))
            .unwrap();
        store
            .insert_triple("s2", "p2", "o2", Some(&graph2))
            .unwrap();

        assert_eq!(store.count_triples(Some(&graph1)).unwrap(), 1);
        assert_eq!(store.count_triples(Some(&graph2)).unwrap(), 1);
        assert_eq!(store.count_triples(None).unwrap(), 2);
    }
}
