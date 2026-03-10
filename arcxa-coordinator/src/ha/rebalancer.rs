//! Shard Rebalancing with Data Migration
//!
//! This module implements dynamic shard rebalancing with minimal downtime
//! through controlled data migration.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use super::consistent_hash::{ConsistentHashRing, VirtualNode};
use crate::governance::distributed::{ShardId, ShardMetadata};

/// Migration plan for rebalancing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationPlan {
    /// Unique migration ID
    pub id: String,
    /// Migration tasks
    pub tasks: Vec<MigrationTask>,
    /// Estimated data to move (bytes)
    pub estimated_bytes: u64,
    /// Estimated duration
    pub estimated_duration: Duration,
    /// Creation timestamp (not serialized - for internal tracking only)
    #[serde(skip, default = "Instant::now")]
    pub created_at: Instant,
    /// Approval status
    pub approved: bool,
}

/// Individual migration task
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationTask {
    /// Task ID
    pub id: String,
    /// Source shard
    pub source: ShardId,
    /// Target shard
    pub target: ShardId,
    /// Virtual nodes to migrate
    pub virtual_nodes: Vec<u64>,
    /// Estimated data size
    pub estimated_bytes: u64,
    /// Key ranges affected
    pub key_ranges: Vec<KeyRange>,
}

/// Key range for migration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyRange {
    /// Start hash
    pub start: u64,
    /// End hash (exclusive)
    pub end: u64,
}

/// Migration state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MigrationState {
    /// Planning phase
    Planning,
    /// Waiting for approval
    PendingApproval,
    /// Actively migrating data
    Migrating,
    /// Verifying migrated data
    Verifying,
    /// Switching routing
    Switching,
    /// Successfully completed
    Completed,
    /// Failed and rolled back
    Failed,
    /// Manually aborted
    Aborted,
}

/// Migration progress
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationProgress {
    /// Migration ID
    pub id: String,
    /// Current state
    pub state: MigrationState,
    /// Overall progress percentage
    pub progress: f64,
    /// Bytes migrated
    pub bytes_migrated: u64,
    /// Bytes total
    pub bytes_total: u64,
    /// Current task index
    pub current_task: usize,
    /// Total tasks
    pub total_tasks: usize,
    /// Start time (not serialized - for internal tracking only)
    #[serde(skip, default)]
    pub started_at: Option<Instant>,
    /// Completion time (not serialized - for internal tracking only)
    #[serde(skip, default)]
    pub completed_at: Option<Instant>,
    /// Error message if failed
    pub error: Option<String>,
    /// Current migration rate (bytes/sec)
    pub rate_bps: f64,
    /// Estimated time remaining
    pub eta: Option<Duration>,
}

/// Rebalancer configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RebalancerConfig {
    /// Maximum concurrent migrations
    pub max_concurrent: usize,
    /// Migration batch size (bytes)
    pub batch_size: u64,
    /// Migration rate limit (bytes/sec)
    pub rate_limit_bps: u64,
    /// Verification sample rate (0.0 to 1.0)
    pub verification_sample_rate: f64,
    /// Verification timeout
    pub verification_timeout: Duration,
    /// Auto-approve migrations below this size
    pub auto_approve_threshold: u64,
    /// Minimum time between rebalancing operations
    pub rebalance_cooldown: Duration,
    /// Load imbalance threshold to trigger rebalancing (percentage)
    pub imbalance_threshold: f64,
}

impl Default for RebalancerConfig {
    fn default() -> Self {
        RebalancerConfig {
            max_concurrent: 2,
            batch_size: 100 * 1024 * 1024,    // 100MB
            rate_limit_bps: 50 * 1024 * 1024, // 50MB/s
            verification_sample_rate: 0.01,   // 1% sampling
            verification_timeout: Duration::from_secs(300),
            auto_approve_threshold: 1024 * 1024 * 1024, // 1GB
            rebalance_cooldown: Duration::from_secs(3600), // 1 hour
            imbalance_threshold: 20.0,                  // 20% imbalance
        }
    }
}

