//! Field→Ontology Mapping Resolver
//!
//! Resolves ontology mappings for discovered schema fields using pattern inference
//! and dynamic ontology matching from OntologyConstraintRegistry.

use super::constraint_rules::OntologyConstraintRegistry;
use super::types::{FieldOntologyMapping, MappingMethod};
use crate::mapping::discovery::types::DiscoveredColumn;
use anyhow::Result;
use std::sync::Arc;

/// Resolver for field→ontology mappings
///
/// This resolver uses multiple strategies:
/// 1. Pattern inference from sample data (high confidence)
/// 2. Dynamic ontology term matching from registry (medium-high confidence)
/// 3. Field name heuristics (medium confidence)
/// 4. Type-based inference (low confidence)
pub struct MappingResolver {
    min_confidence: f64,
    registry: Arc<OntologyConstraintRegistry>,
}

impl MappingResolver {
    /// Create a new mapping resolver with an ontology constraint registry
    pub fn with_registry(min_confidence: f64, registry: Arc<OntologyConstraintRegistry>) -> Self {
        Self {
            min_confidence,
            registry,
        }
    }

    /// Create a new mapping resolver with default schema.org terms only
    ///
    /// **Deprecated**: Use `with_registry()` to enable custom ontology support
    pub fn new(min_confidence: f64) -> Self {
        Self {
            min_confidence,
            registry: Arc::new(OntologyConstraintRegistry::new()),
        }
    }

    /// Resolve ontology mappings for discovered columns
    ///
    /// Returns mappings that meet the minimum confidence threshold.
    pub fn resolve_mappings(
        &self,
        table_name: &str,
        columns: &[DiscoveredColumn],
    ) -> Result<Vec<FieldOntologyMapping>> {
        let mut mappings = Vec::new();

        for column in columns {
            if let Some(mapping) = self.resolve_single_field(table_name, column)? {
                if mapping.confidence >= self.min_confidence {
                    mappings.push(mapping);
                }
            }
        }

        Ok(mappings)
    }

    /// Resolve a single field mapping
    ///
    /// Tries multiple inference strategies in order of confidence:
    /// 1. Pattern inference (0.85-0.95 confidence)
    /// 2. Dynamic registry ontology matching (0.75-0.90 confidence)
    /// 3. Field name + type inference (0.70-0.85 confidence)
    pub fn resolve_single_field(
        &self,
        table_name: &str,
        column: &DiscoveredColumn,
    ) -> Result<Option<FieldOntologyMapping>> {
        // Try pattern-based inference first (highest confidence)
        if let Some(mapping) = self.infer_from_pattern(table_name, column)? {
            return Ok(Some(mapping));
        }

        // Try dynamic ontology matching from registry (includes custom ontologies)
        if let Some(mapping) = self.infer_from_registry_ontologies(table_name, column)? {
            return Ok(Some(mapping));
        }

        // Try field name + type inference (fallback for schema.org only)
        if let Some(mapping) = self.infer_from_name_and_type(table_name, column)? {
            return Ok(Some(mapping));
        }

        // No mapping found
        Ok(None)
    }

    /// Infer ontology mapping from registry ontologies (includes custom ontologies)
    ///
    /// Queries the OntologyConstraintRegistry for all available ontology terms
    /// and attempts fuzzy matching against the field name.
    fn infer_from_registry_ontologies(
        &self,
        table_name: &str,
        column: &DiscoveredColumn,
    ) -> Result<Option<FieldOntologyMapping>> {
        // Get all available ontology URIs from the registry
        let all_uris = self.registry.get_all_uris();

        let column_name_lower = column.name.to_lowercase();
        let column_name_normalized = column_name_lower
            .replace('_', "")
            .replace('-', "")
            .replace(' ', "");

        let mut best_match: Option<(String, f64)> = None;

        for uri in all_uris {
            // Extract the term name from the URI (part after last # or /)
            let term_name = if let Some(pos) = uri.rfind('#') {
                &uri[pos + 1..]
            } else if let Some(pos) = uri.rfind('/') {
                &uri[pos + 1..]
            } else {
                continue;
            };

            let term_name_lower = term_name.to_lowercase();

            // Calculate similarity score
            let similarity = self.calculate_similarity(&column_name_normalized, &term_name_lower);

            // Update best match if this is better
            if let Some((_, current_score)) = best_match {
                if similarity > current_score {
                    best_match = Some((uri.clone(), similarity));
                }
            } else if similarity > 0.6 {
                // Minimum threshold for consideration
                best_match = Some((uri.clone(), similarity));
            }
        }

        // If we found a good match, return it
        if let Some((ontology_uri, similarity)) = best_match {
            // Map similarity to confidence (0.75 - 0.90 range)
            let confidence = 0.75 + (similarity - 0.6) * 0.375; // Maps 0.6-1.0 → 0.75-0.90

            return Ok(Some(FieldOntologyMapping {
                field_id: format!("{}_{}", table_name, column.name),
                field_name: column.name.clone(),
                table_name: table_name.to_string(),
                ontology_uri,
                confidence,
                mapping_method: MappingMethod::RegistryMatching,
                mapped_at: chrono::Utc::now().timestamp(),
            }));
        }

        Ok(None)
    }

