//! Pilot Streaming Workflow Demo
//!
//! This example demonstrates a complete streaming workflow for real-time data processing.
//! It showcases:
//! - Creating a streaming workflow with multiple routes
//! - Kafka source configuration
//! - Real-time event processing
//! - Graceful shutdown
//!
//! ## Use Case: Real-Time Customer Event Processing
//!
//! This workflow processes customer events from Kafka and applies different
//! transformations based on event type:
//! - High-value transactions: Enrichment + validation + alerting
//! - Standard transactions: Basic validation + logging
//! - Profile updates: Validation + database update
//!
//! ## Running the Demo
//!
//! ```bash
//! # Start Kafka (Docker)
//! docker run -d --name kafka \
//!   -p 9092:9092 \
//!   -e KAFKA_ZOOKEEPER_CONNECT=zookeeper:2181 \
//!   wurstmeister/kafka
//!
//! # Run the demo
//! cargo run --example streaming_workflow_demo
//! ```

use graphica_coordinator::workflows::domain::{
    Action, AutoScalingConfig, Condition, ExecutionMode, Route, StateBackendConfig,
    StreamingConfig, WatermarkStrategy, Workflow,
};
use graphica_coordinator::workflows::engine::StreamExecutor;
use graphica_coordinator::workflows::storage::{ExecutionStore, WorkflowStore};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tracing::{info, Level};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();

    info!("🚀 Starting Graphica Streaming Workflow Demo");

    // Create workflow stores
    let workflow_store = Arc::new(WorkflowStore::new());
    let execution_store = Arc::new(ExecutionStore::new());

    // Create the pilot streaming workflow
    let workflow = create_pilot_streaming_workflow();

    // Validate workflow
    info!("✅ Validating workflow configuration...");
    workflow.validate()?;
    info!("✅ Workflow validation passed!");

    // Display workflow configuration
    display_workflow_info(&workflow);

    // Store workflow
    workflow_store.create(workflow.clone())?;
    info!("💾 Workflow stored successfully");

    // Create StreamExecutor
    let executor = StreamExecutor::new(workflow_store.clone(), execution_store.clone());
    info!("🎛️  StreamExecutor initialized");

    // Start streaming execution
    info!("🌊 Starting streaming execution...");
    info!("📡 Consuming from Kafka topic: customer_events");
    info!("👥 Consumer group: customer_events_processor");

    match executor.start_stream(&workflow).await {
        Ok(handle) => {
            info!("✅ Streaming execution started successfully!");
            info!("   Workflow ID: {}", handle.workflow_id);
            info!("   Workers: {}", handle.workers);
            info!("   Consumer Group: {}", handle.consumer_group);

            // In production, this would run indefinitely
            // For demo purposes, run for 30 seconds then shutdown
            info!("⏰ Demo will run for 30 seconds...");
            tokio::time::sleep(Duration::from_secs(30)).await;

            // Graceful shutdown
            info!("🛑 Initiating graceful shutdown...");
            executor.stop_stream(&workflow.id).await?;
            info!("✅ Streaming execution stopped gracefully");
        }
        Err(e) => {
            // If Kafka is not available, show the workflow configuration
            info!("⚠️  Could not start streaming (Kafka may not be available)");
            info!("   Error: {}", e);
            info!("📋 Workflow is properly configured and ready for Kafka");
            info!("   To run with real Kafka, ensure broker is available at localhost:9092");
        }
    }

    info!("✅ Demo complete!");

    Ok(())
}