/// Shard rebalancer
pub struct ShardRebalancer {
    /// Configuration
    config: RebalancerConfig,
    /// Active migrations
    active_migrations: Arc<RwLock<HashMap<String, ActiveMigration>>>,
    /// Migration history
    history: Arc<RwLock<Vec<MigrationProgress>>>,
    /// Last rebalance time
    last_rebalance: Arc<RwLock<Option<Instant>>>,
}

/// Active migration state
struct ActiveMigration {
    plan: MigrationPlan,
    progress: MigrationProgress,
    cancel_tx: mpsc::Sender<()>,
}

impl ShardRebalancer {
    /// Create a new rebalancer
    pub fn new(config: RebalancerConfig) -> Self {
        ShardRebalancer {
            config,
            active_migrations: Arc::new(RwLock::new(HashMap::new())),
            history: Arc::new(RwLock::new(Vec::new())),
            last_rebalance: Arc::new(RwLock::new(None)),
        }
    }

    /// Analyze current distribution and create rebalancing plan
    pub fn create_rebalancing_plan(
        &self,
        ring: &ConsistentHashRing,
        shard_stats: &HashMap<ShardId, ShardStats>,
    ) -> Result<Option<MigrationPlan>> {
        // Check cooldown
        if let Some(last) = *self.last_rebalance.read().unwrap() {
            if last.elapsed() < self.config.rebalance_cooldown {
                debug!("Rebalancing cooldown active, skipping");
                return Ok(None);
            }
        }

        // Calculate load distribution (virtual nodes)
        let distribution = ring.calculate_load_distribution();
        let vnode_imbalance = self.calculate_imbalance(&distribution);

        // Calculate actual data size imbalance
        let size_distribution: HashMap<ShardId, f64> = shard_stats
            .iter()
            .map(|(id, stats)| (*id, stats.size_bytes as f64))
            .collect();
        let data_imbalance = self.calculate_imbalance(&size_distribution);

        // Use the maximum of both imbalances
        let imbalance = vnode_imbalance.max(data_imbalance);

        if imbalance < self.config.imbalance_threshold {
            debug!(
                "Load imbalance {}% (vnodes: {}%, data: {}%) below threshold {}%",
                imbalance, vnode_imbalance, data_imbalance, self.config.imbalance_threshold
            );
            return Ok(None);
        }

        info!(
            "Load imbalance {}% exceeds threshold, planning rebalancing",
            imbalance
        );

        // Find overloaded and underloaded shards
        let (overloaded, underloaded) = self.identify_imbalanced_shards(&distribution, shard_stats);

        // Create migration tasks
        let tasks = self.plan_migrations(ring, &overloaded, &underloaded, shard_stats)?;

        if tasks.is_empty() {
            return Ok(None);
        }

        // Calculate estimates
        let estimated_bytes: u64 = tasks.iter().map(|t| t.estimated_bytes).sum();
        let estimated_duration = self.estimate_duration(estimated_bytes);

        let plan = MigrationPlan {
            id: format!("rebalance-{}", uuid::Uuid::new_v4()),
            tasks,
            estimated_bytes,
            estimated_duration,
            created_at: Instant::now(),
            approved: estimated_bytes <= self.config.auto_approve_threshold,
        };

        Ok(Some(plan))
    }

    /// Calculate load imbalance percentage
    fn calculate_imbalance(&self, distribution: &HashMap<ShardId, f64>) -> f64 {
        if distribution.is_empty() {
            return 0.0;
        }

        let values: Vec<f64> = distribution.values().copied().collect();
        let mean = values.iter().sum::<f64>() / values.len() as f64;
        let max_deviation = values.iter().map(|v| (v - mean).abs()).fold(0.0, f64::max);

        (max_deviation / mean) * 100.0
    }

