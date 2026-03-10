//! SHACL Shape Generator
//!
//! Generates SHACL NodeShapes from discovered schemas with ontology mappings.
//!
//! This module bridges discovered schemas and ontology constraints, producing
//! well-formed SHACL shapes that can be converted to SQL DDL via the existing
//! `convert_shape_to_table()` infrastructure.

use anyhow::{Context, Result};
use std::collections::HashMap;

use super::constraint_rules::OntologyConstraintRegistry;
use super::types::{FieldOntologyMapping, OntologyDdlConfig};
use crate::mapping::ddl::shacl::types::{NodeShape, PropertyShape};
use crate::mapping::discovery::types::{DiscoveredColumn, DiscoveredTable};
use std::sync::Arc;

/// SHACL shape generator
///
/// Combines discovered schema constraints with ontology-derived constraints
/// to produce SHACL NodeShapes suitable for DDL generation.
pub struct ShaclGenerator {
    /// Constraint registry for ontology terms (shared with resolver)
    registry: Arc<OntologyConstraintRegistry>,

    /// Configuration
    config: OntologyDdlConfig,
}

impl ShaclGenerator {
    /// Create a new SHACL generator with default schema.org registry
    pub fn new(config: OntologyDdlConfig) -> Self {
        Self {
            registry: Arc::new(OntologyConstraintRegistry::new()),
            config,
        }
    }

    /// Create a new SHACL generator with a shared registry (including custom ontologies)
    pub fn with_registry(
        config: OntologyDdlConfig,
        registry: Arc<OntologyConstraintRegistry>,
    ) -> Self {
        Self { registry, config }
    }

    /// Generate SHACL NodeShape from discovered table and ontology mappings
    ///
    /// # Arguments
    /// * `discovered` - Discovered table from schema discovery
    /// * `mappings` - Field→ontology mappings from MappingResolver
    ///
    /// # Returns
    /// A SHACL NodeShape with PropertyShapes for each column
    pub fn generate_shape(
        &self,
        discovered: &DiscoveredTable,
        mappings: &[FieldOntologyMapping],
    ) -> Result<NodeShape> {
        // Create mapping lookup by field name
        let mapping_by_field: HashMap<&str, &FieldOntologyMapping> = mappings
            .iter()
            .map(|m| (m.field_name.as_str(), m))
            .collect();

        // Create NodeShape
        let shape_uri = format!("http://graphica.io/shape/{}", discovered.name);
        let target_class = format!("http://graphica.io/class/{}", discovered.name);

        let mut node_shape = NodeShape::new(shape_uri, target_class);
        node_shape.label = Some(format!("{} Shape", discovered.name));
        node_shape.closed = self.config.strict_constraints;

        // Generate PropertyShape for each column
        for column in &discovered.columns {
            let property_shape = if let Some(mapping) = mapping_by_field.get(column.name.as_str()) {
                // Use ontology-derived constraints
                self.generate_property_from_ontology(column, mapping)?
            } else {
                // Fall back to discovery-only constraints
                self.generate_property_from_discovery(column)?
            };

            node_shape.add_property(property_shape);
        }

        Ok(node_shape)
    }

    /// Generate PropertyShape from ontology mapping
    fn generate_property_from_ontology(
        &self,
        column: &DiscoveredColumn,
        mapping: &FieldOntologyMapping,
    ) -> Result<PropertyShape> {
        // Get constraint template from registry
        let template = self
            .registry
            .get_constraint(&mapping.ontology_uri)
            .context(format!(
                "No constraint template for ontology URI: {}",
                mapping.ontology_uri
            ))?;

        // Create PropertyShape with ontology URI as path
        let mut prop = PropertyShape::new(mapping.ontology_uri.clone());
        prop.name = Some(column.name.clone());
        prop.description = Some(format!(
            "Mapped to {} (confidence: {:.2})",
            mapping.ontology_uri, mapping.confidence
        ));

        // Apply ontology-derived constraints
        prop.datatype = Some(template.datatype.clone());
        prop.max_length = template.max_length;
        prop.pattern = template.pattern.clone();
        prop.min_count = template.min_count;
        prop.max_count = template.max_count;
        prop.min_inclusive = template.min_inclusive;
        prop.max_inclusive = template.max_inclusive;
        prop.in_values = template.in_values.clone();
        prop.default_value = template.default_value.clone();

        // Override with discovered constraints in non-strict mode
        if !self.config.strict_constraints {
            self.apply_discovered_constraints(&mut prop, column);
        }

        Ok(prop)
    }

