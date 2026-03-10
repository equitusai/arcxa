//! # Impact Analysis Engine
//!
//! The KEY DIFFERENTIATOR feature that makes Graphica crush competitors.
//! Enables forward impact simulation, backward root-cause analysis, and change safety checks.

use super::{DataRef, LineageEvent, LineageGraph, LineageSink, ModelRef};
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// Impact analyzer for forward/backward lineage queries
pub struct ImpactAnalyzer {
    storage: Arc<dyn LineageSink>,
}

impl ImpactAnalyzer {
    pub fn new(storage: Arc<dyn LineageSink>) -> Self {
        Self { storage }
    }

    /// Forward impact analysis: What will be affected if this changes?
    /// Returns all downstream records, models, and datasets that depend on this source
    pub async fn forward_impact(
        &self,
        source: &DataRef,
        as_of: Option<DateTime<Utc>>,
    ) -> Result<ImpactReport> {
        let as_of_ts = as_of.unwrap_or_else(Utc::now);

        // Query all lineage events that reference this data source
        let all_events = self.storage.query_by_time_range(
            as_of_ts - chrono::Duration::days(365), // Look back 1 year
            as_of_ts,
        )?;

        // Build lineage graph
        let graph = LineageGraph::from_events(all_events);

        // Find all events that reference this source
        let mut affected_records = HashSet::new();
        let mut affected_models = Vec::new();
        let mut affected_datasets = HashSet::new();
        let mut can_replay = false;

        for event in graph.events() {
            // Check if this event uses the source
            let uses_source = event
                .source_refs
                .iter()
                .any(|s| s.system == source.system && s.path == source.path);

            if uses_source {
                affected_records.insert(event.record_id.clone());
                affected_datasets.insert(event.dataset.clone());

                // Check if any models were trained on this data
                for model_ref in &event.model_refs {
                    affected_models.push(model_ref.clone());
                }

                // Check if we can replay (has CDC position)
                if event.source_refs.iter().any(|s| s.cdc_position.is_some()) {
                    can_replay = true;
                }
            }
        }

        // Calculate risk level
        let risk_level = if affected_models.len() > 5 {
            RiskLevel::Critical
        } else if affected_models.len() > 1 {
            RiskLevel::High
        } else if affected_records.len() > 100 {
            RiskLevel::Medium
        } else {
            RiskLevel::Low
        };

        Ok(ImpactReport {
            affected_records: affected_records.into_iter().collect(),
            affected_models,
            affected_datasets: affected_datasets.into_iter().collect(),
            risk_level,
            can_replay,
            analysis_timestamp: Utc::now(),
        })
    }

