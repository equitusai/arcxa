//! Ontology-Driven DDL Orchestrator
//!
//! Integrates all phases of ontology-driven DDL generation:
//! 1. Discovery → DiscoveredTable
//! 2. Ontology Mapping → FieldOntologyMapping[]
//! 3. SHACL Generation → NodeShape
//! 4. RDF Lineage → PROV triples
//! 5. DDL Generation → SQL statements
//!
//! This is the main entry point for Phase 2.5 (GAP-002).

use anyhow::{Context, Result};
use std::sync::Arc;
use uuid::Uuid;

use crate::mapping::ddl::shacl::types::NodeShape;
use crate::mapping::ddl::{convert_shape_to_table, get_dialect, TableDefinition};
use crate::mapping::discovery::types::DiscoveredTable;
use crate::mapping::field_mapping::OntologyDdlAdapter;
use crate::mapping::ontology_registry::RegistryClient;

use super::constraint_rules::OntologyConstraintRegistry;
use super::mapping_resolver::MappingResolver;
use super::rdf_lineage::{LineageSummary, RdfLineageGenerator};
use super::shacl_generator::ShaclGenerator;
use super::transformation_rules::{FieldTransformation, OntologyTransformationRegistry};
use super::types::{FieldOntologyMapping, OntologyDdlConfig, OntologyDdlResult};

/// Orchestrator for ontology-driven DDL generation
///
/// This is the main integration point that coordinates all phases of the
/// ontology-driven DDL generation pipeline.
pub struct OntologyDdlOrchestrator {
    /// Configuration
    config: OntologyDdlConfig,

    /// Mapping resolver (legacy)
    resolver: MappingResolver,

    /// Unified ontology mapper (preferred)
    unified_adapter: Option<Arc<OntologyDdlAdapter>>,

    /// SHACL generator
    shacl_generator: ShaclGenerator,

    /// Ontology constraint registry (shared with resolver)
    registry: Arc<OntologyConstraintRegistry>,

    /// Transformation rules registry (GAP-003)
    transformation_registry: OntologyTransformationRegistry,
}

impl OntologyDdlOrchestrator {
    /// Create a new orchestrator with the given configuration (default schema.org only)
    pub fn new(config: OntologyDdlConfig) -> Self {
        let registry = Arc::new(OntologyConstraintRegistry::new());
        let resolver =
            MappingResolver::with_registry(config.min_mapping_confidence, registry.clone());
        let shacl_generator = ShaclGenerator::with_registry(config.clone(), registry.clone());
        let transformation_registry = OntologyTransformationRegistry::new();

        Self {
            config,
            resolver,
            unified_adapter: None,
            shacl_generator,
            registry,
            transformation_registry,
        }
    }

    /// Create a new orchestrator with custom ontologies from RegistryClient
    ///
    /// This constructor loads ontology terms from the PersistedOntologyRegistry
    /// and uses them for field mapping and constraint generation. Custom ontology
    /// terms take precedence over default schema.org terms.
    ///
    /// # Arguments
    ///
    /// * `config` - Configuration for DDL generation
    /// * `registry_client` - Client for querying custom ontologies
    ///
    /// # Returns
    ///
    /// Orchestrator configured with both custom and default ontologies
    ///
    /// # Example
    ///
    /// ```ignore
    /// let registry_client = RegistryClient::new(Some(persisted_registry.registry()));
    /// let orchestrator = OntologyDdlOrchestrator::with_custom_ontologies(
    ///     OntologyDdlConfig::default(),
    ///     &registry_client
    /// )?;
    ///
    /// let result = orchestrator.generate_ddl(&discovered_table, "postgresql")?;
    /// ```
    pub fn with_custom_ontologies(
        config: OntologyDdlConfig,
        registry_client: &RegistryClient,
    ) -> Result<Self> {
        // Create registry with custom ontologies
        let registry = Arc::new(
            OntologyConstraintRegistry::with_custom_ontologies(registry_client)
                .context("Failed to load custom ontologies into constraint registry")?,
        );

        // Create resolver with registry (enables custom ontology matching)
        let resolver =
            MappingResolver::with_registry(config.min_mapping_confidence, registry.clone());
        let shacl_generator = ShaclGenerator::with_registry(config.clone(), registry.clone());
        let transformation_registry = OntologyTransformationRegistry::new();

        Ok(Self {
            config,
            resolver,
            unified_adapter: None,
            shacl_generator,
            registry,
            transformation_registry,
        })
    }

