//! Comprehensive ETL Row-Level Lineage Integration Test
//!
//! This test validates the complete ETL pipeline:
//! 1. CSV ingestion with row-level tracking
//! 2. Data transformation with lineage capture
//! 3. DB2 loading with destination tracking
//! 4. Quality rule filtering with rejection reasons
//! 5. End-to-end lineage query validation

use anyhow::Result;
use chrono::{DateTime, Utc};
use graphica_core::core::lineage::row_level::{
    DatabaseType, JobStatistics, JourneyStep, ProcessingOutcome, QualityViolation, RowId,
    RowJourney, RowLevelLineageSink, RowLineageEvent, RowTransformation,
};
use serde_json::json;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use tempfile::TempDir;
use tokio;

/// Test CSV data with various scenarios
const TEST_CSV: &str = r#"customer_id,name,email,age,country
1001,John Doe,john@example.com,35,USA
1002,Jane Smith,jane@example.com,28,Canada
1003,Bob Wilson,,42,USA
1004,Alice Brown,alice@invalid,25,UK
1005,Charlie Davis,charlie@example.com,-5,Australia
1006,Eve Martinez,eve@example.com,30,Mexico
1007,Frank,frank@example.com,55,USA
1008,Grace Lee,grace@example.com,abc,China
1009,Henry Taylor,henry@example.com,40,France
1010,Ivy Chen,ivy@example.com,33,Singapore"#;

/// Mock row lineage store for testing
struct MockRowLineageStore {
    events: Arc<tokio::sync::Mutex<Vec<RowLineageEvent>>>,
}

impl MockRowLineageStore {
    fn new() -> Self {
        Self {
            events: Arc::new(tokio::sync::Mutex::new(Vec::new())),
        }
    }

    async fn get_all_events(&self) -> Vec<RowLineageEvent> {
        self.events.lock().await.clone()
    }
}

#[async_trait::async_trait]
impl RowLevelLineageSink for MockRowLineageStore {
    async fn write_row(&self, event: RowLineageEvent) -> Result<()> {
        self.events.lock().await.push(event);
        Ok(())
    }

    async fn write_rows_batch(&self, events: Vec<RowLineageEvent>) -> Result<()> {
        self.events.lock().await.extend(events);
        Ok(())
    }

    async fn get_row_lineage(&self, row_id: &RowId) -> Result<Vec<RowLineageEvent>> {
        let events = self.events.lock().await;
        Ok(events
            .iter()
            .filter(|e| &e.row_id == row_id)
            .cloned()
            .collect())
    }

    async fn trace_row_journey(&self, row_id: &RowId) -> Result<RowJourney> {
        let events = self.get_row_lineage(row_id).await?;

        if events.is_empty() {
            return Ok(RowJourney {
                source: row_id.clone(),
                steps: Vec::new(),
                destination: None,
                total_duration_ms: 0,
            });
        }

        // Build journey from events
        let mut steps = Vec::new();
        let mut total_duration: u64 = 0;
        let mut destination = None;

        for event in &events {
            for (i, transform) in event.transformations.iter().enumerate() {
                let duration_ms = 10; // Mock duration
                total_duration += duration_ms;

                steps.push(JourneyStep {
                    activity: transform.transform_type.clone(),
                    timestamp: transform.applied_at,
                    duration_ms,
                    outcome: event.outcome.clone(),
                });
            }

            if event.is_success() {
                destination = event.output_row_id.clone();
            }
        }

        Ok(RowJourney {
            source: row_id.clone(),
            steps,
            destination,
            total_duration_ms: total_duration,
        })
    }

    async fn search_row_keys(&self, query: &str, limit: usize) -> Result<Vec<RowId>> {
        let normalized_query = query.trim().to_ascii_lowercase();
        if normalized_query.is_empty() || limit == 0 {
            return Ok(Vec::new());
        }

        let events = self.events.lock().await;
        let mut row_ids = Vec::new();

        for row_id in events
            .iter()
            .map(|event| event.row_id.clone())
            .filter(|row_id| {
                row_id
                    .to_key()
                    .to_ascii_lowercase()
                    .contains(&normalized_query)
            })
        {
            if row_ids.iter().any(|existing: &RowId| existing == &row_id) {
                continue;
            }
            row_ids.push(row_id);
            if row_ids.len() >= limit {
                break;
            }
        }

        Ok(row_ids)
    }