    /// Backward root-cause analysis: What sources caused this output?
    /// Traces upstream to find exact source records, CDC positions, and transformation versions
    pub async fn root_cause_analysis(
        &self,
        record_id: &str,
        as_of: Option<DateTime<Utc>>,
    ) -> Result<RootCauseReport> {
        let events = if let Some(ts) = as_of {
            self.storage.get_lineage_as_of(record_id, ts)?
        } else {
            self.storage.get_record_lineage(record_id)?
        };

        if events.is_empty() {
            return Ok(RootCauseReport {
                record_id: record_id.to_string(),
                source_data_refs: vec![],
                transformation_chain: vec![],
                models_applied: vec![],
                quality_issues: vec![],
                can_replay: false,
                replay_commands: vec![],
            });
        }

        // Build graph for recursive traversal
        let graph = LineageGraph::from_events(events);

        // Get all upstream sources recursively
        let upstream_sources = graph.upstream_recursive(record_id, 10); // max 10 hops

        // Extract transformation chain
        let mut transformation_chain = Vec::new();
        let mut models_applied = Vec::new();
        let mut cdc_positions = HashMap::new();

        for event in graph.events() {
            if event.record_id == record_id {
                // Collect transforms
                for transform in &event.transforms {
                    transformation_chain.push(TransformationStep {
                        transform_type: transform.transform_type.clone(),
                        rule_id: transform.rule_id.clone(),
                        version: transform.version.clone(),
                        applied_at: transform.applied_at,
                    });
                }

                // Collect models
                models_applied.extend(event.model_refs.clone());

                // Collect CDC positions for replay
                for source_ref in &event.source_refs {
                    if let Some(cdc_pos) = &source_ref.cdc_position {
                        let key = format!("{}:{}", source_ref.system, source_ref.path);
                        cdc_positions.insert(key, cdc_pos.clone());
                    }
                }
            }
        }

        // Generate replay commands
        let replay_commands = cdc_positions
            .iter()
            .map(|(source, pos)| {
                format!(
                    "kafka-console-consumer --bootstrap-server localhost:9092 --topic {} --partition {} --offset {} --max-messages 1",
                    pos.topic, pos.partition, pos.offset
                )
            })
            .collect();

        // Query quality violations for this record
        // Note: Quality violations are stored in RDF (gph:QualityViolation) but we only have
        // access to LineageSink here. To properly query violations, we would need either:
        // 1. Add a query_violations() method to LineageSink trait, or
        // 2. Pass RDF store reference to ImpactAnalyzer
        // For now, we extract quality issues from lineage event metadata if available
        let quality_issues: Vec<QualityIssue> = graph
            .events()
            .iter()
            .filter(|e| e.record_id == record_id)
            .flat_map(|event| {
                // Check if quality violations are embedded in event metadata
                // In practice, violations should be queried from RDF store using SPARQL:
                // SELECT ?ruleId ?severity ?message ?detectedAt
                // WHERE {
                //   ?violation a gph:QualityViolation ;
                //              gph:affectedRecord <record_id> ;
                //              gph:ruleId ?ruleId ;
                //              gph:severity ?severity ;
                //              gph:message ?message ;
                //              prov:generatedAtTime ?detectedAt .
                // }

                // Placeholder: Extract from transform metadata if present
                event
                    .transforms
                    .iter()
                    .filter_map(|t| {
                        if t.rule_id.starts_with("quality_check_") {
                            Some(QualityIssue {
                                rule_id: t.rule_id.clone(),
                                severity: "Warning".to_string(),
                                message: format!("Quality rule {} applied", t.rule_id),
                                detected_at: event.ts,
                            })
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>()
            })
            .collect();

        Ok(RootCauseReport {
            record_id: record_id.to_string(),
            source_data_refs: upstream_sources.into_iter().collect(),
            transformation_chain,
            models_applied,
            quality_issues,
            can_replay: !cdc_positions.is_empty(),
            replay_commands,
        })
    }

    /// Simulate change impact: What would break if I modify this?
    /// Checks compatibility and identifies breaking changes
    pub async fn simulate_change(&self, change: &ProposedChange) -> Result<SimulationReport> {
        match &change.change_type {
            ChangeType::SchemaModification => self.simulate_schema_change(change).await,
            ChangeType::TransformationUpdate => self.simulate_transformation_change(change).await,
            ChangeType::DataSourceRemoval => self.simulate_source_removal(change).await,
        }
    }

    async fn simulate_schema_change(&self, change: &ProposedChange) -> Result<SimulationReport> {
        // Find all downstream consumers of this source
        let source = DataRef {
            system: change.target.clone(),
            path: change
                .details
                .get("table")
                .unwrap_or(&"".to_string())
                .clone(),
            version: None,
            extracted_at: Utc::now(),
            cdc_position: None,
        };

        let impact = self.forward_impact(&source, None).await?;

        // Check for breaking changes
        let breaking_changes = if change.details.contains_key("remove_column") {
            vec!["Column removal may break downstream consumers".to_string()]
        } else if change.details.contains_key("change_type") {
            vec!["Type change may cause parsing errors".to_string()]
        } else {
            vec![]
        };

        let recommendation = if !breaking_changes.is_empty() {
            "Review breaking changes before proceeding".to_string()
        } else {
            "Safe to proceed".to_string()
        };

        Ok(SimulationReport {
            change: change.clone(),
            affected_records_count: impact.affected_records.len(),
            affected_models_count: impact.affected_models.len(),
            affected_datasets: impact.affected_datasets,
            breaking_changes,
            warnings: vec![],
            recommendation,
        })
    }

    async fn simulate_transformation_change(
        &self,
        change: &ProposedChange,
    ) -> Result<SimulationReport> {
        // Extract transformation details
        let transform_name = change
            .details
            .get("transform_name")
            .unwrap_or(&change.target)
            .clone();

        let transform_version = change.details.get("new_version").map(|v| v.to_string());

        // Query lineage events that use this transformation
        let all_events = self.storage.query_by_time_range(
            Utc::now() - chrono::Duration::days(365), // Look back 1 year
            Utc::now(),
        )?;

        let graph = LineageGraph::from_events(all_events);

        // Find all records that use this transformation
        let mut affected_records = HashSet::new();
        let mut affected_models = Vec::new();
        let mut affected_datasets = HashSet::new();

        for event in graph.events() {
            let uses_transform = event.transforms.iter().any(|t| {
                // Match by rule_id or transform_type
                t.rule_id == transform_name || t.transform_type == transform_name
            });

            if uses_transform {
                affected_records.insert(event.record_id.clone());
                affected_datasets.insert(event.dataset.clone());

                // Check if any models were affected by this transformation
                for model_ref in &event.model_refs {
                    affected_models.push(model_ref.clone());
                }
            }
        }

        // Analyze change type and determine breaking changes
        let mut breaking_changes = Vec::new();
        let mut warnings = Vec::new();

        if let Some(change_type) = change.details.get("change_type") {
            match change_type.as_str() {
                "logic_change" => {
                    breaking_changes.push(format!(
                        "Logic change in transformation '{}' will affect {} records across {} datasets",
                        transform_name,
                        affected_records.len(),
                        affected_datasets.len()
                    ));
                    warnings.push(
                        "All downstream consumers should be validated with new logic".to_string(),
                    );
                }
                "parameter_change" => {
                    if change.details.get("backwards_compatible") == Some(&"false".to_string()) {
                        breaking_changes.push(format!(
                            "Non-backwards-compatible parameter change in '{}'",
                            transform_name
                        ));
                    } else {
                        warnings.push(format!(
                            "Parameter change in '{}' - verify output consistency",
                            transform_name
                        ));
                    }
                }
                "version_upgrade" => {
                    warnings.push(format!(
                        "Version upgrade for '{}' from {} to {} - test thoroughly",
                        transform_name,
                        change
                            .details
                            .get("old_version")
                            .unwrap_or(&"unknown".to_string()),
                        transform_version.as_ref().unwrap_or(&"unknown".to_string())
                    ));
                }
                "deprecation" => {
                    breaking_changes.push(format!(
                        "Transformation '{}' is being deprecated - {} records need migration",
                        transform_name,
                        affected_records.len()
                    ));
                    warnings.push("Plan migration strategy for all dependent datasets".to_string());
                }
                _ => {
                    warnings.push(format!("Unknown change type: {}", change_type));
                }
            }
        } else {
            // No change type specified - generic warning
            warnings.push(format!(
                "Transformation '{}' change will affect {} records",
                transform_name,
                affected_records.len()
            ));
        }

        // Add warnings for affected models
        if !affected_models.is_empty() {
            warnings.push(format!(
                "{} ML models use data from this transformation - retesting recommended",
                affected_models.len()
            ));
        }

        // Generate recommendation based on impact
        let recommendation = if !breaking_changes.is_empty() {
            "CAUTION: Breaking changes detected. Create rollback plan and test in staging environment".to_string()
        } else if affected_records.len() > 10000 {
            "High impact change. Deploy incrementally and monitor closely".to_string()
        } else if affected_records.len() > 1000 {
            "Moderate impact. Test thoroughly before deployment".to_string()
        } else {
            "Low impact change. Standard testing procedures apply".to_string()
        };

        Ok(SimulationReport {
            change: change.clone(),
            affected_records_count: affected_records.len(),
            affected_models_count: affected_models.len(),
            affected_datasets: affected_datasets.into_iter().collect(),
            breaking_changes,
            warnings,
            recommendation,
        })
    }

    async fn simulate_source_removal(&self, change: &ProposedChange) -> Result<SimulationReport> {
        let source = DataRef {
            system: change.target.clone(),
            path: "".to_string(),
            version: None,
            extracted_at: Utc::now(),
            cdc_position: None,
        };

        let impact = self.forward_impact(&source, None).await?;

        Ok(SimulationReport {
            change: change.clone(),
            affected_records_count: impact.affected_records.len(),
            affected_models_count: impact.affected_models.len(),
            affected_datasets: impact.affected_datasets.clone(),
            breaking_changes: vec!["Source removal will break all downstream consumers".to_string()],
            warnings: impact
                .affected_datasets
                .iter()
                .map(|ds| format!("Dataset {} will lose data source", ds))
                .collect(),
            recommendation: "DO NOT PROCEED - critical dependencies exist".to_string(),
        })
    }
}

/// Impact analysis report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImpactReport {
    pub affected_records: Vec<String>,
    pub affected_models: Vec<ModelRef>,
    pub affected_datasets: Vec<String>,
    pub risk_level: RiskLevel,
    pub can_replay: bool,
    pub analysis_timestamp: DateTime<Utc>,
}

/// Root cause analysis report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RootCauseReport {
    pub record_id: String,
    pub source_data_refs: Vec<DataRef>,
    pub transformation_chain: Vec<TransformationStep>,
    pub models_applied: Vec<ModelRef>,
    pub quality_issues: Vec<QualityIssue>,
    pub can_replay: bool,
    pub replay_commands: Vec<String>,
}

/// Change simulation report
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SimulationReport {
    pub change: ProposedChange,
    pub affected_records_count: usize,
    pub affected_models_count: usize,
    pub affected_datasets: Vec<String>,
    pub breaking_changes: Vec<String>,
    pub warnings: Vec<String>,
    pub recommendation: String,
}

/// Transformation step in lineage chain
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransformationStep {
    pub transform_type: String,
    pub rule_id: String,
    pub version: String,
    pub applied_at: DateTime<Utc>,
}

/// Quality issue found in lineage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityIssue {
    pub rule_id: String,
    pub severity: String,
    pub message: String,
    pub detected_at: DateTime<Utc>,
}

/// Proposed change for simulation
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProposedChange {
    pub change_type: ChangeType,
    pub target: String,
    pub details: HashMap<String, String>,
}

/// Type of change being proposed
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub enum ChangeType {
    #[default]
    SchemaModification,
    TransformationUpdate,
    DataSourceRemoval,
}

/// Risk level for impact
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}
