//! # Tests for Unified Ontology Mapping System

#[cfg(test)]
mod tests {
    use super::super::*;
    use std::collections::HashMap;
    use std::sync::Arc;

    // Helper function to create test field
    fn create_test_field(name: &str, data_type: &str, samples: Vec<String>) -> FieldDescriptor {
        FieldDescriptor {
            id: format!("test_{}", name),
            name: name.to_string(),
            normalized_name: name
                .to_lowercase()
                .chars()
                .filter(|c| c.is_alphanumeric())
                .collect(),
            data_type: data_type.to_string(),
            nullable: false,
            primary_key: false,
            sample_values: samples,
            description: None,
            source_id: "test_source".to_string(),
            table_name: "test_table".to_string(),
            statistics: None,
        }
    }

    #[tokio::test]
    async fn test_pattern_strategy_email() {
        // Create pattern detector
        let detector = Arc::new(shared::PatternDetectorImpl::new());
        let strategy = strategies::PatternStrategy::new(detector);

        // Create test field with email samples
        let field = create_test_field(
            "customer_email",
            "VARCHAR",
            vec![
                "john@example.com".to_string(),
                "jane.doe@company.org".to_string(),
                "support@test.net".to_string(),
            ],
        );

        // Test that strategy applies
        assert!(strategy.applies_to(&field));

        // Find matches
        let context = MatchingContext {
            embedding_cache: None,
            ontology_cache: None,
            pattern_detector: None,
            metadata: HashMap::new(),
        };

        let matches = strategy.find_matches(&field, &[], &context).await.unwrap();

        // Should find email pattern
        assert!(!matches.is_empty());
        assert_eq!(matches[0].ontology_uri, "http://schema.org/email");
        assert_eq!(matches[0].confidence, 0.95);
    }

    #[tokio::test]
    async fn test_lexical_strategy() {
        let strategy = strategies::LexicalStrategy::new();

        // Use "emails" which has high similarity to "email"
        let field = create_test_field("emails", "VARCHAR", vec![]);

        let terms = vec![
            OntologyTerm {
                uri: "http://schema.org/email".to_string(),
                label: "email".to_string(),
                namespace: "schema.org".to_string(),
                term_type: OntologyTermType::Property,
                description: None,
                data_type: None,
                alt_labels: vec![],
            },
            OntologyTerm {
                uri: "http://schema.org/name".to_string(),
                label: "name".to_string(),
                namespace: "schema.org".to_string(),
                term_type: OntologyTermType::Property,
                description: None,
                data_type: None,
                alt_labels: vec![],
            },
        ];

        let context = MatchingContext {
            embedding_cache: None,
            ontology_cache: None,
            pattern_detector: None,
            metadata: HashMap::new(),
        };

        let matches = strategy
            .find_matches(&field, &terms, &context)
            .await
            .unwrap();

        // Should find email as better match (emails vs email has high similarity)
        assert!(!matches.is_empty());
        let email_match = matches.iter().find(|m| m.ontology_uri.contains("email"));
        assert!(email_match.is_some());
    }