    /// Calculate similarity between two normalized strings
    ///
    /// Uses a simple similarity metric:
    /// - Exact match: 1.0
    /// - Substring match: 0.8-0.9
    /// - Partial overlap: 0.6-0.8
    fn calculate_similarity(&self, field: &str, term: &str) -> f64 {
        // Exact match
        if field == term {
            return 1.0;
        }

        // One contains the other
        if field.contains(term) || term.contains(field) {
            let shorter_len = field.len().min(term.len());
            let longer_len = field.len().max(term.len());
            return 0.8 + (shorter_len as f64 / longer_len as f64) * 0.1;
        }

        // Calculate Levenshtein-like similarity using longest common substring
        let common_len = self.longest_common_substring(field, term);
        let max_len = field.len().max(term.len());

        if common_len == 0 {
            return 0.0;
        }

        // Return similarity based on common substring ratio
        0.6 * (common_len as f64 / max_len as f64)
    }

    /// Find longest common substring length
    fn longest_common_substring(&self, s1: &str, s2: &str) -> usize {
        let chars1: Vec<char> = s1.chars().collect();
        let chars2: Vec<char> = s2.chars().collect();

        let mut max_len = 0;
        let mut dp = vec![vec![0; chars2.len() + 1]; chars1.len() + 1];

        for i in 1..=chars1.len() {
            for j in 1..=chars2.len() {
                if chars1[i - 1] == chars2[j - 1] {
                    dp[i][j] = dp[i - 1][j - 1] + 1;
                    max_len = max_len.max(dp[i][j]);
                }
            }
        }

        max_len
    }

    /// Infer ontology mapping from data patterns
    ///
    /// Analyzes sample values to detect well-known patterns.
    fn infer_from_pattern(
        &self,
        table_name: &str,
        column: &DiscoveredColumn,
    ) -> Result<Option<FieldOntologyMapping>> {
        // Email pattern detection
        if self.is_email_pattern(&column.sample_values) {
            return Ok(Some(FieldOntologyMapping {
                field_id: format!("{}_{}", table_name, column.name),
                field_name: column.name.clone(),
                table_name: table_name.to_string(),
                ontology_uri: "http://schema.org/email".to_string(),
                confidence: 0.95,
                mapping_method: MappingMethod::PatternInference,
                mapped_at: chrono::Utc::now().timestamp(),
            }));
        }

        // Phone pattern detection
        if self.is_phone_pattern(&column.sample_values) {
            return Ok(Some(FieldOntologyMapping {
                field_id: format!("{}_{}", table_name, column.name),
                field_name: column.name.clone(),
                table_name: table_name.to_string(),
                ontology_uri: "http://schema.org/telephone".to_string(),
                confidence: 0.92,
                mapping_method: MappingMethod::PatternInference,
                mapped_at: chrono::Utc::now().timestamp(),
            }));
        }

        // Age pattern detection (numeric 0-150)
        if self.is_age_pattern(&column.name, &column.sample_values, &column.data_type) {
            return Ok(Some(FieldOntologyMapping {
                field_id: format!("{}_{}", table_name, column.name),
                field_name: column.name.clone(),
                table_name: table_name.to_string(),
                ontology_uri: "http://schema.org/age".to_string(),
                confidence: 0.88,
                mapping_method: MappingMethod::PatternInference,
                mapped_at: chrono::Utc::now().timestamp(),
            }));
        }

        // Price/amount pattern detection
        if self.is_price_pattern(&column.name, &column.sample_values, &column.data_type) {
            return Ok(Some(FieldOntologyMapping {
                field_id: format!("{}_{}", table_name, column.name),
                field_name: column.name.clone(),
                table_name: table_name.to_string(),
                ontology_uri: "http://schema.org/price".to_string(),
                confidence: 0.85,
                mapping_method: MappingMethod::PatternInference,
                mapped_at: chrono::Utc::now().timestamp(),
            }));
        }

        Ok(None)
    }

