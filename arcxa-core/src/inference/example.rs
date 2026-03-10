// graphica-core/src/inference/example.rs
//! Complete example demonstrating the schema inference pipeline.

#![allow(dead_code)]

use crate::inference::{
    types::*,
    traits::*,
    orchestrator::SchemaInferenceOrchestrator,
    rdf_converter::RdfConverter,
    postgres::PostgresInferrer,
};
use std::sync::Arc;

/// Example: Full inference workflow
///
/// This demonstrates the complete flow from database connection to RDF storage:
/// 1. Connect to PostgreSQL
/// 2. Run multi-tier inference
/// 3. Detect PII and calculate quality metrics
/// 4. Convert to RDF triples
/// 5. Store in governance brain
pub async fn run_complete_example() -> anyhow::Result<()> {
    // Step 1: Create PostgreSQL inferrer
    let database_url = "postgresql://user:pass@localhost/mydb";
    let pool = sqlx::PgPool::connect(database_url).await?;
    let inferrer = Arc::new(PostgresInferrer::new(pool, "src_prod_001".to_string()));

    // Step 2: Create orchestrator
    let orchestrator = SchemaInferenceOrchestrator::new();

    // Step 3: Start async inference job (Tier 3 = Governance)
    let job_id = orchestrator
        .start_inference_job(
            inferrer.clone(),
            "src_prod_001".to_string(),
            vec!["public".to_string()],
            InferenceTier::Governance,
        )
        .await?;

    println!("Started inference job: {}", job_id);

    // Step 4: Poll for completion
    loop {
        let status = orchestrator.get_job_status(&job_id).await;

        if let Some(job) = status {
            println!("Job status: {:?}", job.status);

            match job.status {
                JobStatus::Completed => {
                    println!("Job completed! Result URI: {:?}", job.result_uri);
                    break;
                }
                JobStatus::Failed => {
                    println!("Job failed: {:?}", job.error);
                    return Err(anyhow::anyhow!("Inference job failed"));
                }
                _ => {
                    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                }
            }
        }
    }

    // Step 5: Query results via synchronous call (demonstrates caching)
    let metadata = orchestrator
        .infer_sync(
            &*inferrer,
            "src_prod_001".to_string(),
            "public",
            InferenceTier::Governance,
        )
        .await?;

    println!("Discovered {} tables", metadata.tables.len());

    // Step 6: Analyze results
    for table in &metadata.tables {
        println!("\nTable: {}", table.name);
        println!("  Columns: {}", table.columns.len());

        if let Some(ref gov) = table.governance {
            println!("  Classification: {:?}", gov.data_classification);
            println!("  Quality Score: {:.1}%",
                (gov.quality_metrics.completeness + gov.quality_metrics.validity) / 2.0);
        }

        // Check for PII
        let pii_columns: Vec<_> = table.columns
            .iter()
            .filter(|c| c.pii_detected.is_some())
            .collect();

        if !pii_columns.is_empty() {
            println!("  PII Detected:");
            for col in pii_columns {
                if let Some(ref pii) = col.pii_detected {
                    println!("    - {} ({:?}, confidence: {:.2})",
                        col.name, pii.pii_type, pii.confidence);
                }
            }
        }
    }

    // Step 7: Convert to RDF
    let converter = RdfConverter::new("src_prod_001");
    let triples = converter.convert_schema_metadata(&metadata)?;

    println!("\nGenerated {} RDF triples", triples.len());

    // Step 8: Export to Turtle format
    let turtle = converter.triples_to_turtle(&triples);
    println!("\n--- RDF Turtle Preview ---");
    println!("{}", &turtle[..turtle.len().min(500)]);
    println!("...");

    Ok(())
}

/// Example: Query inference results via SPARQL
pub fn sparql_query_examples() -> Vec<String> {
    vec![
        // Example 1: Find all tables with PII
        r#"
PREFIX dcat: <http://www.w3.org/ns/dcat#>
PREFIX gph: <http://graphica.io/ontology#>

SELECT ?table_name ?pii_type ?confidence
WHERE {
  ?table a dcat:Distribution ;
         dcterms:title ?table_name ;
         gph:hasColumn ?column .
  ?column gph:hasPiiDetection ?pii .
  ?pii gph:piiType ?pii_type ;
       gph:confidence ?confidence .
  FILTER(?confidence > 0.8)
}
ORDER BY DESC(?confidence)
"#.to_string(),

        // Example 2: Find highly restricted tables
        r#"
PREFIX dcat: <http://www.w3.org/ns/dcat#>
PREFIX gph: <http://graphica.io/ontology#>

SELECT ?table_name ?classification
WHERE {
  ?table a dcat:Distribution ;
         dcterms:title ?table_name ;
         gph:hasGovernance ?gov .
  ?gov gph:classification ?classification .
  FILTER(?classification = "HighlyRestricted")
}
"#.to_string(),

        // Example 3: Find tables with foreign key relationships
        r#"
PREFIX schema: <http://graphica.io/schema#>

SELECT ?table_name ?fk_name ?ref_table
WHERE {
  ?table a dcat:Distribution ;
         dcterms:title ?table_name ;
         schema:hasForeignKey ?fk .
  ?fk schema:name ?fk_name ;
      schema:referencesTable ?ref_table_uri .
  ?ref_table_uri dcterms:title ?ref_table .
}
"#.to_string(),

        // Example 4: Calculate average completeness by schema
        r#"
PREFIX gph: <http://graphica.io/ontology#>

SELECT ?schema (AVG(?completeness) as ?avg_completeness)
WHERE {
  ?table a dcat:Distribution ;
         gph:hasGovernance ?gov .
  ?gov gph:completeness ?completeness .
  ?table dcterms:isPartOf ?dataset .
  ?dataset dcterms:title ?schema .
}
GROUP BY ?schema
ORDER BY DESC(?avg_completeness)
"#.to_string(),

        // Example 5: Find tables last modified more than 7 days ago
        r#"
PREFIX gph: <http://graphica.io/ontology#>
PREFIX xsd: <http://www.w3.org/2001/XMLSchema#>

SELECT ?table_name ?last_modified
WHERE {
  ?table a dcat:Distribution ;
         dcterms:title ?table_name ;
         gph:hasStatistics ?stats .
  ?stats dcterms:modified ?last_modified .
  FILTER(?last_modified < NOW() - "P7D"^^xsd:duration)
}
ORDER BY ?last_modified
"#.to_string(),
    ]
}