    /// Generate PropertyShape from discovered schema only
    fn generate_property_from_discovery(&self, column: &DiscoveredColumn) -> Result<PropertyShape> {
        // Use column name as path (no ontology URI available)
        let path = format!("http://graphica.io/property/{}", column.name);
        let mut prop = PropertyShape::new(path);
        prop.name = Some(column.name.clone());
        prop.description = Some("No ontology mapping available".to_string());

        // Apply discovered constraints
        self.apply_discovered_constraints(&mut prop, column);

        Ok(prop)
    }

    /// Apply constraints derived from discovered schema
    fn apply_discovered_constraints(&self, prop: &mut PropertyShape, column: &DiscoveredColumn) {
        // Cardinality from nullable + primary_key
        if !column.nullable {
            prop.min_count = Some(1);
        }

        if column.primary_key {
            prop.max_count = Some(1);
        }

        // Datatype from SQL type
        if prop.datatype.is_none() {
            prop.datatype = Some(self.sql_to_xsd_type(&column.data_type));
        }

        // Max length from statistics (string types)
        if prop.max_length.is_none() {
            if let Some(avg_len) = column.statistics.avg_length {
                // Use 2x average length as max, with ceiling at 1000
                let estimated_max = (avg_len * 2.0).ceil() as u32;
                prop.max_length = Some(estimated_max.min(1000));
            }
        }

        // Numeric ranges from statistics
        if let (Some(min_str), Some(max_str)) =
            (&column.statistics.min_value, &column.statistics.max_value)
        {
            if let (Ok(min_val), Ok(max_val)) = (min_str.parse::<f64>(), max_str.parse::<f64>()) {
                if prop.min_inclusive.is_none() {
                    prop.min_inclusive = Some(min_val);
                }
                if prop.max_inclusive.is_none() {
                    prop.max_inclusive = Some(max_val);
                }
            }
        }

        // Enumeration from most common values (low cardinality)
        if column.statistics.distinct_count <= 20 {
            if let Some(mcv) = &column.statistics.most_common_values {
                if prop.in_values.is_none() {
                    prop.in_values = Some(mcv.clone());
                }
            }
        }
    }

