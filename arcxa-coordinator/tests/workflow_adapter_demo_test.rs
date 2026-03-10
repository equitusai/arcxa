//! Demonstration of Workflow Adapter Pattern
//!
//! This test shows how the WorkflowCsvAdapter can be used to apply workflow
//! filtering to CSV files WITHOUT modifying LoaderWorker. This maintains
//! clean separation of concerns while allowing the components to work together.

use anyhow::Result;
use graphica_coordinator::mapping::loader::orchestration::workflow_adapter::WorkflowCsvAdapter;
use graphica_coordinator::workflows::domain::{Action, Condition, Route, Workflow};
use serde_json::json;
use std::io::Write;
use tempfile::{NamedTempFile, TempDir};

/// Create a workflow that filters customers by status
fn create_customer_filter_workflow() -> Workflow {
    let active_route = Route::new(
        "process_active",
        "Process Active Customers",
        Condition::and(vec![
            // Must be from customers table
            Condition::Equals {
                field: "source_table".to_string(),
                value: json!("customers"),
            },
            // Must have active status
            Condition::Equals {
                field: "status".to_string(),
                value: json!("active"),
            },
            // Must have valid email (contains @)
            Condition::Contains {
                field: "email".to_string(),
                substring: "@".to_string(),
            },
        ]),
        vec![
            Action::Log {
                level: "info".to_string(),
                message: "Processing active customer".to_string(),
            },
            Action::RecordLineage {
                event_type: "customer_filter".to_string(),
                metadata: json!({"workflow": "customer_filtering"}),
            },
        ],
    );

    Workflow::new(
        "customer_filter_workflow",
        "Customer Filtering Workflow",
        vec![active_route],
    )
}

/// Create test CSV with mixed customer data
fn create_customer_csv() -> Result<NamedTempFile> {
    let mut file = NamedTempFile::new()?;
    writeln!(file, "customer_id,name,email,status,registration_date")?;
    writeln!(file, "1001,Alice Smith,alice@example.com,active,2024-01-15")?; // ✅ Will pass
    writeln!(file, "1002,Bob Jones,bob@example.com,inactive,2024-02-20")?; // ❌ Inactive
    writeln!(
        file,
        "1003,Charlie Brown,charlie@example.com,active,2024-03-10"
    )?; // ✅ Will pass
    writeln!(file, "1004,David Wilson,david@invalid,active,2024-04-05")?; // ✅ Has @ so passes
    writeln!(file, "1005,Eve Davis,eve@example.com,deleted,2024-05-12")?; // ❌ Deleted
    writeln!(
        file,
        "1006,Frank Miller,frank@example.com,active,2024-06-01"
    )?; // ✅ Will pass
    file.flush()?;
    Ok(file)
}

#[test]
fn test_workflow_adapter_filtering() -> Result<()> {
    // Step 1: Create workflow
    let workflow = create_customer_filter_workflow();
    workflow.validate()?;

    println!("✅ Created customer filtering workflow");

    // Step 2: Create adapter
    let adapter = WorkflowCsvAdapter::new(workflow);

    println!("✅ Created workflow adapter");

    // Step 3: Test row-by-row filtering
    let headers = vec![
        "customer_id".to_string(),
        "name".to_string(),
        "email".to_string(),
        "status".to_string(),
        "registration_date".to_string(),
    ];

    // Test active customer with valid email - should pass
    let row1 = vec![
        "1001".to_string(),
        "Alice".to_string(),
        "alice@example.com".to_string(),
        "active".to_string(),
        "2024-01-15".to_string(),
    ];
    assert!(adapter.should_process_row(&headers, &row1, "customers")?);
    println!("  ✓ Active customer with valid email: PASS");

    // Test inactive customer - should be filtered
    let row2 = vec![
        "1002".to_string(),
        "Bob".to_string(),
        "bob@example.com".to_string(),
        "inactive".to_string(),
        "2024-02-20".to_string(),
    ];
    assert!(!adapter.should_process_row(&headers, &row2, "customers")?);
    println!("  ✓ Inactive customer: FILTERED");

    // Test active customer with email containing @ - should PASS (our filter only checks for @)
    let row3 = vec![
        "1004".to_string(),
        "David".to_string(),
        "david@invalid".to_string(), // Has @ so passes our simple filter
        "active".to_string(),
        "2024-04-05".to_string(),
    ];
    assert!(adapter.should_process_row(&headers, &row3, "customers")?);
    println!("  ✓ Active customer with @ in email: PASS (simple filter)");

    // Test deleted customer - should be filtered
    let row4 = vec![
        "1005".to_string(),
        "Eve".to_string(),
        "eve@example.com".to_string(),
        "deleted".to_string(), // Not active status
        "2024-05-12".to_string(),
    ];
    assert!(!adapter.should_process_row(&headers, &row4, "customers")?);
    println!("  ✓ Deleted customer: FILTERED");

    Ok(())
}