    /// Infer ontology mapping from field name and type
    fn infer_from_name_and_type(
        &self,
        table_name: &str,
        column: &DiscoveredColumn,
    ) -> Result<Option<FieldOntologyMapping>> {
        let name_lower = column.name.to_lowercase();

        // Name-based inference
        if name_lower.contains("name")
            && !name_lower.contains("file")
            && !name_lower.contains("user")
        {
            return Ok(Some(FieldOntologyMapping {
                field_id: format!("{}_{}", table_name, column.name),
                field_name: column.name.clone(),
                table_name: table_name.to_string(),
                ontology_uri: "http://schema.org/name".to_string(),
                confidence: 0.75,
                mapping_method: MappingMethod::PatternInference,
                mapped_at: chrono::Utc::now().timestamp(),
            }));
        }

        // Identifier inference
        if (name_lower.contains("id") || name_lower == "identifier")
            && !column.nullable
            && column.primary_key
        {
            return Ok(Some(FieldOntologyMapping {
                field_id: format!("{}_{}", table_name, column.name),
                field_name: column.name.clone(),
                table_name: table_name.to_string(),
                ontology_uri: "http://schema.org/identifier".to_string(),
                confidence: 0.80,
                mapping_method: MappingMethod::PatternInference,
                mapped_at: chrono::Utc::now().timestamp(),
            }));
        }

        // Address inference
        if name_lower.contains("address")
            || name_lower.contains("street")
            || name_lower.contains("city")
        {
            return Ok(Some(FieldOntologyMapping {
                field_id: format!("{}_{}", table_name, column.name),
                field_name: column.name.clone(),
                table_name: table_name.to_string(),
                ontology_uri: "http://schema.org/PostalAddress".to_string(),
                confidence: 0.70,
                mapping_method: MappingMethod::PatternInference,
                mapped_at: chrono::Utc::now().timestamp(),
            }));
        }

        Ok(None)
    }

    /// Check if sample values match email pattern
    fn is_email_pattern(&self, samples: &[String]) -> bool {
        if samples.is_empty() {
            return false;
        }

        let email_count = samples.iter().filter(|s| self.looks_like_email(s)).count();

        // At least 80% of samples should look like emails
        email_count as f64 / samples.len() as f64 >= 0.8
    }

    /// Check if a string looks like an email
    fn looks_like_email(&self, s: &str) -> bool {
        s.contains('@') && s.contains('.') && s.len() > 5
    }

    /// Check if sample values match phone pattern
    fn is_phone_pattern(&self, samples: &[String]) -> bool {
        if samples.is_empty() {
            return false;
        }

        let phone_count = samples.iter().filter(|s| self.looks_like_phone(s)).count();

        // At least 80% of samples should look like phones
        phone_count as f64 / samples.len() as f64 >= 0.8
    }

    /// Check if a string looks like a phone number
    fn looks_like_phone(&self, s: &str) -> bool {
        let digit_count = s.chars().filter(|c| c.is_ascii_digit()).count();

        // Has at least 10 digits
        if digit_count < 10 {
            return false;
        }

        // Either has phone formatting characters OR is mostly digits (phone-like)
        let has_formatting = s.contains('-')
            || s.contains('(')
            || s.contains(')')
            || s.starts_with('+')
            || s.contains(' ');

        // Or check if it's mostly digits (at least 70% digits for unformatted phones)
        let mostly_digits = digit_count as f64 / s.len() as f64 >= 0.7;

        has_formatting || mostly_digits
    }

    /// Check if field matches age pattern
    fn is_age_pattern(&self, name: &str, samples: &[String], data_type: &str) -> bool {
        let name_lower = name.to_lowercase();

        // Field name should contain "age"
        if !name_lower.contains("age") {
            return false;
        }

        // Type should be numeric
        let type_upper = data_type.to_uppercase();
        if !type_upper.contains("INT")
            && !type_upper.contains("NUMBER")
            && !type_upper.contains("NUMERIC")
        {
            return false;
        }

        // All sample values should parse as numbers in valid age range
        if samples.is_empty() {
            return true; // Trust the field name if no samples
        }

        samples.iter().all(|s| {
            if let Ok(age) = s.parse::<i32>() {
                age >= 0 && age <= 150
            } else {
                false
            }
        })
    }