    async fn get_batch_lineage(&self, batch_id: &str) -> Result<Vec<RowLineageEvent>> {
        let events = self.events.lock().await;
        Ok(events
            .iter()
            .filter(|e| e.batch_id == batch_id)
            .cloned()
            .collect())
    }

    async fn get_job_stats(&self, job_id: &str) -> Result<JobStatistics> {
        let events = self.events.lock().await;
        let job_events: Vec<_> = events.iter().filter(|e| e.job_id == job_id).collect();

        let total_rows = job_events.len() as u64;
        let success_count = job_events.iter().filter(|e| e.is_success()).count() as u64;
        let filtered_count = job_events.iter().filter(|e| e.is_filtered()).count() as u64;
        let failed_count = job_events
            .iter()
            .filter(|e| matches!(e.outcome, ProcessingOutcome::Failed { .. }))
            .count() as u64;

        let mut filter_reasons = HashMap::new();
        for event in &job_events {
            if let ProcessingOutcome::Filtered { reason, .. } = &event.outcome {
                *filter_reasons.entry(reason.clone()).or_insert(0) += 1;
            }
        }

        let start_time = job_events
            .first()
            .map(|e| e.timestamp)
            .unwrap_or_else(Utc::now);
        let end_time = job_events.last().map(|e| e.timestamp);

        Ok(JobStatistics {
            job_id: job_id.to_string(),
            total_rows,
            success_count,
            filtered_count,
            failed_count,
            filter_reasons,
            avg_processing_time_ms: 10.0, // Mock value
            start_time,
            end_time,
        })
    }

    async fn get_filtered_rows(
        &self,
        job_id: &str,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> Result<Vec<(RowId, String)>> {
        let events = self.events.lock().await;
        let mut filtered = Vec::new();

        for event in events.iter() {
            if event.job_id != job_id {
                continue;
            }

            if event.timestamp < start_time || event.timestamp > end_time {
                continue;
            }

            if let ProcessingOutcome::Filtered { reason, .. } = &event.outcome {
                filtered.push((event.row_id.clone(), reason.clone()));
            }
        }

        Ok(filtered)
    }

    async fn get_row_transformations(&self, row_id: &RowId) -> Result<Vec<RowTransformation>> {
        let events = self.get_row_lineage(row_id).await?;
        Ok(events.into_iter().flat_map(|e| e.transformations).collect())
    }
}

/// CSV row data
#[derive(Debug, Clone)]
struct CustomerRow {
    customer_id: String,
    name: String,
    email: String,
    age: String,
    country: String,
    row_number: u64,
}

/// Quality validation rules
fn validate_customer_row(row: &CustomerRow) -> Result<(), String> {
    // Check for empty email
    if row.email.is_empty() {
        return Err("Missing required field: email".to_string());
    }

    // Check email format
    if !row.email.contains('@') || !row.email.contains('.') {
        return Err("Invalid email format".to_string());
    }

    // Check for missing last name (name should have at least 2 parts)
    let name_parts: Vec<&str> = row.name.split_whitespace().collect();
    if name_parts.len() < 2 {
        return Err("Missing last name".to_string());
    }

    // Check age is numeric
    if row.age.parse::<i32>().is_err() {
        return Err("Invalid age: non-numeric".to_string());
    }

    // Check age is positive
    if let Ok(age_val) = row.age.parse::<i32>() {
        if age_val < 0 {
            return Err("Invalid age: negative value".to_string());
        }
    }

    Ok(())
}

/// ETL Pipeline Processor
struct EtlPipeline {
    lineage_store: Arc<dyn RowLevelLineageSink>,
    job_id: String,
    batch_id: String,
    tenant_id: String,
}

impl EtlPipeline {
    fn new(
        lineage_store: Arc<dyn RowLevelLineageSink>,
        job_id: String,
        batch_id: String,
        tenant_id: String,
    ) -> Self {
        Self {
            lineage_store,
            job_id,
            batch_id,
            tenant_id,
        }
    }