#[tokio::test]
async fn test_workflow_adapter_file_filtering() -> Result<()> {
    println!("\n╔══════════════════════════════════════════════════════╗");
    println!("║  Workflow Adapter File Filtering Demo                 ║");
    println!("╚══════════════════════════════════════════════════════╝\n");

    // Step 1: Create test data
    let input_csv = create_customer_csv()?;
    let output_csv = NamedTempFile::new()?;

    println!("Step 1: Created test CSV with 6 customer records");
    println!("  - 4 active with @ in email (should pass)");
    println!("  - 1 inactive (should be filtered)");
    println!("  - 1 deleted (should be filtered)\n");

    // Step 2: Create workflow and adapter
    let workflow = create_customer_filter_workflow();
    let adapter = WorkflowCsvAdapter::new(workflow);

    println!("Step 2: Created filtering workflow");
    println!("  Conditions:");
    println!("  - source_table == 'customers'");
    println!("  - status == 'active'");
    println!("  - email contains '@'\n");

    // Step 3: Filter the CSV file
    println!("Step 3: Applying workflow filter to CSV file...");
    let result = adapter
        .filter_csv_file(input_csv.path(), output_csv.path(), "customers")
        .await?;

    println!("\n📊 Filtering Results:");
    println!("  Total rows processed: {}", result.total_rows);
    println!("  Rows written:         {} ✅", result.written_rows);
    println!("  Rows filtered:        {} ❌", result.filtered_rows);
    println!("  Filter rate:          {:.1}%", result.filter_rate());

    if !result.filter_reasons.is_empty() {
        println!("\n  Filter reasons:");
        for (reason, count) in result.filter_reasons.iter() {
            println!("    - {}: {}", reason, count);
        }
    }

    // Step 4: Verify the filtered file
    let mut reader = csv::Reader::from_path(output_csv.path())?;
    let mut valid_count = 0;

    println!("\n📝 Verifying filtered output:");
    for record in reader.records() {
        let record = record?;
        assert_eq!(&record[3], "active", "All records should be active");
        assert!(record[2].contains('@'), "All emails should be valid");
        valid_count += 1;
        println!(
            "  ✓ Customer {} - {} [{}]",
            &record[0], &record[1], &record[2]
        );
    }

    assert_eq!(valid_count, 4, "Should have exactly 4 valid customers");

    println!("\n✅ File filtering successful!");
    println!("   Filtered file ready for LoaderWorker processing");

    Ok(())
}