    #[tokio::test]
    async fn test_confidence_scoring_single_strategy() {
        let scorer = scoring::ConfidenceScorerBuilder::new()
            .with_weight("pattern", 1.0)
            .build();

        let matches = vec![StrategyMatch {
            strategy_name: "pattern".to_string(),
            ontology_uri: "http://schema.org/email".to_string(),
            confidence: 0.9,
            explanation: "Pattern match".to_string(),
            metadata: HashMap::new(),
        }];

        let candidates = scorer.score_candidates(matches);

        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].confidence, 0.9);
        assert_eq!(candidates[0].ontology_uri, "http://schema.org/email");
    }

    #[tokio::test]
    async fn test_confidence_scoring_multiple_strategies() {
        let scorer = scoring::ConfidenceScorerBuilder::new()
            .with_weight("pattern", 1.5)
            .with_weight("lexical", 0.8)
            .build();

        let matches = vec![
            StrategyMatch {
                strategy_name: "pattern".to_string(),
                ontology_uri: "http://schema.org/email".to_string(),
                confidence: 0.95,
                explanation: "Pattern match".to_string(),
                metadata: HashMap::new(),
            },
            StrategyMatch {
                strategy_name: "lexical".to_string(),
                ontology_uri: "http://schema.org/email".to_string(),
                confidence: 0.70,
                explanation: "Lexical match".to_string(),
                metadata: HashMap::new(),
            },
        ];

        let candidates = scorer.score_candidates(matches);

        assert_eq!(candidates.len(), 1);
        // Weighted average: (0.95 * 1.5 + 0.70 * 0.8) / (1.5 + 0.8)
        let expected = (0.95 * 1.5 + 0.70 * 0.8) / 2.3;
        assert!((candidates[0].confidence - expected).abs() < 0.01);
    }

    #[tokio::test]
    async fn test_pattern_detector() {
        let detector = shared::PatternDetectorImpl::new();

        // Test email detection
        let email_samples = vec!["john@example.com".to_string(), "jane@test.org".to_string()];
        let email_patterns = detector.detect_patterns(&email_samples);
        assert!(!email_patterns.is_empty());
        assert_eq!(email_patterns[0].pattern_type, PatternType::Email);
        assert_eq!(email_patterns[0].confidence, 1.0);

        // Test phone detection
        let phone_samples = vec!["555-123-4567".to_string(), "(555) 987-6543".to_string()];
        let phone_patterns = detector.detect_patterns(&phone_samples);
        assert!(!phone_patterns.is_empty());
        assert_eq!(phone_patterns[0].pattern_type, PatternType::Phone);

        // Test mixed samples
        let mixed_samples = vec![
            "john@example.com".to_string(),
            "not an email".to_string(),
            "another@test.org".to_string(),
        ];
        let mixed_patterns = detector.detect_patterns(&mixed_samples);
        assert!(!mixed_patterns.is_empty());
        assert_eq!(mixed_patterns[0].pattern_type, PatternType::Email);
        assert!(mixed_patterns[0].confidence > 0.5);
    }

    #[tokio::test]
    async fn test_heuristic_strategy() {
        let strategy = strategies::HeuristicStrategy::new();

        let field = create_test_field("customer_email", "VARCHAR", vec![]);

        let context = MatchingContext {
            embedding_cache: None,
            ontology_cache: None,
            pattern_detector: None,
            metadata: HashMap::new(),
        };

        let matches = strategy.find_matches(&field, &[], &context).await.unwrap();

        // Should match based on field name containing "email"
        assert!(!matches.is_empty());
        let email_match = matches.iter().find(|m| m.ontology_uri.contains("email"));
        assert!(email_match.is_some());
    }

    #[tokio::test]
    async fn test_unified_engine_integration() {
        // Create config
        let config = UnifiedMappingConfig::default();

        // Create unified engine (without external services)
        let engine = UnifiedOntologyMappingEngine::new(
            config, None, // No registry client
            None, // No manual mapping store
        )
        .await
        .unwrap();

        // Create test field
        let field = create_test_field(
            "customer_email",
            "VARCHAR",
            vec!["john@example.com".to_string(), "jane@test.org".to_string()],
        );

        // Map field
        let options = MappingOptions {
            min_confidence: 0.5,
            max_candidates: 5,
            ontology_namespaces: None,
            enabled_strategies: None,
            use_cache: true,
            timeout_ms: Some(5000),
        };

        let candidates = engine.map_field(&field, &options).await.unwrap();

        // Should find email mapping with high confidence
        assert!(!candidates.is_empty());
        assert!(candidates[0].ontology_uri.contains("email"));
        assert!(candidates[0].confidence > 0.8);
    }

    #[tokio::test]
    async fn test_batch_mapping() {
        let config = UnifiedMappingConfig::default();
        let engine = UnifiedOntologyMappingEngine::new(config, None, None)
            .await
            .unwrap(); // No manual mapping store

        let fields = vec![
            create_test_field(
                "customer_email",
                "VARCHAR",
                vec!["john@example.com".to_string()],
            ),
            create_test_field("customer_name", "VARCHAR", vec!["John Doe".to_string()]),
            create_test_field("customer_age", "INTEGER", vec!["30".to_string()]),
        ];

        let options = MappingOptions::default();
        let results = engine.map_fields(&fields, &options).await.unwrap();

        assert_eq!(results.len(), 3);

        // Check each field was mapped
        for result in results {
            assert!(!result.candidates.is_empty() || !result.errors.is_empty());
        }
    }

    #[tokio::test]
    async fn test_strategy_filtering() {
        let config = UnifiedMappingConfig::default();
        let engine = UnifiedOntologyMappingEngine::new(config, None, None)
            .await
            .unwrap(); // No manual mapping store

        let field = create_test_field(
            "customer_email",
            "VARCHAR",
            vec!["john@example.com".to_string()],
        );

        // Only enable pattern strategy
        let options = MappingOptions {
            min_confidence: 0.5,
            max_candidates: 5,
            ontology_namespaces: None,
            enabled_strategies: Some(vec!["pattern".to_string()]),
            use_cache: false,
            timeout_ms: None,
        };

        let candidates = engine.map_field(&field, &options).await.unwrap();

        // Should only have pattern-based matches
        assert!(!candidates.is_empty());
        for candidate in candidates {
            assert!(candidate
                .evidence
                .iter()
                .all(|e| e.strategy_name == "pattern"));
        }
    }

    // ============================================================================
    // Integration Tests: Manual Mapping → Statistical Matcher Feedback Loop
    // ============================================================================

    use crate::governance::rdf_store::GraphicaRdfStore;
    use crate::mapping::manual::{
        ManualFieldMapping, ManualMappingStore, SourceContext, UsageStats,
    };
    use tempfile::TempDir;

    /// Helper to create a test manual mapping store
    async fn create_test_manual_store() -> (Arc<ManualMappingStore>, TempDir) {
        let rdf_store = Arc::new(GraphicaRdfStore::new_in_memory().unwrap());
        let temp_dir = TempDir::new().unwrap();
        let rocksdb_path = temp_dir.path().join("rocksdb");
        let store =
            Arc::new(ManualMappingStore::new(rdf_store, rocksdb_path.to_str().unwrap()).unwrap());
        (store, temp_dir)
    }

    #[tokio::test]
    async fn test_manual_mapping_overrides_pattern_strategy() {
        // Create manual mapping store with a predefined mapping
        let (manual_store, _temp_dir) = create_test_manual_store().await;

        // Add a manual mapping: customer_email -> http://schema.org/email
        let manual_mapping = ManualFieldMapping {
            id: "manual_email_001".to_string(),
            source_context: SourceContext {
                source_id: Some("test_source".to_string()),
                table_name: "test_table".to_string(),
                field_name: "customer_email".to_string(),
                field_metadata: None,
            },
            target_field_uri: "http://schema.org/email".to_string(),
            confidence: 1.0,
            created_by: "test_user".to_string(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            notes: Some("Manually mapped by data steward".to_string()),
            usage_stats: UsageStats {
                apply_count: 0,
                accept_count: 0,
                reject_count: 0,
                last_used: None,
            },
        };

        manual_store.store_mapping(manual_mapping).await.unwrap();

        // Create unified engine with manual mapping store
        let config = UnifiedMappingConfig::default();
        let engine = UnifiedOntologyMappingEngine::new(
            config,
            None, // No registry client
            Some(manual_store.clone()),
        )
        .await
        .unwrap();

        // Create test field matching the manual mapping
        let field = create_test_field(
            "customer_email",
            "VARCHAR",
            vec!["john@example.com".to_string(), "jane@test.org".to_string()],
        );

        // Map the field
        let options = MappingOptions {
            min_confidence: 0.5,
            max_candidates: 5,
            ontology_namespaces: None,
            enabled_strategies: None, // Enable all strategies
            use_cache: false,
            timeout_ms: Some(5000),
        };

        let candidates = engine.map_field(&field, &options).await.unwrap();

        // Should find the manual mapping as top candidate
        assert!(!candidates.is_empty());

        // Manual mapping should be first (highest confidence)
        let top_candidate = &candidates[0];
        assert_eq!(top_candidate.ontology_uri, "http://schema.org/email");
        assert_eq!(top_candidate.confidence, 1.0);

        // Should have manual strategy in evidence
        let manual_evidence = top_candidate
            .evidence
            .iter()
            .find(|e| e.strategy_name == "manual");
        assert!(manual_evidence.is_some());
        assert_eq!(manual_evidence.unwrap().confidence, 1.0);

        // Verify usage stats were updated (apply_count should be incremented)
        // Note: Usage stats update is fire-and-forget, so we need to wait a bit
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let updated_mapping = manual_store.get_mapping("manual_email_001").await.unwrap();
        assert!(updated_mapping.is_some());
        let stats = updated_mapping.unwrap().usage_stats;
        assert_eq!(stats.apply_count, 1, "apply_count should be incremented");
    }

    #[tokio::test]
    async fn test_manual_mapping_priority_over_statistical() {
        let (manual_store, _temp_dir) = create_test_manual_store().await;

        // Add manual mapping for "user_name" -> ontology URI
        let manual_mapping = ManualFieldMapping {
            id: "manual_name_001".to_string(),
            source_context: SourceContext {
                source_id: Some("test_source".to_string()),
                table_name: "test_table".to_string(),
                field_name: "user_name".to_string(),
                field_metadata: None,
            },
            target_field_uri: "http://schema.org/name".to_string(),
            confidence: 1.0,
            created_by: "data_steward".to_string(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            notes: Some("Custom business mapping".to_string()),
            usage_stats: UsageStats::default(),
        };

        manual_store.store_mapping(manual_mapping).await.unwrap();

        // Create engine with manual store
        let mut config = UnifiedMappingConfig::default();

        // Enable both manual and statistical strategies
        config.strategies.insert(
            "manual".to_string(),
            StrategyConfig {
                enabled: true,
                weight: 2.0, // Manual gets highest weight
                min_confidence: None,
                max_confidence: None,
                settings: HashMap::new(),
            },
        );
        config.strategies.insert(
            "statistical".to_string(),
            StrategyConfig {
                enabled: true,
                weight: 1.0,
                min_confidence: None,
                max_confidence: None,
                settings: HashMap::new(),
            },
        );

        let engine = UnifiedOntologyMappingEngine::new(config, None, Some(manual_store.clone()))
            .await
            .unwrap();

        let field = create_test_field("user_name", "VARCHAR", vec!["John Doe".to_string()]);

        let options = MappingOptions {
            min_confidence: 0.3,
            max_candidates: 10,
            ontology_namespaces: None,
            enabled_strategies: None,
            use_cache: false,
            timeout_ms: Some(5000),
        };

        let candidates = engine.map_field(&field, &options).await.unwrap();

        // Manual mapping should be first
        assert!(!candidates.is_empty());
        assert_eq!(candidates[0].ontology_uri, "http://schema.org/name");
        assert_eq!(candidates[0].confidence, 1.0);

        // Check that manual strategy has highest weight in evidence
        let manual_evidence = candidates[0]
            .evidence
            .iter()
            .find(|e| e.strategy_name == "manual");
        assert!(manual_evidence.is_some());
    }

    #[tokio::test]
    async fn test_multiple_manual_mappings_for_different_fields() {
        let (manual_store, _temp_dir) = create_test_manual_store().await;

        // Add multiple manual mappings
        let mappings = vec![
            ManualFieldMapping {
                id: "manual_001".to_string(),
                source_context: SourceContext {
                    source_id: Some("test_source".to_string()),
                    table_name: "test_table".to_string(),
                    field_name: "customer_email".to_string(),
                    field_metadata: None,
                },
                target_field_uri: "http://schema.org/email".to_string(),
                confidence: 1.0,
                created_by: "user1".to_string(),
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
                notes: None,
                usage_stats: UsageStats::default(),
            },
            ManualFieldMapping {
                id: "manual_002".to_string(),
                source_context: SourceContext {
                    source_id: Some("test_source".to_string()),
                    table_name: "test_table".to_string(),
                    field_name: "customer_phone".to_string(),
                    field_metadata: None,
                },
                target_field_uri: "http://schema.org/telephone".to_string(),
                confidence: 1.0,
                created_by: "user1".to_string(),
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
                notes: None,
                usage_stats: UsageStats::default(),
            },
            ManualFieldMapping {
                id: "manual_003".to_string(),
                source_context: SourceContext {
                    source_id: Some("test_source".to_string()),
                    table_name: "test_table".to_string(),
                    field_name: "customer_age".to_string(),
                    field_metadata: None,
                },
                target_field_uri: "http://schema.org/age".to_string(),
                confidence: 1.0,
                created_by: "user2".to_string(),
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
                notes: None,
                usage_stats: UsageStats::default(),
            },
        ];

        for mapping in mappings.clone() {
            manual_store.store_mapping(mapping).await.unwrap();
        }

        // Create engine
        let config = UnifiedMappingConfig::default();
        let engine = UnifiedOntologyMappingEngine::new(config, None, Some(manual_store.clone()))
            .await
            .unwrap();

        // Test batch mapping
        let fields = vec![
            create_test_field(
                "customer_email",
                "VARCHAR",
                vec!["john@example.com".to_string()],
            ),
            create_test_field("customer_phone", "VARCHAR", vec!["555-1234".to_string()]),
            create_test_field("customer_age", "INTEGER", vec!["30".to_string()]),
        ];

        let options = MappingOptions::default();
        let results = engine.map_fields(&fields, &options).await.unwrap();

        assert_eq!(results.len(), 3);

        // Verify each field got its manual mapping
        for (i, result) in results.iter().enumerate() {
            assert!(
                !result.candidates.is_empty(),
                "Field {} should have candidates",
                i
            );
            assert_eq!(
                result.candidates[0].confidence, 1.0,
                "Field {} should have confidence 1.0",
                i
            );

            // Verify correct URIs
            match i {
                0 => assert!(result.candidates[0].ontology_uri.contains("email")),
                1 => assert!(result.candidates[0].ontology_uri.contains("telephone")),
                2 => assert!(result.candidates[0].ontology_uri.contains("age")),
                _ => unreachable!(),
            }
        }

        // Verify all usage stats were updated
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        for mapping in &mappings {
            let updated = manual_store
                .get_mapping(&mapping.id)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(updated.usage_stats.apply_count, 1);
        }
    }

    #[tokio::test]
    async fn test_manual_mapping_feedback_with_usage_stats() {
        let (manual_store, _temp_dir) = create_test_manual_store().await;

        // Create a manual mapping
        let mapping = ManualFieldMapping {
            id: "feedback_test_001".to_string(),
            source_context: SourceContext {
                source_id: Some("test_source".to_string()),
                table_name: "test_table".to_string(),
                field_name: "email_address".to_string(),
                field_metadata: None,
            },
            target_field_uri: "http://schema.org/email".to_string(),
            confidence: 1.0,
            created_by: "test_user".to_string(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            notes: None,
            usage_stats: UsageStats {
                apply_count: 5,
                accept_count: 4,
                reject_count: 0,
                last_used: Some(chrono::Utc::now()),
            },
        };

        manual_store.store_mapping(mapping).await.unwrap();

        // Create engine
        let config = UnifiedMappingConfig::default();
        let engine = UnifiedOntologyMappingEngine::new(config, None, Some(manual_store.clone()))
            .await
            .unwrap();

        // Map the field multiple times
        let field = create_test_field(
            "email_address",
            "VARCHAR",
            vec!["test@example.com".to_string()],
        );
        let options = MappingOptions::default();

        for _ in 0..3 {
            let candidates = engine.map_field(&field, &options).await.unwrap();
            assert!(!candidates.is_empty());
            assert_eq!(candidates[0].confidence, 1.0);
        }

        // Wait for async usage stat updates
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

        // Verify usage stats incremented
        let updated_mapping = manual_store
            .get_mapping("feedback_test_001")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            updated_mapping.usage_stats.apply_count, 8,
            "apply_count should increase from 5 to 8"
        );
        assert!(updated_mapping.usage_stats.last_used.is_some());
    }

    #[tokio::test]
    async fn test_no_manual_mapping_falls_back_to_other_strategies() {
        let (manual_store, _temp_dir) = create_test_manual_store().await;

        // Create engine with manual store but NO manual mappings
        let config = UnifiedMappingConfig::default();
        let engine = UnifiedOntologyMappingEngine::new(config, None, Some(manual_store.clone()))
            .await
            .unwrap();

        // Try to map a field with no manual mapping
        let field = create_test_field(
            "customer_email",
            "VARCHAR",
            vec!["john@example.com".to_string()],
        );

        let options = MappingOptions {
            min_confidence: 0.5,
            max_candidates: 5,
            ontology_namespaces: None,
            enabled_strategies: None,
            use_cache: false,
            timeout_ms: Some(5000),
        };

        let candidates = engine.map_field(&field, &options).await.unwrap();

        // Should still find matches from other strategies (pattern, heuristic, etc.)
        assert!(!candidates.is_empty());

        // But confidence should be < 1.0 (not from manual strategy)
        assert!(candidates[0].confidence < 1.0);

        // Verify no manual strategy in evidence
        let has_manual = candidates[0]
            .evidence
            .iter()
            .any(|e| e.strategy_name == "manual");
        assert!(
            !has_manual,
            "Should not have manual strategy evidence when no manual mapping exists"
        );
    }

    #[tokio::test]
    async fn test_manual_mapping_with_source_context_filtering() {
        let (manual_store, _temp_dir) = create_test_manual_store().await;

        // Add manual mapping for specific source_id
        let mapping = ManualFieldMapping {
            id: "source_specific_001".to_string(),
            source_context: SourceContext {
                source_id: Some("production_db".to_string()),
                table_name: "customers".to_string(),
                field_name: "email".to_string(),
                field_metadata: None,
            },
            target_field_uri: "http://schema.org/email".to_string(),
            confidence: 1.0,
            created_by: "admin".to_string(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            notes: None,
            usage_stats: UsageStats::default(),
        };

        manual_store.store_mapping(mapping).await.unwrap();

        let config = UnifiedMappingConfig::default();
        let engine = UnifiedOntologyMappingEngine::new(config, None, Some(manual_store.clone()))
            .await
            .unwrap();

        // Test 1: Field from matching source should get manual mapping
        let field_matching = FieldDescriptor {
            id: "test_email_1".to_string(),
            name: "email".to_string(),
            normalized_name: "email".to_string(),
            data_type: "VARCHAR".to_string(),
            nullable: false,
            primary_key: false,
            sample_values: vec!["test@example.com".to_string()],
            description: None,
            source_id: "production_db".to_string(),
            table_name: "customers".to_string(),
            statistics: None,
        };

        let options = MappingOptions::default();
        let candidates_matching = engine.map_field(&field_matching, &options).await.unwrap();

        assert!(!candidates_matching.is_empty());
        assert_eq!(candidates_matching[0].confidence, 1.0);
        assert_eq!(
            candidates_matching[0].ontology_uri,
            "http://schema.org/email"
        );

        // Test 2: Field from different source should fall back to other strategies
        let field_different = FieldDescriptor {
            id: "test_email_2".to_string(),
            name: "email".to_string(),
            normalized_name: "email".to_string(),
            data_type: "VARCHAR".to_string(),
            nullable: false,
            primary_key: false,
            sample_values: vec!["test@example.com".to_string()],
            description: None,
            source_id: "staging_db".to_string(), // Different source
            table_name: "customers".to_string(),
            statistics: None,
        };

        let candidates_different = engine.map_field(&field_different, &options).await.unwrap();

        // Should still find matches but not from manual strategy
        assert!(!candidates_different.is_empty());

        // May or may not have confidence 1.0 depending on other strategies
        let has_manual = candidates_different[0]
            .evidence
            .iter()
            .any(|e| e.strategy_name == "manual");
        assert!(
            !has_manual,
            "Should not match manual mapping for different source_id"
        );
    }
}
