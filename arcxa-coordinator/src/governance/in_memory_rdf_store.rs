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

#[derive(Debug, Clone, PartialEq, Eq)]
enum QueryTerm {
    Variable(String),
    Constant(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TriplePosition {
    Subject,
    Predicate,
    Object,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SimpleTriplePattern {
    subject: QueryTerm,
    predicate: QueryTerm,
    object: QueryTerm,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SimpleSelectQuery {
    projections: Vec<String>,
    pattern: SimpleTriplePattern,
    graph: Option<String>,
    limit: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SimpleAskQuery {
    pattern: SimpleTriplePattern,
    graph: Option<String>,
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
        let trimmed = sparql.trim();
        let (prefixes, query_body) = self.parse_prefixes(trimmed);
        let normalized = query_body.trim();
        let upper = normalized.to_ascii_uppercase();

        // Count triples queries
        if upper.contains("COUNT") {
            let count = self.count_all_triples()?;
            return Ok(vec![serde_json::json!({
                "count": count
            })]);
        }

        if upper.starts_with("ASK") {
            let query = self.parse_simple_ask_query(normalized, &prefixes)?;
            return self.execute_simple_ask_query(&query);
        }

        let query = self.parse_simple_select_query(normalized, &prefixes)?;
        self.execute_simple_select_query(&query)
    }

    fn parse_prefixes(&self, sparql: &str) -> (HashMap<String, String>, String) {
        let mut prefixes = HashMap::new();
        let mut remaining_lines = Vec::new();

        for line in sparql.lines() {
            let trimmed = line.trim();
            let upper = trimmed.to_ascii_uppercase();
            if upper.starts_with("PREFIX ") || upper.starts_with("@PREFIX ") {
                let parts = trimmed
                    .trim_end_matches('.')
                    .split_whitespace()
                    .collect::<Vec<_>>();
                if parts.len() >= 3 {
                    let prefix = parts[1].trim_end_matches(':');
                    let value = parts[2].trim();
                    if value.starts_with('<') && value.ends_with('>') && value.len() > 2 {
                        prefixes.insert(prefix.to_string(), value[1..value.len() - 1].to_string());
                    }
                }
                continue;
            }

            if !trimmed.is_empty() {
                remaining_lines.push(trimmed.to_string());
            }
        }

        (prefixes, remaining_lines.join("\n"))
    }

    fn parse_simple_select_query(
        &self,
        sparql: &str,
        prefixes: &HashMap<String, String>,
    ) -> Result<SimpleSelectQuery> {
        let sparql = sparql.trim();
        let upper = sparql.to_ascii_uppercase();

        let where_idx = upper
            .find("WHERE")
            .ok_or_else(|| anyhow!("Unsupported SPARQL query: missing WHERE clause"))?;
        if !upper.starts_with("SELECT ") {
            return Err(anyhow!(
                "Unsupported SPARQL query: only SELECT is supported"
            ));
        }

        let projections = sparql["SELECT ".len()..where_idx]
            .split_whitespace()
            .filter_map(|token| token.strip_prefix('?').map(|name| name.to_string()))
            .collect::<Vec<_>>();
        if projections.is_empty() {
            return Err(anyhow!("Unsupported SPARQL query: missing projections"));
        }

        let where_body = self.extract_where_body(&sparql[where_idx + "WHERE".len()..])?;
        let (graph, pattern_source) = self.parse_optional_graph_clause(where_body)?;
        let pattern = self.parse_triple_pattern(pattern_source, prefixes)?;
        let limit = self.parse_limit_clause(sparql)?;

        Ok(SimpleSelectQuery {
            projections,
            pattern,
            graph,
            limit,
        })
    }

    fn parse_simple_ask_query(
        &self,
        sparql: &str,
        prefixes: &HashMap<String, String>,
    ) -> Result<SimpleAskQuery> {
        let sparql = sparql.trim();
        let upper = sparql.to_ascii_uppercase();
        if !upper.starts_with("ASK") {
            return Err(anyhow!("Unsupported SPARQL query: only ASK is supported"));
        }

        let body_source = if let Some(where_idx) = upper.find("WHERE") {
            &sparql[where_idx + "WHERE".len()..]
        } else {
            &sparql["ASK".len()..]
        };

        let where_body = self.extract_where_body(body_source)?;
        let (graph, pattern_source) = self.parse_optional_graph_clause(where_body)?;
        let pattern = self.parse_triple_pattern(pattern_source, prefixes)?;

        Ok(SimpleAskQuery { pattern, graph })
    }

    fn extract_where_body<'a>(&self, input: &'a str) -> Result<&'a str> {
        let open_idx = input
            .find('{')
            .ok_or_else(|| anyhow!("Unsupported SPARQL query: missing opening brace"))?;
        let close_idx = input
            .rfind('}')
            .ok_or_else(|| anyhow!("Unsupported SPARQL query: missing closing brace"))?;
        if close_idx <= open_idx {
            return Err(anyhow!("Unsupported SPARQL query: malformed WHERE body"));
        }

        Ok(input[open_idx + 1..close_idx].trim())
    }

    fn parse_optional_graph_clause<'a>(&self, body: &'a str) -> Result<(Option<String>, &'a str)> {
        let trimmed = body.trim();
        let upper = trimmed.to_ascii_uppercase();
        if !upper.starts_with("GRAPH ") {
            return Ok((None, trimmed));
        }

        let uri_start = trimmed
            .find('<')
            .ok_or_else(|| anyhow!("Unsupported GRAPH clause: missing graph URI"))?;
        let uri_end = trimmed[uri_start + 1..]
            .find('>')
            .ok_or_else(|| anyhow!("Unsupported GRAPH clause: missing graph URI terminator"))?
            + uri_start
            + 1;
        let graph_uri = trimmed[uri_start + 1..uri_end].to_string();

        let inner = self.extract_where_body(&trimmed[uri_end + 1..])?;
        Ok((Some(graph_uri), inner))
    }

    fn parse_triple_pattern(
        &self,
        pattern: &str,
        prefixes: &HashMap<String, String>,
    ) -> Result<SimpleTriplePattern> {
        let normalized = pattern.trim().trim_end_matches('.').trim();
        let tokens = self.tokenize_pattern(normalized);
        if tokens.len() != 3 {
            return Err(anyhow!(
                "Unsupported SPARQL query: expected one triple pattern, got {:?}",
                tokens
            ));
        }

        Ok(SimpleTriplePattern {
            subject: self.parse_query_term(&tokens[0], prefixes),
            predicate: self.parse_query_term(&tokens[1], prefixes),
            object: self.parse_query_term(&tokens[2], prefixes),
        })
    }

    fn tokenize_pattern(&self, pattern: &str) -> Vec<String> {
        let mut tokens = Vec::new();
        let mut current = String::new();
        let mut in_quotes = false;
        let mut in_uri = false;

        for ch in pattern.chars() {
            match ch {
                '"' => {
                    in_quotes = !in_quotes;
                    current.push(ch);
                }
                '<' if !in_quotes => {
                    in_uri = true;
                    current.push(ch);
                }
                '>' if in_uri && !in_quotes => {
                    in_uri = false;
                    current.push(ch);
                }
                c if c.is_whitespace() && !in_quotes && !in_uri => {
                    if !current.is_empty() {
                        tokens.push(current.clone());
                        current.clear();
                    }
                }
                _ => current.push(ch),
            }
        }

        if !current.is_empty() {
            tokens.push(current);
        }

        tokens
    }

    fn parse_query_term(&self, token: &str, prefixes: &HashMap<String, String>) -> QueryTerm {
        if let Some(variable) = token.strip_prefix('?') {
            return QueryTerm::Variable(variable.to_string());
        }

        let constant = if token.starts_with('<') && token.ends_with('>') && token.len() > 2 {
            token[1..token.len() - 1].to_string()
        } else if !token.starts_with('"') {
            if let Some((prefix, local)) = token.split_once(':') {
                if let Some(base) = prefixes.get(prefix) {
                    format!("{}{}", base, local)
                } else {
                    token.to_string()
                }
            } else {
                token.to_string()
            }
        } else {
            token.to_string()
        };
        QueryTerm::Constant(constant)
    }

    fn parse_limit_clause(&self, sparql: &str) -> Result<Option<usize>> {
        let upper = sparql.to_ascii_uppercase();
        let Some(limit_idx) = upper.rfind("LIMIT") else {
            return Ok(None);
        };

        let limit_str = sparql[limit_idx + "LIMIT".len()..]
            .split_whitespace()
            .next()
            .ok_or_else(|| anyhow!("Invalid LIMIT clause"))?;
        let limit = limit_str
            .parse::<usize>()
            .map_err(|e| anyhow!("Invalid LIMIT value '{}': {}", limit_str, e))?;
        Ok(Some(limit))
    }

    fn execute_simple_select_query(&self, query: &SimpleSelectQuery) -> Result<Vec<JsonValue>> {
        let subject = match &query.pattern.subject {
            QueryTerm::Constant(value) => Some(value.as_str()),
            QueryTerm::Variable(_) => None,
        };
        let predicate = match &query.pattern.predicate {
            QueryTerm::Constant(value) => Some(value.as_str()),
            QueryTerm::Variable(_) => None,
        };

        let graph = query.graph.as_ref().map(NamedGraph::new);
        let triples = self.find_triples(subject, predicate, None, graph.as_ref())?;
        let mut results = Vec::with_capacity(triples.len());

        for triple in triples {
            if !self.triple_matches_query(&query.pattern, &triple) {
                continue;
            }

            let mut row = serde_json::Map::new();
            for projection in &query.projections {
                match projection.as_str() {
                    name if matches!(query.pattern.subject, QueryTerm::Variable(ref var) if var == name) =>
                    {
                        row.insert(name.to_string(), JsonValue::String(triple.subject.clone()));
                    }
                    name if matches!(query.pattern.predicate, QueryTerm::Variable(ref var) if var == name) =>
                    {
                        row.insert(
                            name.to_string(),
                            JsonValue::String(triple.predicate.clone()),
                        );
                    }
                    name if matches!(query.pattern.object, QueryTerm::Variable(ref var) if var == name) =>
                    {
                        row.insert(name.to_string(), JsonValue::String(triple.object.clone()));
                    }
                    "graph" => {
                        row.insert("graph".to_string(), JsonValue::String(triple.graph.clone()));
                    }
                    _ => {}
                }
            }
            results.push(JsonValue::Object(row));
        }

        if let Some(limit) = query.limit {
            results.truncate(limit);
        }

        Ok(results)
    }

    fn execute_simple_ask_query(&self, query: &SimpleAskQuery) -> Result<Vec<JsonValue>> {
        let subject = match &query.pattern.subject {
            QueryTerm::Constant(value) => Some(value.as_str()),
            QueryTerm::Variable(_) => None,
        };
        let predicate = match &query.pattern.predicate {
            QueryTerm::Constant(value) => Some(value.as_str()),
            QueryTerm::Variable(_) => None,
        };

        let graph = query.graph.as_ref().map(NamedGraph::new);
        let triples = self.find_triples(subject, predicate, None, graph.as_ref())?;
        let matched = triples
            .iter()
            .any(|triple| self.triple_matches_query(&query.pattern, triple));

        Ok(vec![serde_json::json!({
            "ASK": matched
        })])
    }

    fn execute_delete_where_update(&self, sparql_update: &str) -> Result<()> {
        let trimmed = sparql_update.trim();
        let (prefixes, query_body) = self.parse_prefixes(trimmed);
        let query_body = query_body.trim();
        let upper = query_body.to_ascii_uppercase();

        if !upper.starts_with("DELETE WHERE") {
            return Err(anyhow!(
                "Unsupported SPARQL UPDATE for in-memory store: {}",
                sparql_update
            ));
        }

        let delete_body = &query_body["DELETE WHERE".len()..];
        let where_body = self.extract_where_body(delete_body)?;
        let (graph, pattern_source) = self.parse_optional_graph_clause(where_body)?;
        let pattern = self.parse_triple_pattern(pattern_source, &prefixes)?;

        let mut store = self
            .triples
            .write()
            .map_err(|e| anyhow!("Failed to acquire write lock: {}", e))?;

        let graphs_to_search: Vec<String> = if let Some(graph_uri) = graph {
            vec![graph_uri]
        } else {
            store.keys().cloned().collect()
        };

        for graph_uri in graphs_to_search {
            let mut remove_graph = false;

            if let Some(graph_triples) = store.get_mut(&graph_uri) {
                graph_triples.retain(|triple| !self.triple_matches_query(&pattern, triple));
                remove_graph = graph_triples.is_empty();
            }

            if remove_graph {
                store.remove(&graph_uri);
            }
        }

        Ok(())
    }

    fn triple_matches_query(&self, pattern: &SimpleTriplePattern, triple: &Triple) -> bool {
        self.term_matches(&pattern.subject, &triple.subject, TriplePosition::Subject)
            && self.term_matches(
                &pattern.predicate,
                &triple.predicate,
                TriplePosition::Predicate,
            )
            && self.term_matches(&pattern.object, &triple.object, TriplePosition::Object)
    }

    fn term_matches(&self, term: &QueryTerm, candidate: &str, position: TriplePosition) -> bool {
        match term {
            QueryTerm::Variable(_) => true,
            QueryTerm::Constant(expected) => {
                if candidate == expected {
                    return true;
                }

                if position == TriplePosition::Object {
                    let bracketed = format!("<{}>", expected);
                    if candidate == bracketed {
                        return true;
                    }

                    if expected.starts_with('"') && expected.ends_with('"') && expected.len() >= 2 {
                        let unquoted = &expected[1..expected.len() - 1];
                        if candidate == unquoted {
                            return true;
                        }
                    }

                    if expected == "true" || expected == "false" {
                        let quoted = format!("\"{}\"", expected);
                        let typed_boolean = format!("{}^^<xsd:boolean>", quoted);
                        if candidate == quoted || candidate == typed_boolean {
                            return true;
                        }
                    }
                }

                false
            }
        }
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
        let upper = sparql_update.to_ascii_uppercase();

        if upper.contains("DELETE WHERE") {
            return self.execute_delete_where_update(sparql_update);
        }

        if upper.contains("INSERT DATA") {
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

    #[test]
    fn test_query_filters_exact_subject_projection() {
        let store = InMemoryRdfStore::new();
        let graph = NamedGraph::current();

        store
            .insert_triple(
                "http://example.org/workflow/1",
                "http://graphica.io/governance#forWorkflow",
                "http://example.org/workflow/target",
                Some(&graph),
            )
            .unwrap();
        store
            .insert_triple(
                "http://example.org/workflow/2",
                "http://graphica.io/governance#forWorkflow",
                "http://example.org/workflow/other",
                Some(&graph),
            )
            .unwrap();

        let results = store
            .query(
                r#"
SELECT ?predicate ?object
WHERE {
  <http://example.org/workflow/1> ?predicate ?object .
}
LIMIT 50
"#,
            )
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(
            results[0].get("predicate").and_then(|v| v.as_str()),
            Some("http://graphica.io/governance#forWorkflow")
        );
        assert_eq!(
            results[0].get("object").and_then(|v| v.as_str()),
            Some("http://example.org/workflow/target")
        );
    }

    #[test]
    fn test_query_filters_by_graph_and_bound_object() {
        let store = InMemoryRdfStore::new();
        let workflow_graph = NamedGraph::workflow_executions();
        let current_graph = NamedGraph::current();

        store
            .insert_triple(
                "http://graphica.io/workflow-execution/exec-1",
                "http://graphica.io/workflow#executedWorkflow",
                "<http://graphica.io/workflow#/workflow/wf-1>",
                Some(&workflow_graph),
            )
            .unwrap();
        store
            .insert_triple(
                "http://graphica.io/workflow-execution/exec-2",
                "http://graphica.io/workflow#executedWorkflow",
                "http://graphica.io/workflow#/workflow/wf-2",
                Some(&workflow_graph),
            )
            .unwrap();
        store
            .insert_triple(
                "http://graphica.io/workflow-execution/exec-3",
                "http://graphica.io/workflow#executedWorkflow",
                "http://graphica.io/workflow#/workflow/wf-1",
                Some(&current_graph),
            )
            .unwrap();

        let results = store
            .query(
                r#"
SELECT ?subject ?predicate
WHERE {
  GRAPH <http://graphica.io/graph/workflow-executions> {
    ?subject ?predicate <http://graphica.io/workflow#/workflow/wf-1> .
  }
}
LIMIT 200
"#,
            )
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(
            results[0].get("subject").and_then(|v| v.as_str()),
            Some("http://graphica.io/workflow-execution/exec-1")
        );
        assert_eq!(
            results[0].get("predicate").and_then(|v| v.as_str()),
            Some("http://graphica.io/workflow#executedWorkflow")
        );
    }

    #[test]
    fn test_query_limit_is_applied_after_filtering() {
        let store = InMemoryRdfStore::new();
        let graph = NamedGraph::current();

        for idx in 0..3 {
            store
                .insert_triple(
                    &format!("http://example.org/check/{}", idx),
                    "http://graphica.io/governance#forWorkflow",
                    "http://example.org/workflow/target",
                    Some(&graph),
                )
                .unwrap();
        }

        let results = store
            .query(
                r#"
SELECT ?subject
WHERE {
  ?subject <http://graphica.io/governance#forWorkflow> <http://example.org/workflow/target> .
}
LIMIT 2
"#,
            )
            .unwrap();

        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_query_supports_prefix_ask_without_where() {
        let store = InMemoryRdfStore::new();
        let graph = NamedGraph::current();

        store
            .insert_triple(
                "http://graphica.io/user/dev-user",
                "http://graphica.io/auth#canExecute",
                "http://graphica.io/workflow/wf-123",
                Some(&graph),
            )
            .unwrap();

        let results = store
            .query(
                r#"
PREFIX auth: <http://graphica.io/auth#>

ASK {
  <http://graphica.io/user/dev-user> auth:canExecute <http://graphica.io/workflow/wf-123> .
}
"#,
            )
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].get("ASK").and_then(|v| v.as_bool()), Some(true));
    }

    #[test]
    fn test_query_supports_prefix_select_with_where() {
        let store = InMemoryRdfStore::new();
        let graph = NamedGraph::current();

        store
            .insert_triple(
                "http://graphica.io/workflow/wf-123",
                "http://graphica.io/workflow#requiresDataClassification",
                "restricted",
                Some(&graph),
            )
            .unwrap();

        let results = store
            .query(
                r#"
PREFIX wf: <http://graphica.io/workflow#>

SELECT ?classification WHERE {
  <http://graphica.io/workflow/wf-123> wf:requiresDataClassification ?classification .
}
"#,
            )
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(
            results[0].get("classification").and_then(|v| v.as_str()),
            Some("restricted")
        );
    }
}