/// Example: Incremental inference workflow
pub async fn incremental_inference_example(
    orchestrator: &SchemaInferenceOrchestrator,
    inferrer: Arc<dyn SchemaInferrer + Send + Sync>,
    source_id: &str,
) -> anyhow::Result<()> {
    // Step 1: Start with basic inference (fast)
    let basic_meta = orchestrator
        .infer_sync(
            &*inferrer,
            source_id.to_string(),
            "public",
            InferenceTier::Basic,
        )
        .await?;

    println!("Quick scan found {} tables", basic_meta.tables.len());

    // Step 2: Identify high-value tables (e.g., large tables, customer data)
    let high_value_tables: Vec<_> = basic_meta
        .tables
        .iter()
        .filter(|t| {
            t.estimated_rows.unwrap_or(0) > 1_000_000
                || t.name.to_lowercase().contains("customer")
                || t.name.to_lowercase().contains("user")
        })
        .collect();

    println!("Identified {} high-value tables for deep inference", high_value_tables.len());

    // Step 3: Run deep inference only on high-value tables
    // (In practice, you'd implement per-table inference)
    if !high_value_tables.is_empty() {
        let deep_meta = orchestrator
            .infer_sync(
                &*inferrer,
                source_id.to_string(),
                "public",
                InferenceTier::Governance,
            )
            .await?;

        for table in &deep_meta.tables {
            if high_value_tables.iter().any(|t| t.name == table.name) {
                if let Some(ref gov) = table.governance {
                    println!(
                        "Table {} - Quality: {:.1}%, Classification: {:?}",
                        table.name,
                        gov.quality_metrics.completeness,
                        gov.data_classification
                    );
                }
            }
        }
    }

    Ok(())
}

/// Example: PII detection pipeline
pub async fn pii_detection_example() -> anyhow::Result<()> {
    use crate::inference::detectors::PiiDetector;

    let detector = PiiDetector::new();

    // Test columns
    let test_cases = vec![
        ("email", vec!["john@example.com", "jane.doe@company.org"]),
        ("ssn", vec!["123-45-6789", "987-65-4321"]),
        ("phone", vec!["555-123-4567", "555-987-6543"]),
        ("user_id", vec!["12345", "67890"]),
    ];

    println!("PII Detection Results:\n");

    for (col_name, samples) in test_cases {
        let samples: Vec<String> = samples.iter().map(|s| s.to_string()).collect();

        if let Some(detection) = detector.detect_pii(col_name, &samples) {
            println!("Column: {}", col_name);
            println!("  PII Type: {:?}", detection.pii_type);
            println!("  Confidence: {:.2}", detection.confidence);
            println!("  Method: {:?}", detection.detection_method);
            println!();
        } else {
            println!("Column: {} - No PII detected\n", col_name);
        }
    }

    Ok(())
}

/// Example: Quality metrics calculation
pub async fn quality_metrics_example() -> anyhow::Result<()> {
    use crate::inference::detectors::QualityCalculator;

    // Simulate table with 10,000 rows
    let total_rows = 10_000;
    let null_rows = 150;
    let distinct_values = 8_500;

    let completeness = QualityCalculator::completeness(null_rows, total_rows);
    let uniqueness = QualityCalculator::uniqueness(distinct_values, total_rows);

    println!("Quality Metrics:");
    println!("  Total Rows: {}", total_rows);
    println!("  Null Rows: {}", null_rows);
    println!("  Distinct Values: {}", distinct_values);
    println!("  Completeness: {:.2}%", completeness);
    println!("  Uniqueness: {:.2}%", uniqueness);

    // Overall score
    let metrics = DataQualityMetrics {
        completeness,
        uniqueness,
        validity: 97.5,
        consistency: 98.0,
        timeliness: 85.0,
        accuracy_score: None,
    };

    let overall = QualityCalculator::overall_score(&metrics);
    println!("  Overall Quality Score: {:.2}%", overall);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sparql_queries_valid() {
        let queries = sparql_query_examples();
        assert_eq!(queries.len(), 5);

        for query in queries {
            assert!(query.contains("SELECT"));
            assert!(query.contains("WHERE"));
        }
    }

    #[tokio::test]
    async fn test_pii_detection() {
        let result = pii_detection_example().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_quality_metrics() {
        let result = quality_metrics_example().await;
        assert!(result.is_ok());
    }
}