    /// Create a new orchestrator with unified ontology mapper
    ///
    /// This constructor uses the new unified mapping engine that consolidates
    /// all mapping strategies (pattern, semantic, statistical, lexical, registry, heuristic).
    /// This provides better mapping quality and full integration with graphica-model.
    ///
    /// # Arguments
    ///
    /// * `config` - Configuration for DDL generation
    /// * `unified_adapter` - Unified ontology mapping adapter
    ///
    /// # Returns
    ///
    /// Orchestrator configured with unified mapping engine
    ///
    /// # Example
    ///
    /// ```ignore
    /// use crate::mapping::field_mapping::{UnifiedMappingConfig, create_ontology_ddl_adapter};
    ///
    /// let unified_config = UnifiedMappingConfig::default();
    /// let adapter = create_ontology_ddl_adapter(unified_config, None, None).await?;
    ///
    /// let orchestrator = OntologyDdlOrchestrator::with_unified_mapper(
    ///     OntologyDdlConfig::default(),
    ///     Arc::new(adapter)
    /// );
    ///
    /// let result = orchestrator.generate_ddl(&discovered_table, "postgresql").await?;
    /// ```
    pub fn with_unified_mapper(
        config: OntologyDdlConfig,
        unified_adapter: Arc<OntologyDdlAdapter>,
    ) -> Self {
        // Create minimal components (unified mapper handles ontology matching)
        let registry = Arc::new(OntologyConstraintRegistry::new());
        let resolver =
            MappingResolver::with_registry(config.min_mapping_confidence, registry.clone());
        let shacl_generator = ShaclGenerator::with_registry(config.clone(), registry.clone());
        let transformation_registry = OntologyTransformationRegistry::new();

        Self {
            config,
            resolver,
            unified_adapter: Some(unified_adapter),
            shacl_generator,
            registry,
            transformation_registry,
        }
    }

    /// Generate ontology-driven DDL from discovered schema
    ///
    /// This is the main entry point that executes the complete pipeline:
    /// Discovery → Ontology Mapping → SHACL Generation → DDL Generation → Transformation Generation → Lineage
    ///
    /// # Arguments
    /// * `discovered` - Discovered table from schema discovery phase
    /// * `target_dialect` - SQL dialect for DDL generation (e.g., "postgresql")
    ///
    /// # Returns
    /// Complete DDL result with statements, mappings, shape, transformations, and lineage
    pub async fn generate_ddl(
        &self,
        discovered: &DiscoveredTable,
        target_dialect: &str,
    ) -> Result<OntologyDdlResult> {
        // Phase 1: Ontology Mapping
        let mappings = if self.config.skip_ontology_mapping {
            Vec::new()
        } else if let Some(adapter) = &self.unified_adapter {
            // Use unified mapper (preferred)
            adapter
                .resolve_mappings(
                    &discovered.name,
                    &discovered.columns,
                    self.config.min_mapping_confidence,
                )
                .await
                .context("Failed to resolve ontology mappings via unified mapper")?
        } else {
            // Fallback to legacy resolver
            self.resolver
                .resolve_mappings(&discovered.name, &discovered.columns)
                .context("Failed to resolve ontology mappings via legacy resolver")?
        };

        // Phase 2: SHACL Shape Generation
        let shacl_shape = self
            .shacl_generator
            .generate_shape(discovered, &mappings)
            .context("Failed to generate SHACL shape")?;

        // Phase 3: DDL Generation
        let table_definition = self
            .generate_table_definition(&shacl_shape, target_dialect)
            .context("Failed to generate table definition")?;

        let ddl_statements = self
            .generate_ddl_statements(&table_definition, target_dialect)
            .context("Failed to generate DDL statements")?;

        // Phase 3.5: Transformation Generation from ontology mappings (GAP-003)
        let transformations = self.generate_transformations(&mappings);

        // Phase 4: RDF Lineage (if enabled)
        let rdf_triples = if self.config.record_lineage {
            Some(self.generate_lineage(discovered, &mappings, &shacl_shape, &ddl_statements)?)
        } else {
            None
        };

        Ok(OntologyDdlResult {
            ddl_statements,
            table_definition,
            ontology_mappings: mappings,
            shacl_shape,
            rdf_triples,
            transformations,
        })
    }