/// Create the pilot streaming workflow
fn create_pilot_streaming_workflow() -> Workflow {
    // Route 1: High-value transactions (priority 100)
    let high_value_route = Route::with_priority(
        "high_value_transactions",
        "High-Value Transaction Processing",
        Condition::And(Box::new(vec![
            Condition::Equals {
                field: "event_type".to_string(),
                value: json!("transaction"),
            },
            Condition::GreaterThanOrEqual {
                field: "amount".to_string(),
                value: json!(10000),
            },
        ])),
        vec![
            Action::Log {
                level: "info".to_string(),
                message: "Processing high-value transaction".to_string(),
            },
            Action::Transform {
                transformer: "enrich_customer_data".to_string(),
                config: json!({
                    "source": "customer_db",
                    "fields": ["vip_status", "risk_score"]
                }),
            },
            Action::Validate {
                rule_id: "high_value_fraud_check".to_string(),
            },
            Action::Transform {
                transformer: "send_alert".to_string(),
                config: json!({
                    "webhook_url": "https://alerts.example.com/high-value",
                    "method": "POST"
                }),
            },
        ],
        100,
    );

    // Route 2: Standard transactions (priority 50)
    let standard_transaction_route = Route::with_priority(
        "standard_transactions",
        "Standard Transaction Processing",
        Condition::Equals {
            field: "event_type".to_string(),
            value: json!("transaction"),
        },
        vec![
            Action::Log {
                level: "info".to_string(),
                message: "Processing standard transaction".to_string(),
            },
            Action::Validate {
                rule_id: "standard_transaction_validation".to_string(),
            },
            Action::Transform {
                transformer: "normalize_transaction".to_string(),
                config: json!({
                    "currency": "USD",
                    "round_to": 2
                }),
            },
        ],
        50,
    );

    // Route 3: Profile updates (priority 30)
    let profile_update_route = Route::with_priority(
        "profile_updates",
        "Customer Profile Updates",
        Condition::Equals {
            field: "event_type".to_string(),
            value: json!("profile_update"),
        },
        vec![
            Action::Log {
                level: "info".to_string(),
                message: "Processing profile update".to_string(),
            },
            Action::Validate {
                rule_id: "profile_schema_validation".to_string(),
            },
            Action::Transform {
                transformer: "persist_to_database".to_string(),
                config: json!({
                    "connection": "postgresql://localhost:5432/graphica",
                    "table": "customer_profiles",
                    "mode": "upsert"
                }),
            },
        ],
        30,
    );

    // Route 4: Default/catch-all (priority 1)
    let default_route = Route::with_priority(
        "default_route",
        "Default Event Handler",
        Condition::Always,
        vec![Action::Log {
            level: "warn".to_string(),
            message: "Unhandled event type".to_string(),
        }],
        1,
    );

    // Kafka configuration
    let mut kafka_properties = HashMap::new();
    kafka_properties.insert(
        "bootstrap.servers".to_string(),
        "localhost:9092".to_string(),
    );
    kafka_properties.insert("session.timeout.ms".to_string(), "30000".to_string());
    kafka_properties.insert("heartbeat.interval.ms".to_string(), "10000".to_string());

    // Streaming configuration with auto-scaling
    let streaming_config = StreamingConfig {
        source_topic: "customer_events".to_string(),
        consumer_group: "customer_events_processor".to_string(),
        checkpoint_interval_ms: 60000, // 60 seconds
        watermark_strategy: WatermarkStrategy::BoundedOutOfOrderness {
            max_out_of_orderness_ms: 30000, // 30 seconds tolerance
        },
        max_parallel_workers: Some(4), // Scale to 4 workers
        state_backend: StateBackendConfig::RocksDB {
            path: "/tmp/graphica/streaming_state".to_string(),
            incremental_checkpoints: true,
        },
        auto_scaling: Some(AutoScalingConfig {
            min_workers: 2,
            max_workers: 10,
            target_lag: 50000,       // Scale up if lag > 50K messages
            target_latency_ms: 2000, // Scale up if latency > 2s
            scaleup_cooldown_secs: 120,
            scaledown_cooldown_secs: 600,
        }),
        kafka_properties,
    };

    // Create workflow with streaming execution mode
    let mut workflow = Workflow::new(
        "pilot_streaming_workflow",
        "Pilot Streaming Workflow - Customer Events",
        vec![
            high_value_route,
            standard_transaction_route,
            profile_update_route,
            default_route,
        ],
    );

    workflow.execution_mode = ExecutionMode::Streaming {
        config: streaming_config,
    };

    workflow
        .with_description(
            "Real-time customer event processing with intelligent routing and auto-scaling",
        )
        .with_tags(vec![
            "streaming".to_string(),
            "real-time".to_string(),
            "customer-events".to_string(),
            "production".to_string(),
        ])
        .with_default_route("default_route")
}

/// Display workflow information
fn display_workflow_info(workflow: &Workflow) {
    info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    info!("📊 Workflow Configuration");
    info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    info!("   ID: {}", workflow.id);
    info!("   Name: {}", workflow.name);
    info!("   Description: {}", workflow.description);
    info!("   Version: {}", workflow.version);
    info!("   Routes: {}", workflow.routes.len());
    info!("   Tags: {:?}", workflow.tags);

    match &workflow.execution_mode {
        ExecutionMode::Streaming { config } => {
            info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
            info!("🌊 Streaming Configuration");
            info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
            info!("   Source Topic: {}", config.source_topic);
            info!("   Consumer Group: {}", config.consumer_group);
            info!(
                "   Checkpoint Interval: {}ms",
                config.checkpoint_interval_ms
            );
            info!("   Max Workers: {:?}", config.max_parallel_workers);

            if let Some(ref auto_scaling) = config.auto_scaling {
                info!("   Auto-Scaling: Enabled");
                info!("     Min Workers: {}", auto_scaling.min_workers);
                info!("     Max Workers: {}", auto_scaling.max_workers);
                info!("     Target Lag: {} messages", auto_scaling.target_lag);
                info!("     Target Latency: {}ms", auto_scaling.target_latency_ms);
            } else {
                info!("   Auto-Scaling: Disabled");
            }

            match &config.state_backend {
                StateBackendConfig::RocksDB {
                    path,
                    incremental_checkpoints,
                } => {
                    info!("   State Backend: RocksDB");
                    info!("     Path: {}", path);
                    info!("     Incremental Checkpoints: {}", incremental_checkpoints);
                }
                StateBackendConfig::Memory => {
                    info!("   State Backend: In-Memory");
                }
            }

            // Resource estimation
            let resources = workflow.execution_mode.estimate_resources();
            info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
            info!("💻 Resource Estimation");
            info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
            info!("   CPU Cores: {}", resources.cpu_cores);
            info!("   Memory: {} MB", resources.memory_mb);
            info!("   Storage: {} MB", resources.storage_mb);
            info!("   Network: {} Mbps", resources.network_mbps);
        }
        _ => {}
    }

    info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    info!("📋 Routes (by priority)");
    info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    for route in workflow.routes_by_priority() {
        info!("   {} (priority: {})", route.name, route.priority);
        info!("     ID: {}", route.id);
        info!("     Actions: {}", route.actions.len());
    }
    info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
}