    /// Check if field matches price/amount pattern
    fn is_price_pattern(&self, name: &str, samples: &[String], data_type: &str) -> bool {
        let name_lower = name.to_lowercase();

        // Field name should contain price/amount/cost
        if !name_lower.contains("price")
            && !name_lower.contains("amount")
            && !name_lower.contains("cost")
            && !name_lower.contains("total")
        {
            return false;
        }

        // Type should be numeric (decimal preferred)
        let type_upper = data_type.to_uppercase();
        if !type_upper.contains("DECIMAL")
            && !type_upper.contains("NUMERIC")
            && !type_upper.contains("FLOAT")
            && !type_upper.contains("DOUBLE")
        {
            return false;
        }

        // All sample values should parse as positive numbers
        if samples.is_empty() {
            return true; // Trust the field name if no samples
        }

        samples.iter().all(|s| {
            if let Ok(price) = s.parse::<f64>() {
                price >= 0.0
            } else {
                false
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapping::discovery::types::ColumnStatistics;

    fn create_test_column(
        name: &str,
        data_type: &str,
        nullable: bool,
        primary_key: bool,
        samples: Vec<String>,
    ) -> DiscoveredColumn {
        DiscoveredColumn {
            name: name.to_string(),
            data_type: data_type.to_string(),
            nullable,
            primary_key,
            semantic_type: None,
            confidence: 0.9,
            patterns: vec![],
            statistics: ColumnStatistics::default(),
            sample_values: samples,
        }
    }

    #[test]
    fn test_email_pattern_inference() {
        let resolver = MappingResolver::new(0.7);

        let column = create_test_column(
            "customer_email",
            "VARCHAR",
            false,
            false,
            vec![
                "john@example.com".to_string(),
                "jane@test.org".to_string(),
                "bob@company.net".to_string(),
            ],
        );

        let mapping = resolver
            .resolve_single_field("customers", &column)
            .unwrap()
            .unwrap();

        assert_eq!(mapping.ontology_uri, "http://schema.org/email");
        assert_eq!(mapping.confidence, 0.95);
        assert_eq!(mapping.mapping_method, MappingMethod::PatternInference);
        assert_eq!(mapping.field_name, "customer_email");
        assert_eq!(mapping.table_name, "customers");
    }

    #[test]
    fn test_phone_pattern_inference() {
        let resolver = MappingResolver::new(0.7);

        let column = create_test_column(
            "phone_number",
            "VARCHAR",
            true,
            false,
            vec![
                "+1-555-555-0123".to_string(),
                "555-555-0124".to_string(),
                "(555) 555-0125".to_string(),
            ],
        );

        let mapping = resolver
            .resolve_single_field("contacts", &column)
            .unwrap()
            .unwrap();

        assert_eq!(mapping.ontology_uri, "http://schema.org/telephone");
        assert_eq!(mapping.confidence, 0.92);
        assert_eq!(mapping.mapping_method, MappingMethod::PatternInference);
    }

    #[test]
    fn test_age_pattern_inference() {
        let resolver = MappingResolver::new(0.7);

        let column = create_test_column(
            "customer_age",
            "INTEGER",
            true,
            false,
            vec!["30".to_string(), "25".to_string(), "45".to_string()],
        );

        let mapping = resolver
            .resolve_single_field("customers", &column)
            .unwrap()
            .unwrap();

        assert_eq!(mapping.ontology_uri, "http://schema.org/age");
        assert_eq!(mapping.confidence, 0.88);
    }

    #[test]
    fn test_age_invalid_range_rejected() {
        let resolver = MappingResolver::new(0.7);

        let column = create_test_column(
            "customer_age",
            "INTEGER",
            true,
            false,
            vec!["300".to_string(), "250".to_string()], // Invalid ages
        );

        let mapping = resolver.resolve_single_field("customers", &column).unwrap();

        // With registry matching, it will still find "age" in the registry
        // (name-based matching doesn't validate data ranges)
        // But it won't match via PatternInference (which validates ranges)
        if let Some(m) = mapping {
            // If found, it should be via RegistryMatching, not PatternInference
            assert_eq!(m.mapping_method, MappingMethod::RegistryMatching);
        }
        // Note: Pattern inference correctly rejects this due to invalid range
    }

    #[test]
    fn test_price_pattern_inference() {
        let resolver = MappingResolver::new(0.7);

        let column = create_test_column(
            "product_price",
            "DECIMAL",
            false,
            false,
            vec!["19.99".to_string(), "29.99".to_string(), "9.99".to_string()],
        );

        let mapping = resolver
            .resolve_single_field("products", &column)
            .unwrap()
            .unwrap();

        assert_eq!(mapping.ontology_uri, "http://schema.org/price");
        assert_eq!(mapping.confidence, 0.85);
    }

    #[test]
    fn test_name_inference() {
        let resolver = MappingResolver::new(0.7);

        let column = create_test_column(
            "customer_name",
            "VARCHAR",
            false,
            false,
            vec!["John Doe".to_string(), "Jane Smith".to_string()],
        );

        let mapping = resolver
            .resolve_single_field("customers", &column)
            .unwrap()
            .unwrap();

        assert_eq!(mapping.ontology_uri, "http://schema.org/name");
        // With registry matching, confidence may be higher than the old default 0.75
        assert!(
            mapping.confidence >= 0.75,
            "confidence should be at least 0.75, got {}",
            mapping.confidence
        );
    }

    #[test]
    fn test_identifier_inference() {
        let resolver = MappingResolver::new(0.7);

        let column = create_test_column(
            "customer_id",
            "INTEGER",
            false, // NOT NULL
            true,  // PRIMARY KEY
            vec!["1".to_string(), "2".to_string(), "3".to_string()],
        );

        let mapping = resolver
            .resolve_single_field("customers", &column)
            .unwrap()
            .unwrap();

        assert_eq!(mapping.ontology_uri, "http://schema.org/identifier");
        assert_eq!(mapping.confidence, 0.80);
    }

    #[test]
    fn test_address_inference() {
        let resolver = MappingResolver::new(0.7);

        let column = create_test_column(
            "street_address",
            "VARCHAR",
            true,
            false,
            vec!["123 Main St".to_string(), "456 Oak Ave".to_string()],
        );

        let mapping = resolver
            .resolve_single_field("customers", &column)
            .unwrap()
            .unwrap();

        assert_eq!(mapping.ontology_uri, "http://schema.org/PostalAddress");
        assert_eq!(mapping.confidence, 0.70);
    }

    #[test]
    fn test_confidence_threshold() {
        let resolver = MappingResolver::new(0.9); // High threshold

        let column = create_test_column(
            "street_address",
            "VARCHAR",
            true,
            false,
            vec!["123 Main St".to_string()],
        );

        // Address has 0.70 confidence, below 0.9 threshold
        let mappings = resolver.resolve_mappings("customers", &[column]).unwrap();

        // Should be filtered out
        assert_eq!(mappings.len(), 0);
    }

    #[test]
    fn test_multiple_columns_mapping() {
        let resolver = MappingResolver::new(0.7);

        let columns = vec![
            create_test_column(
                "customer_email",
                "VARCHAR",
                false,
                false,
                vec!["john@example.com".to_string()],
            ),
            create_test_column(
                "customer_age",
                "INTEGER",
                true,
                false,
                vec!["30".to_string()],
            ),
            create_test_column(
                "customer_name",
                "VARCHAR",
                false,
                false,
                vec!["John Doe".to_string()],
            ),
        ];

        let mappings = resolver.resolve_mappings("customers", &columns).unwrap();

        assert_eq!(mappings.len(), 3);

        // Check that all expected mappings are present
        let uris: Vec<&str> = mappings.iter().map(|m| m.ontology_uri.as_str()).collect();
        assert!(uris.contains(&"http://schema.org/email"));
        assert!(uris.contains(&"http://schema.org/age"));
        assert!(uris.contains(&"http://schema.org/name"));
    }

    #[test]
    fn test_no_mapping_for_unknown_field() {
        let resolver = MappingResolver::new(0.7);

        let column = create_test_column(
            "unknown_field",
            "VARCHAR",
            true,
            false,
            vec!["some data".to_string()],
        );

        let mapping = resolver.resolve_single_field("customers", &column).unwrap();

        assert!(mapping.is_none());
    }

    #[test]
    fn test_email_partial_match_rejected() {
        let resolver = MappingResolver::new(0.7);

        // Only 50% are emails (below 80% threshold)
        let column = create_test_column(
            "field",
            "VARCHAR",
            false,
            false,
            vec!["john@example.com".to_string(), "not an email".to_string()],
        );

        let mapping = resolver.resolve_single_field("table", &column).unwrap();

        // Should not match email pattern (below 80% threshold)
        assert!(mapping.is_none());
    }
}