    /// Generate table definition from SHACL shape
    fn generate_table_definition(
        &self,
        shacl_shape: &NodeShape,
        target_dialect: &str,
    ) -> Result<TableDefinition> {
        // Get dialect instance
        let dialect = get_dialect(target_dialect)
            .with_context(|| format!("Failed to get dialect: {}", target_dialect))?;

        Ok(convert_shape_to_table(shacl_shape, &*dialect))
    }

    /// Generate DDL statements from table definition
    fn generate_ddl_statements(
        &self,
        table_def: &TableDefinition,
        target_dialect: &str,
    ) -> Result<Vec<String>> {
        let mut statements = Vec::new();

        // Get dialect instance
        let dialect = get_dialect(target_dialect)
            .with_context(|| format!("Failed to get dialect: {}", target_dialect))?;

        // CREATE TABLE statement
        statements.push(dialect.create_table(table_def));

        // CREATE INDEX statements
        for index in &table_def.indexes {
            statements.push(dialect.create_index(index));
        }

        Ok(statements)
    }

    /// Generate RDF lineage triples
    fn generate_lineage(
        &self,
        discovered: &DiscoveredTable,
        mappings: &[FieldOntologyMapping],
        shacl_shape: &NodeShape,
        ddl_statements: &[String],
    ) -> Result<Vec<(String, String, String)>> {
        let run_id = Uuid::new_v4().to_string();
        let agent_id = "graphica-coordinator:ontology-ddl".to_string();
        let base_uri = "http://graphica.io/".to_string();

        let generator = RdfLineageGenerator::new(base_uri, agent_id, run_id);

        generator.generate_lineage(discovered, mappings, shacl_shape, ddl_statements)
    }

    /// Get lineage summary for a result
    pub fn get_lineage_summary(&self, result: &OntologyDdlResult) -> Option<LineageSummary> {
        if let Some(triples) = &result.rdf_triples {
            let run_id = Uuid::new_v4().to_string(); // Placeholder
            let generator = RdfLineageGenerator::new(
                "http://graphica.io/".to_string(),
                "graphica-coordinator:ontology-ddl".to_string(),
                run_id,
            );
            Some(generator.generate_lineage_summary(triples))
        } else {
            None
        }
    }

    /// Get the ontology constraint registry
    pub fn registry(&self) -> &Arc<OntologyConstraintRegistry> {
        &self.registry
    }

    /// Get the configuration
    pub fn config(&self) -> &OntologyDdlConfig {
        &self.config
    }

    /// Generate transformations from ontology mappings (GAP-003)
    ///
    /// Maps ontology URIs to standard data transformations. For example:
    /// - `schema:email` → `LOWER(TRIM(email))`
    /// - `schema:givenName` → `PROPER_CASE(TRIM(first_name))`
    ///
    /// This enables automatic data cleansing based on semantic type.
    fn generate_transformations(
        &self,
        mappings: &[FieldOntologyMapping],
    ) -> Vec<FieldTransformation> {
        mappings
            .iter()
            .filter_map(|mapping| {
                self.transformation_registry
                    .get_transformation(&mapping.ontology_uri, &mapping.field_name)
            })
            .collect()
    }

    /// Get transformation registry
    pub fn transformation_registry(&self) -> &OntologyTransformationRegistry {
        &self.transformation_registry
    }
}

impl Default for OntologyDdlOrchestrator {
    fn default() -> Self {
        Self::new(OntologyDdlConfig::default())
    }
}

/// Convenience function to generate ontology-driven DDL with default configuration
///
/// # Example
/// ```ignore
/// use graphica_coordinator::mapping::ontology_ddl::generate_ontology_ddl;
/// use graphica_coordinator::mapping::discovery::types::DiscoveredTable;
///
/// let discovered: DiscoveredTable = /* ... */;
/// let result = generate_ontology_ddl(&discovered, "postgresql").await?;
///
/// for statement in result.ddl_statements {
///     println!("{}", statement);
/// }
/// ```
pub async fn generate_ontology_ddl(
    discovered: &DiscoveredTable,
    target_dialect: &str,
) -> Result<OntologyDdlResult> {
    let orchestrator = OntologyDdlOrchestrator::default();
    orchestrator.generate_ddl(discovered, target_dialect).await
}

