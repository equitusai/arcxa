//! DAG-based workflow execution
//!
//! Uses petgraph to build dependency graphs and execute steps in topological order

use anyhow::{Context, Result};
use petgraph::algo::{is_cyclic_directed, toposort};
use petgraph::graph::{DiGraph, NodeIndex};
use std::collections::HashMap;

use super::definition::{WorkflowDefinition, WorkflowStep};

/// DAG executor for workflow steps
pub struct DagExecutor {
    /// Petgraph directed graph
    graph: DiGraph<String, ()>,
    /// Map step ID to node index
    step_index: HashMap<String, NodeIndex>,
    /// Map node index to step
    index_step: HashMap<NodeIndex, WorkflowStep>,
}

impl DagExecutor {
    /// Build DAG from workflow definition
    pub fn from_workflow(workflow: &WorkflowDefinition) -> Result<Self> {
        let mut graph = DiGraph::new();
        let mut step_index = HashMap::new();
        let mut index_step = HashMap::new();

        // Add all steps as nodes
        for step in &workflow.steps {
            let idx = graph.add_node(step.id.clone());
            step_index.insert(step.id.clone(), idx);
            index_step.insert(idx, step.clone());
        }

        // Add edges for dependencies
        for step in &workflow.steps {
            let step_idx = step_index[&step.id];
            for dep_id in &step.depends_on {
                let dep_idx = step_index.get(dep_id).ok_or_else(|| {
                    anyhow::anyhow!("Step '{}' depends on unknown step '{}'", step.id, dep_id)
                })?;
                // Edge from dependency to dependent (dep -> step)
                graph.add_edge(*dep_idx, step_idx, ());
            }
        }

        // Check for cycles
        if is_cyclic_directed(&graph) {
            anyhow::bail!("Workflow contains cyclic dependencies");
        }

        Ok(Self {
            graph,
            step_index,
            index_step,
        })
    }

    /// Get execution order via topological sort
    pub fn execution_order(&self) -> Result<Vec<WorkflowStep>> {
        let sorted = toposort(&self.graph, None)
            .map_err(|_| anyhow::anyhow!("Failed to compute topological order"))?;

        Ok(sorted
            .into_iter()
            .map(|idx| self.index_step[&idx].clone())
            .collect())
    }

    /// Get steps that can execute in parallel (same level in DAG)
    pub fn parallel_batches(&self) -> Result<Vec<Vec<WorkflowStep>>> {
        let sorted = toposort(&self.graph, None)
            .map_err(|_| anyhow::anyhow!("Failed to compute topological order"))?;

        let mut batches: Vec<Vec<WorkflowStep>> = Vec::new();
        let mut visited = HashMap::new();
        let mut current_level = 0;

        for node_idx in sorted {
            // Calculate level (max depth from root)
            let mut level = 0;
            for incoming in self
                .graph
                .neighbors_directed(node_idx, petgraph::Direction::Incoming)
            {
                if let Some(&dep_level) = visited.get(&incoming) {
                    level = level.max(dep_level + 1);
                }
            }

            visited.insert(node_idx, level);

            // Add to appropriate batch
            while batches.len() <= level {
                batches.push(Vec::new());
            }
            batches[level].push(self.index_step[&node_idx].clone());
        }

        Ok(batches)
    }

    /// Get dependencies for a specific step
    pub fn get_dependencies(&self, step_id: &str) -> Result<Vec<String>> {
        let step_idx = self
            .step_index
            .get(step_id)
            .ok_or_else(|| anyhow::anyhow!("Unknown step: {}", step_id))?;

        Ok(self
            .graph
            .neighbors_directed(*step_idx, petgraph::Direction::Incoming)
            .map(|idx| self.graph[idx].clone())
            .collect())
    }