#[tokio::test]
async fn test_workflow_adapter_with_loader_simulation() -> Result<()> {
    println!("\n╔══════════════════════════════════════════════════════╗");
    println!("║  Complete Workflow → LoaderWorker Pipeline Demo       ║");
    println!("╚══════════════════════════════════════════════════════╝\n");

    let temp_dir = TempDir::new()?;

    // Phase 1: Create and filter CSV
    println!("🔹 Phase 1: Pre-processing with Workflow Adapter");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    let original_csv = create_customer_csv()?;
    let filtered_csv_path = temp_dir.path().join("filtered_customers.csv");

    let workflow = create_customer_filter_workflow();
    let adapter = WorkflowCsvAdapter::new(workflow);

    let filter_result = adapter
        .filter_csv_file(original_csv.path(), &filtered_csv_path, "customers")
        .await?;

    println!("  Original records: {}", filter_result.total_rows);
    println!(
        "  After filtering:  {} (removed {} invalid records)\n",
        filter_result.written_rows, filter_result.filtered_rows
    );

    // Phase 2: Pass filtered file to LoaderWorker
    println!("🔹 Phase 2: LoaderWorker Processing (Simulated)");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    // This is where LoaderWorker would process the filtered file
    // LoaderWorker remains UNCHANGED - it just processes the filtered CSV

    println!("  LoaderWorker configuration:");
    println!("  - Source: filtered_customers.csv");
    println!("  - Target: DB2.CUSTOMERS table");
    println!("  - Mode: INSERT");
    println!("  - Batch size: 1000\n");

    // Simulate LoaderWorker processing
    println!("  [LoaderWorker] Starting job...");
    println!("  [LoaderWorker] Reading filtered_customers.csv");
    println!(
        "  [LoaderWorker] Processing {} rows",
        filter_result.written_rows
    );
    println!("  [LoaderWorker] Batch 1: Loading rows to DB2...");
    println!("  [LoaderWorker] ✅ Job completed successfully\n");

    // Phase 3: Results
    println!("🔹 Phase 3: Pipeline Results");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    println!(
        "  ✅ Workflow filtering: {} invalid records removed",
        filter_result.filtered_rows
    );
    println!(
        "  ✅ LoaderWorker: {} valid records loaded to DB2",
        filter_result.written_rows
    );
    println!("  ✅ Total pipeline time: ~2.5 seconds (simulated)\n");

    println!("📌 Key Insight: LoaderWorker didn't need ANY modifications!");
    println!("   The WorkflowAdapter pre-processed the data, keeping concerns separated.");

    Ok(())
}

#[test]
fn test_workflow_validation_for_csv_filtering() -> Result<()> {
    println!("\n🔍 Testing Workflow Validation for CSV Context\n");

    // Create a complex workflow with multiple routes
    let customer_route = Route::with_priority(
        "customers",
        "Customer Processing",
        Condition::and(vec![
            Condition::Equals {
                field: "source_table".to_string(),
                value: json!("customers"),
            },
            Condition::or(vec![
                Condition::Equals {
                    field: "status".to_string(),
                    value: json!("active"),
                },
                Condition::Equals {
                    field: "status".to_string(),
                    value: json!("pending"),
                },
            ]),
        ]),
        vec![
            Action::Transform {
                transformer: "normalize_customer".to_string(),
                config: json!({}),
            },
            Action::Validate {
                rule_id: "customer_completeness".to_string(),
            },
        ],
        100,
    );

    let order_route = Route::with_priority(
        "orders",
        "Order Processing",
        Condition::and(vec![
            Condition::Equals {
                field: "source_table".to_string(),
                value: json!("orders"),
            },
            Condition::GreaterThan {
                field: "amount".to_string(),
                value: json!(100.0),
            },
        ]),
        vec![
            Action::Transform {
                transformer: "enrich_order".to_string(),
                config: json!({}),
            },
            Action::SendToKafka {
                topic: "high_value_orders".to_string(),
                partition_key: Some("customer_id".to_string()),
            },
        ],
        90,
    );

    let workflow = Workflow::new(
        "multi_table_workflow",
        "Multi-Table Processing",
        vec![customer_route, order_route],
    );

    // Validate the workflow
    workflow.validate()?;

    println!("✅ Complex workflow validated successfully");
    println!("   Routes: {}", workflow.routes.len());
    for route in workflow.routes.iter() {
        println!("   - {} (priority: {})", route.name, route.priority);
    }

    Ok(())
}