/// Generate ontology-driven DDL with custom configuration
pub async fn generate_ontology_ddl_with_config(
    discovered: &DiscoveredTable,
    target_dialect: &str,
    config: OntologyDdlConfig,
) -> Result<OntologyDdlResult> {
    let orchestrator = OntologyDdlOrchestrator::new(config);
    orchestrator.generate_ddl(discovered, target_dialect).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapping::discovery::types::{ColumnStatistics, DiscoveredColumn};

    fn create_customers_table() -> DiscoveredTable {
        DiscoveredTable {
            name: "customers".to_string(),
            columns: vec![
                DiscoveredColumn {
                    name: "customer_id".to_string(),
                    data_type: "INTEGER".to_string(),
                    nullable: false,
                    primary_key: true,
                    semantic_type: None,
                    confidence: 0.8,
                    patterns: vec![],
                    statistics: ColumnStatistics::default(),
                    sample_values: vec![],
                },
                DiscoveredColumn {
                    name: "email".to_string(),
                    data_type: "VARCHAR(255)".to_string(),
                    nullable: false,
                    primary_key: false,
                    semantic_type: None,
                    confidence: 0.95,
                    patterns: vec![],
                    statistics: ColumnStatistics::default(),
                    sample_values: vec![
                        "john@example.com".to_string(),
                        "jane@example.com".to_string(),
                        "bob@example.com".to_string(),
                    ],
                },
                DiscoveredColumn {
                    name: "customer_age".to_string(),
                    data_type: "INTEGER".to_string(),
                    nullable: true,
                    primary_key: false,
                    semantic_type: None,
                    confidence: 0.88,
                    patterns: vec![],
                    statistics: ColumnStatistics {
                        min_value: Some("18".to_string()),
                        max_value: Some("75".to_string()),
                        ..Default::default()
                    },
                    sample_values: vec!["25".to_string(), "35".to_string(), "45".to_string()],
                },
            ],
            row_count: Some(1000),
        }
    }

    #[tokio::test]
    async fn test_orchestrator_basic() {
        let orchestrator = OntologyDdlOrchestrator::default();
        let table = create_customers_table();

        let result = orchestrator
            .generate_ddl(&table, "postgresql")
            .await
            .unwrap();

        // Should have DDL statements
        assert!(!result.ddl_statements.is_empty());
        assert!(result.ddl_statements[0].contains("CREATE TABLE"));

        // Should have table definition
        assert_eq!(result.table_definition.name, "CUSTOMERS");
        assert_eq!(result.table_definition.columns.len(), 3);

        // Should have ontology mappings (email + age)
        assert!(result.ontology_mappings.len() >= 2);

        // Should have SHACL shape
        assert_eq!(result.shacl_shape.properties.len(), 3);

        // Should have RDF lineage (default config has record_lineage=true)
        assert!(result.rdf_triples.is_some());
    }

    #[tokio::test]
    async fn test_orchestrator_with_ontology_mapping() {
        let orchestrator = OntologyDdlOrchestrator::default();
        let table = create_customers_table();

        let result = orchestrator
            .generate_ddl(&table, "postgresql")
            .await
            .unwrap();

        // Check for email mapping
        let email_mapping = result
            .ontology_mappings
            .iter()
            .find(|m| m.field_name == "email");

        assert!(email_mapping.is_some());
        let mapping = email_mapping.unwrap();
        assert_eq!(mapping.ontology_uri, "http://schema.org/email");
        assert!(mapping.confidence >= 0.85);

        // Check for age mapping
        let age_mapping = result
            .ontology_mappings
            .iter()
            .find(|m| m.field_name == "customer_age");

        assert!(age_mapping.is_some());
        let mapping = age_mapping.unwrap();
        assert_eq!(mapping.ontology_uri, "http://schema.org/age");
    }

    #[tokio::test]
    async fn test_orchestrator_skip_ontology_mapping() {
        let mut config = OntologyDdlConfig::default();
        config.skip_ontology_mapping = true;

        let orchestrator = OntologyDdlOrchestrator::new(config);
        let table = create_customers_table();

        let result = orchestrator
            .generate_ddl(&table, "postgresql")
            .await
            .unwrap();

        // Should have no ontology mappings
        assert_eq!(result.ontology_mappings.len(), 0);

        // But should still have DDL
        assert!(!result.ddl_statements.is_empty());
    }

    #[tokio::test]
    async fn test_orchestrator_skip_lineage() {
        let mut config = OntologyDdlConfig::default();
        config.record_lineage = false;

        let orchestrator = OntologyDdlOrchestrator::new(config);
        let table = create_customers_table();

        let result = orchestrator
            .generate_ddl(&table, "postgresql")
            .await
            .unwrap();

        // Should have no RDF triples
        assert!(result.rdf_triples.is_none());
    }

    #[tokio::test]
    async fn test_orchestrator_strict_constraints() {
        let mut config = OntologyDdlConfig::default();
        config.strict_constraints = true;

        let orchestrator = OntologyDdlOrchestrator::new(config);
        let table = create_customers_table();

        let result = orchestrator
            .generate_ddl(&table, "postgresql")
            .await
            .unwrap();

        // Strict mode should affect SHACL shape
        assert!(result.shacl_shape.closed);
    }

    #[tokio::test]
    async fn test_orchestrator_confidence_threshold() {
        let mut config = OntologyDdlConfig::default();
        config.min_mapping_confidence = 0.95; // Very high threshold

        let orchestrator = OntologyDdlOrchestrator::new(config);
        let table = create_customers_table();

        let result = orchestrator
            .generate_ddl(&table, "postgresql")
            .await
            .unwrap();

        // Only email should pass (0.95), age won't (0.88)
        assert_eq!(result.ontology_mappings.len(), 1);
        assert_eq!(result.ontology_mappings[0].field_name, "email");
    }

    #[tokio::test]
    async fn test_convenience_function() {
        let table = create_customers_table();
        let result = generate_ontology_ddl(&table, "postgresql").await.unwrap();

        assert!(!result.ddl_statements.is_empty());
        assert!(result.ontology_mappings.len() >= 2);
    }

    #[tokio::test]
    async fn test_convenience_function_with_config() {
        let table = create_customers_table();
        let mut config = OntologyDdlConfig::default();
        config.skip_ontology_mapping = true;

        let result = generate_ontology_ddl_with_config(&table, "postgresql", config)
            .await
            .unwrap();

        assert!(!result.ddl_statements.is_empty());
        assert_eq!(result.ontology_mappings.len(), 0);
    }

    #[tokio::test]
    async fn test_ddl_statement_format() {
        let orchestrator = OntologyDdlOrchestrator::default();
        let table = create_customers_table();

        let result = orchestrator
            .generate_ddl(&table, "postgresql")
            .await
            .unwrap();

        // Check CREATE TABLE statement
        let create_table = &result.ddl_statements[0];
        assert!(create_table.contains("CREATE TABLE"));
        assert!(create_table.contains("CUSTOMERS"));
        assert!(create_table.contains("customer_id"));
        assert!(create_table.contains("email"));
        assert!(create_table.contains("customer_age"));
    }

    #[tokio::test]
    async fn test_shacl_shape_integration() {
        let orchestrator = OntologyDdlOrchestrator::default();
        let table = create_customers_table();

        let result = orchestrator
            .generate_ddl(&table, "postgresql")
            .await
            .unwrap();

        // Check SHACL shape has correct properties
        let email_prop = result
            .shacl_shape
            .properties
            .iter()
            .find(|p| p.name.as_ref() == Some(&"email".to_string()));

        assert!(email_prop.is_some());
        let prop = email_prop.unwrap();

        // Should have ontology-derived constraints
        assert!(prop.pattern.is_some()); // Email regex
        assert_eq!(prop.max_length, Some(255));
        assert_eq!(prop.min_count, Some(1)); // NOT NULL
    }

    #[tokio::test]
    async fn test_lineage_summary() {
        let orchestrator = OntologyDdlOrchestrator::default();
        let table = create_customers_table();

        let result = orchestrator
            .generate_ddl(&table, "postgresql")
            .await
            .unwrap();

        let summary = orchestrator.get_lineage_summary(&result);
        assert!(summary.is_some());

        let summary = summary.unwrap();
        assert!(summary.total_triples > 0);
        assert!(summary.entity_count > 0);
        assert!(summary.activity_count > 0);
    }

    #[tokio::test]
    async fn test_end_to_end_pipeline() {
        // This is the key integration test - verify complete pipeline
        let config = OntologyDdlConfig {
            skip_ontology_mapping: false,
            min_mapping_confidence: 0.7,
            strict_constraints: true,
            record_lineage: true,
            max_candidates: 5,
        };

        let orchestrator = OntologyDdlOrchestrator::new(config);
        let table = create_customers_table();

        // Execute full pipeline
        let result = orchestrator
            .generate_ddl(&table, "postgresql")
            .await
            .unwrap();

        // Verify Phase 2.2: Ontology Mapping
        assert!(result.ontology_mappings.len() >= 2);
        let email_mapped = result
            .ontology_mappings
            .iter()
            .any(|m| m.field_name == "email");
        let age_mapped = result
            .ontology_mappings
            .iter()
            .any(|m| m.field_name == "customer_age");
        assert!(email_mapped, "Email should be mapped to ontology");
        assert!(age_mapped, "Age should be mapped to ontology");

        // Verify Phase 2.3: SHACL Shape
        assert_eq!(result.shacl_shape.properties.len(), 3);
        assert!(result.shacl_shape.closed); // Strict mode

        // Verify Phase 2.4: RDF Lineage
        assert!(result.rdf_triples.is_some());
        let triples = result.rdf_triples.as_ref().unwrap();
        assert!(triples.len() > 10); // Should have substantial lineage

        // Verify lineage contains PROV relationships
        let has_prov_entity = triples.iter().any(|(_, _, o)| o.contains("prov#Entity"));
        let has_prov_activity = triples.iter().any(|(_, _, o)| o.contains("prov#Activity"));
        assert!(has_prov_entity, "Should have prov:Entity");
        assert!(has_prov_activity, "Should have prov:Activity");

        // Verify Phase 2.5: DDL Generation
        assert!(!result.ddl_statements.is_empty());
        let create_table = &result.ddl_statements[0];
        assert!(create_table.contains("CREATE TABLE CUSTOMERS"));

        // Verify table definition
        assert_eq!(result.table_definition.name, "CUSTOMERS");
        assert_eq!(result.table_definition.columns.len(), 3);
    }

    #[tokio::test]
    async fn test_multiple_tables_sequential() {
        let orchestrator = OntologyDdlOrchestrator::default();

        let customers = create_customers_table();
        let result1 = orchestrator
            .generate_ddl(&customers, "postgresql")
            .await
            .unwrap();

        // Create another table
        let mut products = DiscoveredTable {
            name: "products".to_string(),
            columns: vec![DiscoveredColumn {
                name: "price".to_string(),
                data_type: "DECIMAL(10,2)".to_string(),
                nullable: false,
                primary_key: false,
                semantic_type: None,
                confidence: 0.85,
                patterns: vec![],
                statistics: ColumnStatistics {
                    min_value: Some("0.01".to_string()),
                    max_value: Some("9999.99".to_string()),
                    ..Default::default()
                },
                sample_values: vec!["19.99".to_string(), "49.99".to_string()],
            }],
            row_count: Some(500),
        };

        let result2 = orchestrator
            .generate_ddl(&products, "postgresql")
            .await
            .unwrap();

        // Both should succeed
        assert!(!result1.ddl_statements.is_empty());
        assert!(!result2.ddl_statements.is_empty());

        // Price should be mapped to schema:price
        let price_mapping = result2
            .ontology_mappings
            .iter()
            .find(|m| m.field_name == "price");
        assert!(price_mapping.is_some());
        assert_eq!(
            price_mapping.unwrap().ontology_uri,
            "http://schema.org/price"
        );
    }

    #[tokio::test]
    async fn test_transformation_generation_from_ontology_mappings() {
        // GAP-003: Test that transformations are automatically generated from ontology mappings
        let orchestrator = OntologyDdlOrchestrator::default();
        let table = create_customers_table();

        let result = orchestrator
            .generate_ddl(&table, "postgresql")
            .await
            .unwrap();

        // Verify transformations were generated
        assert!(
            !result.transformations.is_empty(),
            "Should have transformations"
        );

        // Verify email field got transformation (LOWER(TRIM(email)))
        let email_transform = result
            .transformations
            .iter()
            .find(|t| t.field_name == "email");

        assert!(
            email_transform.is_some(),
            "Email field should have transformation"
        );
        let transform = email_transform.unwrap();
        assert_eq!(transform.ontology_uri, "http://schema.org/email");
        assert_eq!(transform.expression, "LOWER(TRIM(email))");
        assert!(
            transform.description.contains("email"),
            "Description should mention email"
        );

        // Verify transformations count matches ontology mappings with transformation rules
        let ontology_count = result.ontology_mappings.len();
        let transform_count = result.transformations.len();

        // Not all ontology mappings have transformations (e.g., age doesn't)
        // But all transformations should correspond to ontology mappings
        assert!(
            transform_count <= ontology_count,
            "Transformations ({}) should be <= ontology mappings ({})",
            transform_count,
            ontology_count
        );

        println!(
            "✓ Transformation generation test passed: {} ontology mappings → {} transformations",
            ontology_count, transform_count
        );
    }

    #[tokio::test]
    async fn test_custom_ontology_integration_infrastructure() {
        use crate::mapping::ontology_ddl::constraint_rules::OntologyConstraintRegistry;
        use crate::mapping::ontology_registry::RegistryClient;
        use graphica_core::catalog::OntologyRegistry;
        use parking_lot::RwLock;
        use std::sync::Arc;

        // This test verifies the plumbing is in place for custom ontology integration:
        // 1. OntologyConstraintRegistry can load from RegistryClient
        // 2. Orchestrator can be created with custom ontologies
        // 3. MappingResolver has access to the registry for term matching

        // Step 1: Create a simple ontology registry
        let ontology_registry = OntologyRegistry::new();

        // Step 2: Create RegistryClient
        let registry_client = RegistryClient::new(Some(Arc::new(RwLock::new(ontology_registry))));

        // Step 3: Create OntologyConstraintRegistry with custom ontologies
        // This verifies that with_custom_ontologies() works
        let constraint_registry =
            OntologyConstraintRegistry::with_custom_ontologies(&registry_client)
                .expect("Should create constraint registry from RegistryClient");

        // Should have at least default schema.org terms
        let uris = constraint_registry.get_all_uris();
        assert!(
            uris.len() >= 8,
            "Should have default terms, got {}",
            uris.len()
        );

        // Verify default terms are present
        assert!(constraint_registry.has_constraint("http://schema.org/email"));
        assert!(constraint_registry.has_constraint("http://schema.org/age"));

        // Step 4: Create Orchestrator with custom ontologies
        // This verifies the orchestrator constructor works
        let config = OntologyDdlConfig::default();
        let orchestrator =
            OntologyDdlOrchestrator::with_custom_ontologies(config, &registry_client)
                .expect("Should create orchestrator with custom ontologies");

        // Step 5: Verify orchestrator has access to the registry
        let orch_registry = orchestrator.registry();
        assert!(orch_registry.has_constraint("http://schema.org/email"));

        // Step 6: Verify basic DDL generation still works with default terms
        let table = create_customers_table();
        let result = orchestrator
            .generate_ddl(&table, "postgresql")
            .await
            .expect("Should generate DDL with default ontologies");

        // Should have DDL statements
        assert!(!result.ddl_statements.is_empty());

        // Should have ontology mappings (email should match)
        assert!(result.ontology_mappings.len() >= 1);
        let email_mapping = result
            .ontology_mappings
            .iter()
            .find(|m| m.field_name == "email");
        assert!(
            email_mapping.is_some(),
            "email should be mapped to schema.org"
        );
        assert_eq!(
            email_mapping.unwrap().ontology_uri,
            "http://schema.org/email"
        );

        println!(
            "✓ Custom ontology infrastructure test passed: registry has {} terms, generated {} DDL statements with {} mappings",
            orch_registry.get_all_uris().len(),
            result.ddl_statements.len(),
            result.ontology_mappings.len()
        );
    }
}