    /// Identify overloaded and underloaded shards
    ///
    /// Uses a multi-factor approach:
    /// 1. Actual data size (primary factor)
    /// 2. Virtual node distribution (for hash space coverage)
    /// 3. Query rate and resource usage (optional, for future enhancement)
    fn identify_imbalanced_shards(
        &self,
        distribution: &HashMap<ShardId, f64>,
        shard_stats: &HashMap<ShardId, ShardStats>,
    ) -> (Vec<ShardId>, Vec<ShardId>) {
        if shard_stats.is_empty() {
            return (Vec::new(), Vec::new());
        }

        // Calculate mean data size
        let total_size: u64 = shard_stats.values().map(|s| s.size_bytes).sum();
        let mean_size = total_size as f64 / shard_stats.len() as f64;

        // Calculate mean hash space coverage (virtual nodes)
        let mean_vnode_coverage = distribution.values().sum::<f64>() / distribution.len() as f64;

        let mut overloaded = Vec::new();
        let mut underloaded = Vec::new();

        for (&shard_id, stats) in shard_stats {
            // Primary factor: actual data size
            let size_deviation = (stats.size_bytes as f64 - mean_size).abs() / mean_size;

            // Secondary factor: virtual node distribution
            let vnode_coverage = distribution.get(&shard_id).copied().unwrap_or(0.0);
            let vnode_deviation =
                (vnode_coverage - mean_vnode_coverage).abs() / mean_vnode_coverage;

            // Combined load metric (weighted 80% data size, 20% vnodes)
            let combined_load = (size_deviation * 0.8) + (vnode_deviation * 0.2);
            let threshold_fraction = self.config.imbalance_threshold / 100.0;

            // Classify shard based on combined load
            if combined_load > threshold_fraction {
                // Check if actually overloaded (above mean) or underloaded (below mean)
                if stats.size_bytes as f64 > mean_size {
                    overloaded.push(shard_id);
                    debug!(
                        "Shard {} identified as overloaded: size={}GB, mean={}GB, deviation={:.1}%",
                        shard_id,
                        stats.size_bytes / (1024 * 1024 * 1024),
                        mean_size as u64 / (1024 * 1024 * 1024),
                        size_deviation * 100.0
                    );
                } else {
                    underloaded.push(shard_id);
                    debug!("Shard {} identified as underloaded: size={}GB, mean={}GB, deviation={:.1}%",
                           shard_id, stats.size_bytes / (1024*1024*1024),
                           mean_size as u64 / (1024*1024*1024), size_deviation * 100.0);
                }
            }
        }

        // Sort by severity for better migration planning
        overloaded.sort_by_key(|&id| {
            std::cmp::Reverse(shard_stats.get(&id).map(|s| s.size_bytes).unwrap_or(0))
        });
        underloaded.sort_by_key(|&id| shard_stats.get(&id).map(|s| s.size_bytes).unwrap_or(0));

        (overloaded, underloaded)
    }

    /// Plan migration tasks
    fn plan_migrations(
        &self,
        ring: &ConsistentHashRing,
        overloaded: &[ShardId],
        underloaded: &[ShardId],
        shard_stats: &HashMap<ShardId, ShardStats>,
    ) -> Result<Vec<MigrationTask>> {
        let mut tasks = Vec::new();

        for &source in overloaded {
            if underloaded.is_empty() {
                break;
            }

            // Get virtual nodes for this shard
            let vnodes = ring
                .get_shard_vnodes(source)
                .ok_or_else(|| anyhow::anyhow!("No virtual nodes for shard {}", source))?;

            // Calculate how many vnodes to migrate
            let vnodes_to_migrate = (vnodes.len() / 4).max(1); // Move 25% of vnodes

            // Select target shard with lowest load
            let target = *underloaded
                .iter()
                .min_by_key(|&&id| shard_stats.get(&id).map(|s| s.size_bytes).unwrap_or(0))
                .unwrap();

            // Select vnodes to migrate (take first N for simplicity)
            let selected_vnodes: Vec<u64> = vnodes
                .iter()
                .take(vnodes_to_migrate)
                .map(|vn| vn.hash)
                .collect();

            // Estimate data size
            let estimated_bytes =
                self.estimate_vnode_data_size(source, &selected_vnodes, shard_stats);

            // Create key ranges
            let key_ranges = self.vnodes_to_key_ranges(&selected_vnodes);

            tasks.push(MigrationTask {
                id: format!("task-{}", uuid::Uuid::new_v4()),
                source,
                target,
                virtual_nodes: selected_vnodes,
                estimated_bytes,
                key_ranges,
            });
        }

        Ok(tasks)
    }