    /// Process CSV file with full lineage tracking
    async fn process_csv(&self, csv_path: &str) -> Result<ProcessingResult> {
        let mut reader = csv::Reader::from_path(csv_path)?;
        let mut results = ProcessingResult::default();

        // Note: csv::Reader::records() already skips the header row
        for (idx, record) in reader.records().enumerate() {
            let record = match record {
                Ok(r) => r,
                Err(_) => {
                    results.errors += 1;
                    continue;
                }
            };

            // Row number in CSV file (1-based, including header)
            // idx 0 = row 2 (first data row after header)
            let row_num = (idx + 2) as u64;

            // Parse row
            let customer_row = CustomerRow {
                customer_id: record.get(0).unwrap_or("").to_string(),
                name: record.get(1).unwrap_or("").to_string(),
                email: record.get(2).unwrap_or("").to_string(),
                age: record.get(3).unwrap_or("").to_string(),
                country: record.get(4).unwrap_or("").to_string(),
                row_number: row_num,
            };

            // Create source row ID
            let source_row_id = RowId::csv(csv_path.to_string(), row_num);

            // Apply transformations and track lineage
            match self.process_row(&customer_row, source_row_id).await {
                Ok(_) => {
                    results.processed += 1;
                }
                Err(rejection_reason) => {
                    results.filtered += 1;
                    results.filtered_reasons.push((row_num, rejection_reason));
                }
            }
        }

        Ok(results)
    }