    /// Convert SQL type to XSD datatype
    fn sql_to_xsd_type(&self, sql_type: &str) -> String {
        let sql_lower = sql_type.to_lowercase();

        if sql_lower.contains("varchar") || sql_lower.contains("text") || sql_lower.contains("char")
        {
            "http://www.w3.org/2001/XMLSchema#string".to_string()
        } else if sql_lower.contains("int")
            || sql_lower.contains("bigint")
            || sql_lower.contains("smallint")
        {
            "http://www.w3.org/2001/XMLSchema#integer".to_string()
        } else if sql_lower.contains("decimal") || sql_lower.contains("numeric") {
            "http://www.w3.org/2001/XMLSchema#decimal".to_string()
        } else if sql_lower.contains("real")
            || sql_lower.contains("double")
            || sql_lower.contains("float")
        {
            "http://www.w3.org/2001/XMLSchema#double".to_string()
        } else if sql_lower.contains("bool") {
            "http://www.w3.org/2001/XMLSchema#boolean".to_string()
        } else if sql_lower.contains("date") && !sql_lower.contains("time") {
            "http://www.w3.org/2001/XMLSchema#date".to_string()
        } else if sql_lower.contains("timestamp") || sql_lower.contains("datetime") {
            "http://www.w3.org/2001/XMLSchema#dateTime".to_string()
        } else if sql_lower.contains("time") {
            "http://www.w3.org/2001/XMLSchema#time".to_string()
        } else {
            // Default to string for unknown types
            "http://www.w3.org/2001/XMLSchema#string".to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapping::discovery::types::ColumnStatistics;
    use crate::mapping::ontology_ddl::types::MappingMethod;

    fn create_test_column(name: &str, data_type: &str, nullable: bool) -> DiscoveredColumn {
        DiscoveredColumn {
            name: name.to_string(),
            data_type: data_type.to_string(),
            nullable,
            primary_key: false,
            semantic_type: None,
            confidence: 0.0,
            patterns: vec![],
            statistics: ColumnStatistics::default(),
            sample_values: vec![],
        }
    }

    fn create_test_mapping(
        field_name: &str,
        table_name: &str,
        ontology_uri: &str,
        confidence: f64,
    ) -> FieldOntologyMapping {
        FieldOntologyMapping {
            field_id: format!("{}_{}", table_name, field_name),
            field_name: field_name.to_string(),
            table_name: table_name.to_string(),
            ontology_uri: ontology_uri.to_string(),
            confidence,
            mapping_method: MappingMethod::PatternInference,
            mapped_at: 1234567890,
        }
    }

    #[test]
    fn test_generate_shape_basic() {
        let config = OntologyDdlConfig::default();
        let generator = ShaclGenerator::new(config);

        let table = DiscoveredTable {
            name: "customers".to_string(),
            columns: vec![
                create_test_column("id", "INTEGER", false),
                create_test_column("name", "VARCHAR(255)", false),
            ],
            row_count: Some(1000),
        };

        let mappings = vec![];

        let shape = generator.generate_shape(&table, &mappings).unwrap();

        assert_eq!(shape.target_class, "http://graphica.io/class/customers");
        assert_eq!(shape.properties.len(), 2);
        assert_eq!(shape.label, Some("customers Shape".to_string()));
    }

    #[test]
    fn test_property_from_ontology_email() {
        let config = OntologyDdlConfig::default();
        let generator = ShaclGenerator::new(config);

        let table = DiscoveredTable {
            name: "customers".to_string(),
            columns: vec![create_test_column("email", "VARCHAR(255)", false)],
            row_count: Some(1000),
        };

        let mappings = vec![create_test_mapping(
            "email",
            "customers",
            "http://schema.org/email",
            0.95,
        )];

        let shape = generator.generate_shape(&table, &mappings).unwrap();

        assert_eq!(shape.properties.len(), 1);
        let email_prop = &shape.properties[0];

        // Check ontology-derived constraints
        assert_eq!(email_prop.path, "http://schema.org/email");
        assert_eq!(email_prop.name, Some("email".to_string()));
        assert_eq!(
            email_prop.datatype,
            Some("http://www.w3.org/2001/XMLSchema#string".to_string())
        );
        assert_eq!(email_prop.max_length, Some(255));
        assert!(email_prop.pattern.is_some());
        assert_eq!(email_prop.min_count, Some(1)); // NOT NULL from ontology
    }

    #[test]
    fn test_property_from_ontology_age() {
        let config = OntologyDdlConfig::default();
        let generator = ShaclGenerator::new(config);

        let table = DiscoveredTable {
            name: "customers".to_string(),
            columns: vec![create_test_column("customer_age", "INTEGER", true)],
            row_count: Some(1000),
        };

        let mappings = vec![create_test_mapping(
            "customer_age",
            "customers",
            "http://schema.org/age",
            0.88,
        )];

        let shape = generator.generate_shape(&table, &mappings).unwrap();

        let age_prop = &shape.properties[0];

        // Check ontology-derived range constraints
        assert_eq!(age_prop.path, "http://schema.org/age");
        assert_eq!(
            age_prop.datatype,
            Some("http://www.w3.org/2001/XMLSchema#integer".to_string())
        );
        assert_eq!(age_prop.min_inclusive, Some(0.0));
        assert_eq!(age_prop.max_inclusive, Some(150.0));
    }

    #[test]
    fn test_property_from_ontology_price() {
        let config = OntologyDdlConfig::default();
        let generator = ShaclGenerator::new(config);

        let table = DiscoveredTable {
            name: "products".to_string(),
            columns: vec![create_test_column("price", "DECIMAL(10,2)", false)],
            row_count: Some(500),
        };

        let mappings = vec![create_test_mapping(
            "price",
            "products",
            "http://schema.org/price",
            0.85,
        )];

        let shape = generator.generate_shape(&table, &mappings).unwrap();

        let price_prop = &shape.properties[0];

        // Check positive price constraint
        assert_eq!(price_prop.path, "http://schema.org/price");
        assert_eq!(
            price_prop.datatype,
            Some("http://www.w3.org/2001/XMLSchema#decimal".to_string())
        );
        assert_eq!(price_prop.min_inclusive, Some(0.0)); // Price >= 0
        assert_eq!(price_prop.min_count, Some(1)); // NOT NULL
    }

    #[test]
    fn test_property_from_discovery_only() {
        let config = OntologyDdlConfig::default();
        let generator = ShaclGenerator::new(config);

        let mut column = create_test_column("description", "VARCHAR(500)", true);
        column.statistics.avg_length = Some(150.0);

        let table = DiscoveredTable {
            name: "products".to_string(),
            columns: vec![column],
            row_count: Some(500),
        };

        let mappings = vec![]; // No ontology mapping

        let shape = generator.generate_shape(&table, &mappings).unwrap();

        let desc_prop = &shape.properties[0];

        // Check discovery-only constraints
        assert_eq!(desc_prop.path, "http://graphica.io/property/description");
        assert_eq!(desc_prop.name, Some("description".to_string()));
        assert_eq!(
            desc_prop.datatype,
            Some("http://www.w3.org/2001/XMLSchema#string".to_string())
        );
        assert_eq!(desc_prop.max_length, Some(300)); // 2x avg_length
        assert_eq!(desc_prop.min_count, None); // Nullable
        assert!(desc_prop
            .description
            .as_ref()
            .unwrap()
            .contains("No ontology mapping"));
    }

    #[test]
    fn test_primary_key_constraints() {
        let config = OntologyDdlConfig::default();
        let generator = ShaclGenerator::new(config);

        let mut id_column = create_test_column("id", "INTEGER", false);
        id_column.primary_key = true;

        let table = DiscoveredTable {
            name: "customers".to_string(),
            columns: vec![id_column],
            row_count: Some(1000),
        };

        let mappings = vec![];

        let shape = generator.generate_shape(&table, &mappings).unwrap();

        let id_prop = &shape.properties[0];

        // Primary key should have NOT NULL + UNIQUE
        assert_eq!(id_prop.min_count, Some(1)); // NOT NULL
        assert_eq!(id_prop.max_count, Some(1)); // UNIQUE
    }

    #[test]
    fn test_numeric_range_from_statistics() {
        let config = OntologyDdlConfig::default();
        let generator = ShaclGenerator::new(config);

        let mut column = create_test_column("quantity", "INTEGER", false);
        column.statistics.min_value = Some("1".to_string());
        column.statistics.max_value = Some("1000".to_string());

        let table = DiscoveredTable {
            name: "orders".to_string(),
            columns: vec![column],
            row_count: Some(5000),
        };

        let mappings = vec![];

        let shape = generator.generate_shape(&table, &mappings).unwrap();

        let qty_prop = &shape.properties[0];

        // Check numeric ranges from statistics
        assert_eq!(qty_prop.min_inclusive, Some(1.0));
        assert_eq!(qty_prop.max_inclusive, Some(1000.0));
    }

    #[test]
    fn test_enumeration_from_low_cardinality() {
        let config = OntologyDdlConfig::default();
        let generator = ShaclGenerator::new(config);

        let mut column = create_test_column("status", "VARCHAR(20)", false);
        column.statistics.distinct_count = 5;
        column.statistics.most_common_values = Some(
            vec!["pending", "approved", "rejected"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
        );

        let table = DiscoveredTable {
            name: "orders".to_string(),
            columns: vec![column],
            row_count: Some(5000),
        };

        let mappings = vec![];

        let shape = generator.generate_shape(&table, &mappings).unwrap();

        let status_prop = &shape.properties[0];

        // Check enumeration constraint
        assert!(status_prop.in_values.is_some());
        let values = status_prop.in_values.as_ref().unwrap();
        assert_eq!(values.len(), 3);
        assert!(values.contains(&"pending".to_string()));
    }

    #[test]
    fn test_mixed_ontology_and_discovery() {
        let config = OntologyDdlConfig::default();
        let generator = ShaclGenerator::new(config);

        let table = DiscoveredTable {
            name: "customers".to_string(),
            columns: vec![
                create_test_column("email", "VARCHAR(255)", false),
                create_test_column("notes", "TEXT", true),
            ],
            row_count: Some(1000),
        };

        // Only email has ontology mapping
        let mappings = vec![create_test_mapping(
            "email",
            "customers",
            "http://schema.org/email",
            0.95,
        )];

        let shape = generator.generate_shape(&table, &mappings).unwrap();

        assert_eq!(shape.properties.len(), 2);

        // Email should use ontology
        let email_prop = &shape.properties[0];
        assert_eq!(email_prop.path, "http://schema.org/email");
        assert!(email_prop.pattern.is_some());

        // Notes should use discovery
        let notes_prop = &shape.properties[1];
        assert_eq!(notes_prop.path, "http://graphica.io/property/notes");
        assert!(notes_prop.pattern.is_none());
    }

    #[test]
    fn test_strict_mode_no_override() {
        let mut config = OntologyDdlConfig::default();
        config.strict_constraints = true;

        let generator = ShaclGenerator::new(config);

        let mut column = create_test_column("email", "VARCHAR(500)", true); // Nullable + longer
        column.statistics.avg_length = Some(250.0);

        let table = DiscoveredTable {
            name: "customers".to_string(),
            columns: vec![column],
            row_count: Some(1000),
        };

        let mappings = vec![create_test_mapping(
            "email",
            "customers",
            "http://schema.org/email",
            0.95,
        )];

        let shape = generator.generate_shape(&table, &mappings).unwrap();

        let email_prop = &shape.properties[0];

        // In strict mode, ontology constraints should NOT be overridden
        assert_eq!(email_prop.max_length, Some(255)); // From ontology, not 500 from discovery
        assert_eq!(email_prop.min_count, Some(1)); // NOT NULL from ontology, not nullable from discovery
        assert!(shape.closed); // Strict mode sets closed=true
    }

    #[test]
    fn test_sql_to_xsd_type_mapping() {
        let config = OntologyDdlConfig::default();
        let generator = ShaclGenerator::new(config);

        assert_eq!(
            generator.sql_to_xsd_type("VARCHAR(255)"),
            "http://www.w3.org/2001/XMLSchema#string"
        );
        assert_eq!(
            generator.sql_to_xsd_type("INTEGER"),
            "http://www.w3.org/2001/XMLSchema#integer"
        );
        assert_eq!(
            generator.sql_to_xsd_type("DECIMAL(10,2)"),
            "http://www.w3.org/2001/XMLSchema#decimal"
        );
        assert_eq!(
            generator.sql_to_xsd_type("BOOLEAN"),
            "http://www.w3.org/2001/XMLSchema#boolean"
        );
        assert_eq!(
            generator.sql_to_xsd_type("DATE"),
            "http://www.w3.org/2001/XMLSchema#date"
        );
        assert_eq!(
            generator.sql_to_xsd_type("TIMESTAMP"),
            "http://www.w3.org/2001/XMLSchema#dateTime"
        );
    }

    #[test]
    fn test_confidence_description() {
        let config = OntologyDdlConfig::default();
        let generator = ShaclGenerator::new(config);

        let table = DiscoveredTable {
            name: "customers".to_string(),
            columns: vec![create_test_column("email", "VARCHAR(255)", false)],
            row_count: Some(1000),
        };

        let mappings = vec![create_test_mapping(
            "email",
            "customers",
            "http://schema.org/email",
            0.95,
        )];

        let shape = generator.generate_shape(&table, &mappings).unwrap();

        let email_prop = &shape.properties[0];

        // Description should include confidence
        assert!(email_prop
            .description
            .as_ref()
            .unwrap()
            .contains("confidence: 0.95"));
        assert!(email_prop
            .description
            .as_ref()
            .unwrap()
            .contains("http://schema.org/email"));
    }
}