    /// Estimate data size for virtual nodes
    fn estimate_vnode_data_size(
        &self,
        shard_id: ShardId,
        vnodes: &[u64],
        shard_stats: &HashMap<ShardId, ShardStats>,
    ) -> u64 {
        if let Some(stats) = shard_stats.get(&shard_id) {
            // Rough estimate: assume uniform distribution
            let vnode_fraction = vnodes.len() as f64 / 150.0; // Assuming 150 vnodes per shard
            (stats.size_bytes as f64 * vnode_fraction) as u64
        } else {
            // Default estimate: 100MB per vnode
            vnodes.len() as u64 * 100 * 1024 * 1024
        }
    }

    /// Convert virtual nodes to key ranges
    fn vnodes_to_key_ranges(&self, vnodes: &[u64]) -> Vec<KeyRange> {
        let mut sorted = vnodes.to_vec();
        sorted.sort();

        sorted
            .windows(2)
            .map(|w| KeyRange {
                start: w[0],
                end: w[1],
            })
            .collect()
    }

    /// Estimate migration duration
    fn estimate_duration(&self, bytes: u64) -> Duration {
        let seconds = bytes as f64 / self.config.rate_limit_bps as f64;
        Duration::from_secs(seconds.ceil() as u64)
    }

    /// Execute a migration plan
    pub async fn execute_migration(&self, plan: MigrationPlan) -> Result<()> {
        if !plan.approved {
            return Err(anyhow::anyhow!("Migration plan not approved"));
        }

        let migration_id = plan.id.clone();
        info!("Starting migration {}", migration_id);

        // Check concurrent migration limit
        if self.active_migrations.read().unwrap().len() >= self.config.max_concurrent {
            return Err(anyhow::anyhow!("Maximum concurrent migrations reached"));
        }

        // Create progress tracker
        let progress = MigrationProgress {
            id: migration_id.clone(),
            state: MigrationState::Migrating,
            progress: 0.0,
            bytes_migrated: 0,
            bytes_total: plan.estimated_bytes,
            current_task: 0,
            total_tasks: plan.tasks.len(),
            started_at: Some(Instant::now()),
            completed_at: None,
            error: None,
            rate_bps: 0.0,
            eta: Some(plan.estimated_duration),
        };

        // Create cancellation channel
        let (cancel_tx, mut cancel_rx) = mpsc::channel(1);

        // Store active migration
        {
            let mut active = self.active_migrations.write().unwrap();
            active.insert(
                migration_id.clone(),
                ActiveMigration {
                    plan: plan.clone(),
                    progress: progress.clone(),
                    cancel_tx,
                },
            );
        }

        // Execute migration tasks
        let result = self
            .execute_migration_tasks(plan, &migration_id, &mut cancel_rx)
            .await;

        // Remove from active migrations
        self.active_migrations
            .write()
            .unwrap()
            .remove(&migration_id);

        // Update history
        let final_progress = if result.is_ok() {
            MigrationProgress {
                state: MigrationState::Completed,
                completed_at: Some(Instant::now()),
                progress: 100.0,
                ..progress
            }
        } else {
            MigrationProgress {
                state: MigrationState::Failed,
                completed_at: Some(Instant::now()),
                error: result.as_ref().err().map(|e| e.to_string()),
                ..progress
            }
        };

        self.history.write().unwrap().push(final_progress);

        // Update last rebalance time
        *self.last_rebalance.write().unwrap() = Some(Instant::now());

        result
    }