    /// Get dependents for a specific step
    pub fn get_dependents(&self, step_id: &str) -> Result<Vec<String>> {
        let step_idx = self
            .step_index
            .get(step_id)
            .ok_or_else(|| anyhow::anyhow!("Unknown step: {}", step_id))?;

        Ok(self
            .graph
            .neighbors_directed(*step_idx, petgraph::Direction::Outgoing)
            .map(|idx| self.graph[idx].clone())
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestration::workflow::definition::{
        ConfidenceGateConfig, FallbackStrategy, StepConfig, StepType,
    };

    fn create_test_workflow() -> WorkflowDefinition {
        WorkflowDefinition {
            steps: vec![
                WorkflowStep {
                    id: "step1".to_string(),
                    step_type: StepType::ConfidenceGate,
                    config: StepConfig::ConfidenceGate(ConfidenceGateConfig {
                        threshold: 0.8,
                        input_step: None,
                    }),
                    depends_on: vec![],
                },
                WorkflowStep {
                    id: "step2".to_string(),
                    step_type: StepType::ConfidenceGate,
                    config: StepConfig::ConfidenceGate(ConfidenceGateConfig {
                        threshold: 0.9,
                        input_step: None,
                    }),
                    depends_on: vec!["step1".to_string()],
                },
                WorkflowStep {
                    id: "step3".to_string(),
                    step_type: StepType::ConfidenceGate,
                    config: StepConfig::ConfidenceGate(ConfidenceGateConfig {
                        threshold: 0.85,
                        input_step: None,
                    }),
                    depends_on: vec!["step1".to_string()],
                },
                WorkflowStep {
                    id: "step4".to_string(),
                    step_type: StepType::ConfidenceGate,
                    config: StepConfig::ConfidenceGate(ConfidenceGateConfig {
                        threshold: 0.95,
                        input_step: None,
                    }),
                    depends_on: vec!["step2".to_string(), "step3".to_string()],
                },
            ],
            fusion_threshold: 0.8,
            fallback: FallbackStrategy::ManualReview,
        }
    }

    #[test]
    fn test_dag_construction() {
        let workflow = create_test_workflow();
        let dag = DagExecutor::from_workflow(&workflow).unwrap();

        assert_eq!(dag.step_index.len(), 4);
    }

    #[test]
    fn test_execution_order() {
        let workflow = create_test_workflow();
        let dag = DagExecutor::from_workflow(&workflow).unwrap();
        let order = dag.execution_order().unwrap();

        assert_eq!(order.len(), 4);
        assert_eq!(order[0].id, "step1");
        assert_eq!(order[3].id, "step4");
    }

    #[test]
    fn test_parallel_batches() {
        let workflow = create_test_workflow();
        let dag = DagExecutor::from_workflow(&workflow).unwrap();
        let batches = dag.parallel_batches().unwrap();

        // Level 0: step1
        assert_eq!(batches[0].len(), 1);
        assert_eq!(batches[0][0].id, "step1");

        // Level 1: step2, step3 (can run in parallel)
        assert_eq!(batches[1].len(), 2);

        // Level 2: step4
        assert_eq!(batches[2].len(), 1);
        assert_eq!(batches[2][0].id, "step4");
    }

    #[test]
    fn test_cyclic_detection() {
        let workflow = WorkflowDefinition {
            steps: vec![
                WorkflowStep {
                    id: "step1".to_string(),
                    step_type: StepType::ConfidenceGate,
                    config: StepConfig::ConfidenceGate(ConfidenceGateConfig {
                        threshold: 0.8,
                        input_step: None,
                    }),
                    depends_on: vec!["step2".to_string()],
                },
                WorkflowStep {
                    id: "step2".to_string(),
                    step_type: StepType::ConfidenceGate,
                    config: StepConfig::ConfidenceGate(ConfidenceGateConfig {
                        threshold: 0.9,
                        input_step: None,
                    }),
                    depends_on: vec!["step1".to_string()],
                },
            ],
            fusion_threshold: 0.8,
            fallback: FallbackStrategy::ManualReview,
        };

        let result = DagExecutor::from_workflow(&workflow);
        assert!(result.is_err());
    }

    #[test]
    fn test_unknown_dependency() {
        let workflow = WorkflowDefinition {
            steps: vec![WorkflowStep {
                id: "step1".to_string(),
                step_type: StepType::ConfidenceGate,
                config: StepConfig::ConfidenceGate(ConfidenceGateConfig {
                    threshold: 0.8,
                    input_step: None,
                }),
                depends_on: vec!["unknown_step".to_string()],
            }],
            fusion_threshold: 0.8,
            fallback: FallbackStrategy::ManualReview,
        };

        let result = DagExecutor::from_workflow(&workflow);
        assert!(result.is_err());
    }
}