    /// Process a single row with lineage tracking
    async fn process_row(&self, row: &CustomerRow, source_row_id: RowId) -> Result<(), String> {
        let mut transformations = Vec::new();

        // Step 1: Standardization (trim whitespace, lowercase email)
        let _standardized_email = row.email.trim().to_lowercase();
        transformations.push(RowTransformation::new(
            "standardization",
            vec!["email".to_string()],
        ));

        // Step 2: Quality validation
        if let Err(validation_error) = validate_customer_row(row) {
            // Record filtered event
            let mut event = RowLineageEvent::filtered(
                source_row_id.clone(),
                self.batch_id.clone(),
                self.job_id.clone(),
                validation_error.clone(),
                "quality_check_rule_001".to_string(),
                self.tenant_id.clone(),
            );
            event.transformations = transformations;

            self.lineage_store
                .write_row(event)
                .await
                .map_err(|e| format!("Failed to record lineage: {}", e))?;

            return Err(validation_error);
        }

        transformations.push(RowTransformation::new(
            "quality_check",
            vec!["email".to_string(), "age".to_string(), "name".to_string()],
        ));

        // Step 3: Enrichment (add derived fields)
        transformations.push(RowTransformation::new(
            "enrichment",
            vec!["email_domain".to_string()],
        ));

        // Step 4: Load to DB2 (simulated)
        let mut pk = BTreeMap::new();
        pk.insert("customer_id".to_string(), row.customer_id.clone());
        let dest_row_id = RowId::database(DatabaseType::DB2, "customers".to_string(), pk);

        transformations.push(RowTransformation::new(
            "database_write",
            vec![
                "customer_id".to_string(),
                "name".to_string(),
                "email".to_string(),
            ],
        ));

        // Record successful processing event
        let mut event = RowLineageEvent::success(
            source_row_id,
            self.batch_id.clone(),
            self.job_id.clone(),
            "db2://prod/customers".to_string(),
            self.tenant_id.clone(),
        );
        event.transformations = transformations;
        event.output_row_id = Some(dest_row_id);

        self.lineage_store
            .write_row(event)
            .await
            .map_err(|e| format!("Failed to record lineage: {}", e))?;

        Ok(())
    }
}

#[derive(Debug, Default)]
struct ProcessingResult {
    processed: u64,
    filtered: u64,
    errors: u64,
    filtered_reasons: Vec<(u64, String)>,
}

/// Test 1: Basic ETL flow with lineage tracking
#[tokio::test]
async fn test_complete_etl_flow_with_lineage() -> Result<()> {
    // Setup
    let temp_dir = TempDir::new()?;
    let csv_path = temp_dir.path().join("customers.csv");
    std::fs::write(&csv_path, TEST_CSV)?;

    let lineage_store = Arc::new(MockRowLineageStore::new());
    let pipeline = EtlPipeline::new(
        lineage_store.clone() as Arc<dyn RowLevelLineageSink>,
        "job-001".to_string(),
        "batch-001".to_string(),
        "tenant-test".to_string(),
    );

    // Execute ETL
    let result = pipeline.process_csv(csv_path.to_str().unwrap()).await?;

    // Validate processing counts
    println!("Processing results:");
    println!("  Processed: {}", result.processed);
    println!("  Filtered: {}", result.filtered);
    println!("  Errors: {}", result.errors);

    assert_eq!(result.processed, 5, "Expected 5 valid rows to be processed");
    assert_eq!(result.filtered, 5, "Expected 5 rows to be filtered");

    // Validate lineage was captured
    let all_events = lineage_store.get_all_events().await;
    assert_eq!(
        all_events.len(),
        10,
        "Expected 10 lineage events (5 processed + 5 filtered)"
    );

    // Validate job statistics
    let stats = lineage_store.get_job_stats("job-001").await?;
    println!("\nJob statistics:");
    println!("  Total rows: {}", stats.total_rows);
    println!("  Processed: {}", stats.success_count);
    println!("  Filtered: {}", stats.filtered_count);
    println!("  Failed: {}", stats.failed_count);

    assert_eq!(stats.total_rows, 10);
    assert_eq!(stats.success_count, 5);
    assert_eq!(stats.filtered_count, 5);
    assert_eq!(stats.failed_count, 0);

    Ok(())
}

/// Test 2: Validate filtered rows with reasons
#[tokio::test]
async fn test_filtered_rows_tracking() -> Result<()> {
    // Setup
    let temp_dir = TempDir::new()?;
    let csv_path = temp_dir.path().join("customers.csv");
    std::fs::write(&csv_path, TEST_CSV)?;

    let lineage_store = Arc::new(MockRowLineageStore::new());
    let pipeline = EtlPipeline::new(
        lineage_store.clone() as Arc<dyn RowLevelLineageSink>,
        "job-002".to_string(),
        "batch-002".to_string(),
        "tenant-test".to_string(),
    );

    // Execute ETL
    let _result = pipeline.process_csv(csv_path.to_str().unwrap()).await?;

    // Query filtered rows with broad time range
    let start_time = Utc::now() - chrono::Duration::hours(1);
    let end_time = Utc::now() + chrono::Duration::hours(1);
    let filtered = lineage_store
        .get_filtered_rows("job-002", start_time, end_time)
        .await?;

    println!("\nFiltered rows:");
    for (row_id, reason) in &filtered {
        println!("  Row {}: {}", row_id.to_key(), reason);
    }

    assert_eq!(filtered.len(), 5, "Expected 5 filtered rows");

    // Validate specific rejection reasons
    let reasons: Vec<String> = filtered.iter().map(|(_, r)| r.clone()).collect();
    assert!(reasons.contains(&"Missing required field: email".to_string()));
    assert!(reasons.contains(&"Invalid email format".to_string()));
    assert!(reasons.contains(&"Invalid age: negative value".to_string()));
    assert!(reasons.contains(&"Missing last name".to_string()));
    assert!(reasons.contains(&"Invalid age: non-numeric".to_string()));

    Ok(())
}

/// Test 3: Validate row journey (end-to-end lineage)
#[tokio::test]
async fn test_row_journey_tracking() -> Result<()> {
    // Setup
    let temp_dir = TempDir::new()?;
    let csv_path = temp_dir.path().join("customers.csv");
    std::fs::write(&csv_path, TEST_CSV)?;

    let lineage_store = Arc::new(MockRowLineageStore::new());
    let pipeline = EtlPipeline::new(
        lineage_store.clone() as Arc<dyn RowLevelLineageSink>,
        "job-003".to_string(),
        "batch-003".to_string(),
        "tenant-test".to_string(),
    );

    // Execute ETL
    let _result = pipeline.process_csv(csv_path.to_str().unwrap()).await?;

    // Trace journey for a successfully processed row (row 2: John Doe)
    let source_row_id = RowId::csv(csv_path.to_str().unwrap().to_string(), 2);
    let journey = lineage_store.trace_row_journey(&source_row_id).await?;

    println!("\nRow journey for customer 1001 (John Doe):");
    println!("  Total steps: {}", journey.steps.len());
    for (i, step) in journey.steps.iter().enumerate() {
        println!(
            "  Step {}: {} at {:?}",
            i + 1,
            step.activity,
            step.timestamp
        );
    }

    // Validate journey has all transformation steps
    assert_eq!(journey.steps.len(), 4, "Expected 4 transformation steps");

    let step_names: Vec<String> = journey.steps.iter().map(|s| s.activity.clone()).collect();
    assert!(step_names.contains(&"standardization".to_string()));
    assert!(step_names.contains(&"quality_check".to_string()));
    assert!(step_names.contains(&"enrichment".to_string()));
    assert!(step_names.contains(&"database_write".to_string()));

    // Validate destination was set
    assert!(
        journey.destination.is_some(),
        "Expected destination to be set"
    );

    Ok(())
}

/// Test 4: Validate batch lineage
#[tokio::test]
async fn test_batch_lineage_tracking() -> Result<()> {
    // Setup
    let temp_dir = TempDir::new()?;
    let csv_path = temp_dir.path().join("customers.csv");
    std::fs::write(&csv_path, TEST_CSV)?;

    let lineage_store = Arc::new(MockRowLineageStore::new());
    let pipeline = EtlPipeline::new(
        lineage_store.clone() as Arc<dyn RowLevelLineageSink>,
        "job-004".to_string(),
        "batch-004".to_string(),
        "tenant-test".to_string(),
    );

    // Execute ETL
    let _result = pipeline.process_csv(csv_path.to_str().unwrap()).await?;

    // Query batch lineage
    let batch_events = lineage_store.get_batch_lineage("batch-004").await?;

    println!("\nBatch lineage for batch-004:");
    println!("  Total events: {}", batch_events.len());

    assert_eq!(batch_events.len(), 10, "Expected 10 events in batch");

    // Validate all events belong to the same batch
    for event in &batch_events {
        assert_eq!(event.batch_id, "batch-004");
        assert_eq!(event.job_id, "job-004");
    }

    Ok(())
}

/// Test 5: Validate transformation metadata
#[tokio::test]
async fn test_transformation_metadata() -> Result<()> {
    // Setup
    let temp_dir = TempDir::new()?;
    let csv_path = temp_dir.path().join("customers.csv");
    std::fs::write(&csv_path, TEST_CSV)?;

    let lineage_store = Arc::new(MockRowLineageStore::new());
    let pipeline = EtlPipeline::new(
        lineage_store.clone() as Arc<dyn RowLevelLineageSink>,
        "job-005".to_string(),
        "batch-005".to_string(),
        "tenant-test".to_string(),
    );

    // Execute ETL
    let _result = pipeline.process_csv(csv_path.to_str().unwrap()).await?;

    // Get events for a processed row
    let source_row_id = RowId::csv(csv_path.to_str().unwrap().to_string(), 2);
    let events = lineage_store.get_row_lineage(&source_row_id).await?;

    assert_eq!(events.len(), 1, "Expected 1 event for this row");

    let event = &events[0];
    println!("\nTransformations for row 2:");
    for (i, transform) in event.transformations.iter().enumerate() {
        println!(
            "  {}: {} (fields: {:?})",
            i + 1,
            transform.transform_type,
            transform.fields
        );
    }

    // Validate transformation sequence
    assert_eq!(event.transformations.len(), 4);
    assert_eq!(event.transformations[0].transform_type, "standardization");
    assert_eq!(event.transformations[1].transform_type, "quality_check");
    assert_eq!(event.transformations[2].transform_type, "enrichment");
    assert_eq!(event.transformations[3].transform_type, "database_write");

    // Validate output row ID was set
    assert!(
        event.output_row_id.is_some(),
        "Expected output_row_id to be set"
    );

    Ok(())
}