    /// Execute migration tasks
    async fn execute_migration_tasks(
        &self,
        plan: MigrationPlan,
        migration_id: &str,
        cancel_rx: &mut mpsc::Receiver<()>,
    ) -> Result<()> {
        for (i, task) in plan.tasks.iter().enumerate() {
            // Check for cancellation
            if cancel_rx.try_recv().is_ok() {
                warn!("Migration {} cancelled", migration_id);
                return Err(anyhow::anyhow!("Migration cancelled"));
            }

            info!(
                "Executing migration task {}/{}: {} -> {}",
                i + 1,
                plan.tasks.len(),
                task.source,
                task.target
            );

            // Update progress
            self.update_progress(migration_id, |p| {
                p.current_task = i;
                p.state = MigrationState::Migrating;
            });

            // Execute the actual migration
            self.migrate_task_data(task, migration_id).await?;

            // Verify migration
            self.update_progress(migration_id, |p| {
                p.state = MigrationState::Verifying;
            });

            self.verify_migration(task).await?;
        }

        // Switch routing
        self.update_progress(migration_id, |p| {
            p.state = MigrationState::Switching;
        });

        self.switch_routing(&plan).await?;

        info!("Migration {} completed successfully", migration_id);
        Ok(())
    }

    /// Migrate data for a single task
    async fn migrate_task_data(&self, task: &MigrationTask, migration_id: &str) -> Result<()> {
        // TODO: Implement actual data migration
        // This would involve:
        // 1. Connecting to source shard
        // 2. Reading data in batches
        // 3. Applying rate limiting
        // 4. Writing to target shard
        // 5. Tracking progress

        // Simulate migration
        let batches = (task.estimated_bytes / self.config.batch_size).max(1);
        for batch in 0..batches {
            tokio::time::sleep(Duration::from_millis(100)).await;

            let bytes_done = ((batch + 1) * self.config.batch_size).min(task.estimated_bytes);
            self.update_progress(migration_id, |p| {
                p.bytes_migrated += self
                    .config
                    .batch_size
                    .min(task.estimated_bytes - p.bytes_migrated);
                p.progress = (p.bytes_migrated as f64 / p.bytes_total as f64) * 100.0;
            });
        }

        Ok(())
    }

    /// Verify migrated data
    async fn verify_migration(&self, task: &MigrationTask) -> Result<()> {
        // TODO: Implement actual verification
        // This would involve:
        // 1. Sampling data from source
        // 2. Checking it exists in target
        // 3. Comparing checksums

        // Simulate verification
        tokio::time::sleep(Duration::from_millis(50)).await;
        Ok(())
    }

    /// Switch routing to use new shard assignments
    async fn switch_routing(&self, plan: &MigrationPlan) -> Result<()> {
        // TODO: Update consistent hash ring via Raft consensus
        // This would involve proposing the new ring configuration

        info!("Switching routing for migration {}", plan.id);
        Ok(())
    }

    /// Update migration progress
    fn update_progress<F>(&self, migration_id: &str, updater: F)
    where
        F: FnOnce(&mut MigrationProgress),
    {
        let mut active = self.active_migrations.write().unwrap();
        if let Some(migration) = active.get_mut(migration_id) {
            updater(&mut migration.progress);

            // Calculate rate and ETA
            if let Some(started) = migration.progress.started_at {
                let elapsed = started.elapsed().as_secs_f64();
                if elapsed > 0.0 {
                    migration.progress.rate_bps =
                        migration.progress.bytes_migrated as f64 / elapsed;

                    let remaining =
                        migration.progress.bytes_total - migration.progress.bytes_migrated;
                    if migration.progress.rate_bps > 0.0 {
                        let eta_secs = remaining as f64 / migration.progress.rate_bps;
                        migration.progress.eta = Some(Duration::from_secs(eta_secs as u64));
                    }
                }
            }
        }
    }

    /// Get active migrations
    pub fn get_active_migrations(&self) -> Vec<MigrationProgress> {
        self.active_migrations
            .read()
            .unwrap()
            .values()
            .map(|m| m.progress.clone())
            .collect()
    }

    /// Cancel a migration
    pub async fn cancel_migration(&self, migration_id: &str) -> Result<()> {
        let cancel_tx = {
            let active = self.active_migrations.read().unwrap();
            active
                .get(migration_id)
                .ok_or_else(|| anyhow::anyhow!("Migration {} not found", migration_id))?
                .cancel_tx
                .clone()
        };

        cancel_tx
            .send(())
            .await
            .context("Failed to send cancellation signal")?;

        Ok(())
    }
}

/// Shard statistics for rebalancing decisions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShardStats {
    pub shard_id: ShardId,
    pub size_bytes: u64,
    pub capacity_bytes: u64,
    pub triple_count: u64,
    pub query_rate: f64,
    pub cpu_usage: f64,
    pub memory_usage: f64,
}

use uuid;

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_stats(id: u32, size: u64) -> ShardStats {
        ShardStats {
            shard_id: ShardId(id),
            size_bytes: size,
            capacity_bytes: 1000 * 1024 * 1024 * 1024, // 1TB
            triple_count: size / 100,                  // Rough estimate
            query_rate: 100.0,
            cpu_usage: 50.0,
            memory_usage: 60.0,
        }
    }

    #[test]
    fn test_imbalance_calculation() {
        let rebalancer = ShardRebalancer::new(RebalancerConfig::default());

        let mut distribution = HashMap::new();
        distribution.insert(ShardId(1), 25.0);
        distribution.insert(ShardId(2), 25.0);
        distribution.insert(ShardId(3), 25.0);
        distribution.insert(ShardId(4), 25.0);

        // Perfect balance
        assert!(rebalancer.calculate_imbalance(&distribution) < 1.0);

        // Create imbalance
        distribution.insert(ShardId(1), 40.0);
        distribution.insert(ShardId(2), 20.0);

        let imbalance = rebalancer.calculate_imbalance(&distribution);
        assert!(imbalance > 10.0);
    }

    #[test]
    fn test_migration_plan_creation() {
        let rebalancer = ShardRebalancer::new(RebalancerConfig::default());

        let mut ring = ConsistentHashRing::new();
        ring.add_shard(ShardId(1), 150).unwrap();
        ring.add_shard(ShardId(2), 150).unwrap();

        let mut shard_stats = HashMap::new();
        shard_stats.insert(ShardId(1), create_test_stats(1, 100_000_000_000)); // 100GB
        shard_stats.insert(ShardId(2), create_test_stats(2, 10_000_000_000)); // 10GB

        // This should trigger rebalancing due to imbalance
        let plan = rebalancer
            .create_rebalancing_plan(&ring, &shard_stats)
            .unwrap();
        assert!(plan.is_some());

        if let Some(plan) = plan {
            assert!(!plan.tasks.is_empty());
            assert!(plan.estimated_bytes > 0);
        }
    }

    #[test]
    fn test_no_rebalancing_when_balanced() {
        let rebalancer = ShardRebalancer::new(RebalancerConfig::default());

        let mut ring = ConsistentHashRing::new();
        ring.add_shard(ShardId(1), 150).unwrap();
        ring.add_shard(ShardId(2), 150).unwrap();

        let mut shard_stats = HashMap::new();
        shard_stats.insert(ShardId(1), create_test_stats(1, 50_000_000_000)); // 50GB
        shard_stats.insert(ShardId(2), create_test_stats(2, 51_000_000_000)); // 51GB (within 20% threshold)

        // Should NOT trigger rebalancing - within threshold
        let plan = rebalancer
            .create_rebalancing_plan(&ring, &shard_stats)
            .unwrap();
        assert!(
            plan.is_none(),
            "Should not create plan when shards are balanced"
        );
    }

    #[test]
    fn test_extreme_imbalance() {
        let rebalancer = ShardRebalancer::new(RebalancerConfig::default());

        let mut ring = ConsistentHashRing::new();
        ring.add_shard(ShardId(1), 150).unwrap();
        ring.add_shard(ShardId(2), 150).unwrap();
        ring.add_shard(ShardId(3), 150).unwrap();

        let mut shard_stats = HashMap::new();
        shard_stats.insert(ShardId(1), create_test_stats(1, 500_000_000_000)); // 500GB
        shard_stats.insert(ShardId(2), create_test_stats(2, 10_000_000_000)); // 10GB
        shard_stats.insert(ShardId(3), create_test_stats(3, 10_000_000_000)); // 10GB

        let plan = rebalancer
            .create_rebalancing_plan(&ring, &shard_stats)
            .unwrap();
        assert!(plan.is_some(), "Should create plan for extreme imbalance");

        if let Some(plan) = plan {
            // Should have tasks to move from shard 1 to shards 2 & 3
            assert!(plan.tasks.len() > 0);
            assert!(
                plan.estimated_bytes > 50_000_000_000,
                "Should move significant data"
            );

            // Source should be the overloaded shard
            for task in &plan.tasks {
                assert_eq!(
                    task.source,
                    ShardId(1),
                    "Should migrate from overloaded shard"
                );
                assert!(
                    task.target == ShardId(2) || task.target == ShardId(3),
                    "Should migrate to underloaded shards"
                );
            }
        }
    }

    #[test]
    fn test_single_shard_no_rebalancing() {
        let rebalancer = ShardRebalancer::new(RebalancerConfig::default());

        let mut ring = ConsistentHashRing::new();
        ring.add_shard(ShardId(1), 150).unwrap();

        let mut shard_stats = HashMap::new();
        shard_stats.insert(ShardId(1), create_test_stats(1, 100_000_000_000)); // 100GB

        // Cannot rebalance with single shard
        let plan = rebalancer
            .create_rebalancing_plan(&ring, &shard_stats)
            .unwrap();
        assert!(plan.is_none(), "Cannot rebalance with single shard");
    }

    #[test]
    fn test_empty_shards() {
        let rebalancer = ShardRebalancer::new(RebalancerConfig::default());

        let mut ring = ConsistentHashRing::new();
        ring.add_shard(ShardId(1), 150).unwrap();
        ring.add_shard(ShardId(2), 150).unwrap();

        let mut shard_stats = HashMap::new();
        shard_stats.insert(ShardId(1), create_test_stats(1, 0)); // Empty
        shard_stats.insert(ShardId(2), create_test_stats(2, 0)); // Empty

        // No data to rebalance
        let plan = rebalancer
            .create_rebalancing_plan(&ring, &shard_stats)
            .unwrap();
        assert!(plan.is_none(), "No rebalancing needed for empty shards");
    }

    #[test]
    fn test_imbalance_identification() {
        let rebalancer = ShardRebalancer::new(RebalancerConfig::default());

        let distribution = HashMap::from([(ShardId(1), 50.0), (ShardId(2), 50.0)]);

        let mut shard_stats = HashMap::new();
        shard_stats.insert(ShardId(1), create_test_stats(1, 100_000_000_000)); // 100GB
        shard_stats.insert(ShardId(2), create_test_stats(2, 20_000_000_000)); // 20GB

        let (overloaded, underloaded) =
            rebalancer.identify_imbalanced_shards(&distribution, &shard_stats);

        assert_eq!(overloaded.len(), 1, "Should identify one overloaded shard");
        assert_eq!(overloaded[0], ShardId(1), "Shard 1 should be overloaded");

        assert_eq!(
            underloaded.len(),
            1,
            "Should identify one underloaded shard"
        );
        assert_eq!(underloaded[0], ShardId(2), "Shard 2 should be underloaded");
    }

    #[test]
    fn test_vnode_data_estimation() {
        let rebalancer = ShardRebalancer::new(RebalancerConfig::default());

        let mut shard_stats = HashMap::new();
        shard_stats.insert(ShardId(1), create_test_stats(1, 150_000_000_000)); // 150GB with 150 vnodes

        // Estimate for 30 vnodes (20% of total)
        let vnodes: Vec<u64> = (0..30).collect();
        let estimated = rebalancer.estimate_vnode_data_size(ShardId(1), &vnodes, &shard_stats);

        // Should be approximately 20% of 150GB = 30GB
        let expected = 30_000_000_000u64;
        let tolerance = 1_000_000_000u64; // 1GB tolerance
        assert!(
            estimated >= expected - tolerance && estimated <= expected + tolerance,
            "Estimated {} should be near {} (150GB * 20%)",
            estimated,
            expected
        );
    }

    #[test]
    fn test_migration_duration_estimation() {
        let config = RebalancerConfig {
            rate_limit_bps: 100 * 1024 * 1024, // 100MB/s
            ..Default::default()
        };
        let rebalancer = ShardRebalancer::new(config);

        // 10GB migration at 100MB/s should take ~100 seconds
        let bytes = 10 * 1024 * 1024 * 1024u64; // 10GB
        let duration = rebalancer.estimate_duration(bytes);

        let expected_seconds = 102; // ~100 seconds (10GB / 100MB/s)
        assert!(
            duration.as_secs() >= expected_seconds - 5
                && duration.as_secs() <= expected_seconds + 5,
            "Duration {} should be near {} seconds",
            duration.as_secs(),
            expected_seconds
        );
    }

    #[test]
    fn test_key_range_generation() {
        let rebalancer = ShardRebalancer::new(RebalancerConfig::default());

        let vnodes = vec![100u64, 500, 1000, 2000];
        let ranges = rebalancer.vnodes_to_key_ranges(&vnodes);

        assert_eq!(ranges.len(), 3, "Should create N-1 ranges for N vnodes");
        assert_eq!(ranges[0].start, 100);
        assert_eq!(ranges[0].end, 500);
        assert_eq!(ranges[1].start, 500);
        assert_eq!(ranges[1].end, 1000);
        assert_eq!(ranges[2].start, 1000);
        assert_eq!(ranges[2].end, 2000);
    }

    #[test]
    fn test_sorted_imbalanced_shards() {
        let rebalancer = ShardRebalancer::new(RebalancerConfig::default());

        let distribution =
            HashMap::from([(ShardId(1), 33.0), (ShardId(2), 33.0), (ShardId(3), 34.0)]);

        let mut shard_stats = HashMap::new();
        shard_stats.insert(ShardId(1), create_test_stats(1, 200_000_000_000)); // 200GB - most overloaded
        shard_stats.insert(ShardId(2), create_test_stats(2, 100_000_000_000)); // 100GB - less overloaded
        shard_stats.insert(ShardId(3), create_test_stats(3, 10_000_000_000)); // 10GB - underloaded

        let (overloaded, underloaded) =
            rebalancer.identify_imbalanced_shards(&distribution, &shard_stats);

        // Overloaded shards should be sorted by size (largest first)
        if overloaded.len() >= 2 {
            let size1 = shard_stats.get(&overloaded[0]).unwrap().size_bytes;
            let size2 = shard_stats.get(&overloaded[1]).unwrap().size_bytes;
            assert!(
                size1 >= size2,
                "Overloaded shards should be sorted largest first"
            );
        }

        // Underloaded shards should be sorted by size (smallest first)
        if underloaded.len() >= 1 {
            assert_eq!(
                underloaded[0],
                ShardId(3),
                "Most underloaded should be first"
            );
        }
    }
}
