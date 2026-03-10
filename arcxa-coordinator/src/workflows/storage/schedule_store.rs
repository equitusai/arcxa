//! Schedule Storage - CRUD operations for workflow schedules
//!
//! In-memory storage with future support for persistent storage.

use crate::workflows::domain::WorkflowSchedule;
use anyhow::Result;
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// In-memory schedule storage
///
/// Thread-safe storage using RwLock for concurrent access.
/// Future: Replace with persistent backend.
#[derive(Clone)]
pub struct ScheduleStore {
    schedules: Arc<RwLock<HashMap<String, WorkflowSchedule>>>,
}

impl Default for ScheduleStore {
    fn default() -> Self {
        Self::new()
    }
}

impl ScheduleStore {
    /// Create a new empty schedule store
    pub fn new() -> Self {
        Self {
            schedules: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create a schedule
    ///
    /// ## Errors
    /// - If schedule ID already exists
    pub fn create(&self, schedule: WorkflowSchedule) -> Result<()> {
        let mut schedules = self
            .schedules
            .write()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {}", e))?;

        if schedules.contains_key(&schedule.schedule_id) {
            anyhow::bail!("Schedule '{}' already exists", schedule.schedule_id);
        }

        schedules.insert(schedule.schedule_id.clone(), schedule);

        Ok(())
    }

    /// Get a schedule by ID
    pub fn get(&self, schedule_id: &str) -> Result<Option<WorkflowSchedule>> {
        let schedules = self
            .schedules
            .read()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {}", e))?;

        Ok(schedules.get(schedule_id).cloned())
    }

    /// Get a schedule by ID (required, returns error if not found)
    pub fn get_required(&self, schedule_id: &str) -> Result<WorkflowSchedule> {
        self.get(schedule_id)?
            .ok_or_else(|| anyhow::anyhow!("Schedule '{}' not found", schedule_id))
    }

    /// Get schedule by workflow ID
    pub fn get_by_workflow(&self, workflow_id: &str) -> Result<Option<WorkflowSchedule>> {
        let schedules = self
            .schedules
            .read()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {}", e))?;

        Ok(schedules
            .values()
            .find(|s| s.workflow_id == workflow_id)
            .cloned())
    }

    /// Update a schedule
    ///
    /// ## Errors
    /// - If schedule doesn't exist
    pub fn update(&self, schedule: WorkflowSchedule) -> Result<()> {
        let mut schedules = self
            .schedules
            .write()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {}", e))?;

        if !schedules.contains_key(&schedule.schedule_id) {
            anyhow::bail!("Schedule '{}' not found", schedule.schedule_id);
        }

        schedules.insert(schedule.schedule_id.clone(), schedule);

        Ok(())
    }

    /// Delete a schedule
    ///
    /// ## Errors
    /// - If schedule doesn't exist
    pub fn delete(&self, schedule_id: &str) -> Result<()> {
        let mut schedules = self
            .schedules
            .write()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {}", e))?;

        if schedules.remove(schedule_id).is_none() {
            anyhow::bail!("Schedule '{}' not found", schedule_id);
        }

        Ok(())
    }

    /// Delete schedule by workflow ID
    pub fn delete_by_workflow(&self, workflow_id: &str) -> Result<bool> {
        let mut schedules = self
            .schedules
            .write()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {}", e))?;

        let schedule_id = schedules
            .iter()
            .find(|(_, s)| s.workflow_id == workflow_id)
            .map(|(id, _)| id.clone());

        if let Some(id) = schedule_id {
            schedules.remove(&id);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// List all schedules
    pub fn list(&self) -> Result<Vec<WorkflowSchedule>> {
        let schedules = self
            .schedules
            .read()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {}", e))?;

        let mut result: Vec<WorkflowSchedule> = schedules.values().cloned().collect();
        result.sort_by(|a, b| a.schedule_id.cmp(&b.schedule_id));

        Ok(result)
    }

    /// List schedules by workflow ID
    pub fn list_by_workflow(&self, workflow_id: &str) -> Result<Vec<WorkflowSchedule>> {
        let schedules = self
            .schedules
            .read()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {}", e))?;

        let result: Vec<WorkflowSchedule> = schedules
            .values()
            .filter(|s| s.workflow_id == workflow_id)
            .cloned()
            .collect();

        Ok(result)
    }

    /// List enabled schedules only
    pub fn list_enabled(&self) -> Result<Vec<WorkflowSchedule>> {
        let schedules = self
            .schedules
            .read()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {}", e))?;

        let result: Vec<WorkflowSchedule> =
            schedules.values().filter(|s| s.enabled).cloned().collect();

        Ok(result)
    }

    /// List schedules that need to run (next_run <= now and enabled)
    pub fn list_due(&self, as_of: DateTime<Utc>) -> Result<Vec<WorkflowSchedule>> {
        let schedules = self
            .schedules
            .read()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {}", e))?;

        let result: Vec<WorkflowSchedule> = schedules
            .values()
            .filter(|s| s.enabled && s.next_run.map(|next| next <= as_of).unwrap_or(false))
            .cloned()
            .collect();

        Ok(result)
    }

    /// Count total schedules
    pub fn count(&self) -> Result<usize> {
        let schedules = self
            .schedules
            .read()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {}", e))?;

        Ok(schedules.len())
    }

    /// Check if a schedule exists
    pub fn exists(&self, schedule_id: &str) -> Result<bool> {
        let schedules = self
            .schedules
            .read()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {}", e))?;

        Ok(schedules.contains_key(schedule_id))
    }

    /// Clear all schedules (for testing)
    #[cfg(test)]
    pub fn clear(&self) -> Result<()> {
        let mut schedules = self
            .schedules
            .write()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {}", e))?;

        schedules.clear();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn create_test_schedule(schedule_id: &str, workflow_id: &str) -> WorkflowSchedule {
        WorkflowSchedule::new(
            schedule_id.to_string(),
            workflow_id.to_string(),
            format!("Workflow {}", workflow_id),
            Some("0 0 * * *".to_string()),
            None,
            None,
            "UTC".to_string(),
            serde_json::json!({}),
            serde_json::json!({}),
            true,
        )
    }

    #[test]
    fn test_create_schedule() {
        let store = ScheduleStore::new();
        let schedule = create_test_schedule("sched_001", "wf_001");

        assert!(store.create(schedule).is_ok());
    }

    #[test]
    fn test_create_duplicate_schedule() {
        let store = ScheduleStore::new();
        let schedule = create_test_schedule("sched_001", "wf_001");

        store.create(schedule.clone()).unwrap();

        // Second create should fail
        let result = store.create(schedule);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("already exists"));
    }

    #[test]
    fn test_get_schedule() {
        let store = ScheduleStore::new();
        let schedule = create_test_schedule("sched_001", "wf_001");

        store.create(schedule.clone()).unwrap();

        let retrieved = store.get("sched_001").unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().schedule_id, "sched_001");
    }

    #[test]
    fn test_get_nonexistent_schedule() {
        let store = ScheduleStore::new();

        let retrieved = store.get("sched_999").unwrap();
        assert!(retrieved.is_none());
    }

    #[test]
    fn test_get_required_schedule() {
        let store = ScheduleStore::new();
        let schedule = create_test_schedule("sched_001", "wf_001");

        store.create(schedule).unwrap();

        let retrieved = store.get_required("sched_001").unwrap();
        assert_eq!(retrieved.schedule_id, "sched_001");
    }

    #[test]
    fn test_get_required_nonexistent() {
        let store = ScheduleStore::new();

        let result = store.get_required("sched_999");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn test_get_by_workflow() {
        let store = ScheduleStore::new();
        let schedule = create_test_schedule("sched_001", "wf_001");

        store.create(schedule).unwrap();

        let retrieved = store.get_by_workflow("wf_001").unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().workflow_id, "wf_001");
    }

    #[test]
    fn test_get_by_workflow_nonexistent() {
        let store = ScheduleStore::new();

        let retrieved = store.get_by_workflow("wf_999").unwrap();
        assert!(retrieved.is_none());
    }

    #[test]
    fn test_update_schedule() {
        let store = ScheduleStore::new();
        let mut schedule = create_test_schedule("sched_001", "wf_001");

        store.create(schedule.clone()).unwrap();

        // Modify schedule
        schedule.update(
            Some("0 */2 * * *".to_string()),
            None,
            None,
            "America/New_York".to_string(),
            None,
            None,
            false,
        );

        store.update(schedule).unwrap();

        // Verify update
        let updated = store.get("sched_001").unwrap().unwrap();
        assert_eq!(updated.cron_expression, Some("0 */2 * * *".to_string()));
        assert_eq!(updated.timezone, "America/New_York");
        assert!(!updated.enabled);
    }

    #[test]
    fn test_update_nonexistent_schedule() {
        let store = ScheduleStore::new();
        let schedule = create_test_schedule("sched_001", "wf_001");

        let result = store.update(schedule);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn test_delete_schedule() {
        let store = ScheduleStore::new();
        let schedule = create_test_schedule("sched_001", "wf_001");

        store.create(schedule).unwrap();
        assert!(store.exists("sched_001").unwrap());

        store.delete("sched_001").unwrap();
        assert!(!store.exists("sched_001").unwrap());
    }

    #[test]
    fn test_delete_nonexistent_schedule() {
        let store = ScheduleStore::new();

        let result = store.delete("sched_999");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn test_delete_by_workflow() {
        let store = ScheduleStore::new();
        let schedule = create_test_schedule("sched_001", "wf_001");

        store.create(schedule).unwrap();
        assert!(store.exists("sched_001").unwrap());

        let deleted = store.delete_by_workflow("wf_001").unwrap();
        assert!(deleted);
        assert!(!store.exists("sched_001").unwrap());
    }

    #[test]
    fn test_delete_by_workflow_nonexistent() {
        let store = ScheduleStore::new();

        let deleted = store.delete_by_workflow("wf_999").unwrap();
        assert!(!deleted);
    }

    #[test]
    fn test_list_schedules() {
        let store = ScheduleStore::new();

        store
            .create(create_test_schedule("sched_001", "wf_001"))
            .unwrap();
        store
            .create(create_test_schedule("sched_002", "wf_002"))
            .unwrap();
        store
            .create(create_test_schedule("sched_003", "wf_003"))
            .unwrap();

        let schedules = store.list().unwrap();
        assert_eq!(schedules.len(), 3);
    }

    #[test]
    fn test_list_by_workflow() {
        let store = ScheduleStore::new();

        store
            .create(create_test_schedule("sched_001", "wf_001"))
            .unwrap();
        store
            .create(create_test_schedule("sched_002", "wf_001"))
            .unwrap();
        store
            .create(create_test_schedule("sched_003", "wf_002"))
            .unwrap();

        let schedules = store.list_by_workflow("wf_001").unwrap();
        assert_eq!(schedules.len(), 2);
    }

    #[test]
    fn test_list_enabled() {
        let store = ScheduleStore::new();

        let mut sched1 = create_test_schedule("sched_001", "wf_001");
        sched1.enabled = true;
        store.create(sched1).unwrap();

        let mut sched2 = create_test_schedule("sched_002", "wf_002");
        sched2.enabled = false;
        store.create(sched2).unwrap();

        let schedules = store.list_enabled().unwrap();
        assert_eq!(schedules.len(), 1);
        assert_eq!(schedules[0].schedule_id, "sched_001");
    }

    #[test]
    fn test_list_due() {
        let store = ScheduleStore::new();
        let now = Utc::now();

        // Schedule due now
        let mut sched1 = create_test_schedule("sched_001", "wf_001");
        sched1.set_next_run(Some(now - Duration::minutes(5)));
        store.create(sched1).unwrap();

        // Schedule due in future
        let mut sched2 = create_test_schedule("sched_002", "wf_002");
        sched2.set_next_run(Some(now + Duration::minutes(5)));
        store.create(sched2).unwrap();

        // Schedule with no next_run
        let sched3 = create_test_schedule("sched_003", "wf_003");
        store.create(sched3).unwrap();

        // Schedule due but disabled
        let mut sched4 = create_test_schedule("sched_004", "wf_004");
        sched4.set_next_run(Some(now - Duration::minutes(5)));
        sched4.enabled = false;
        store.create(sched4).unwrap();

        let due_schedules = store.list_due(now).unwrap();
        assert_eq!(due_schedules.len(), 1);
        assert_eq!(due_schedules[0].schedule_id, "sched_001");
    }

    #[test]
    fn test_count() {
        let store = ScheduleStore::new();

        assert_eq!(store.count().unwrap(), 0);

        store
            .create(create_test_schedule("sched_001", "wf_001"))
            .unwrap();
        assert_eq!(store.count().unwrap(), 1);

        store
            .create(create_test_schedule("sched_002", "wf_002"))
            .unwrap();
        assert_eq!(store.count().unwrap(), 2);

        store.delete("sched_001").unwrap();
        assert_eq!(store.count().unwrap(), 1);
    }

    #[test]
    fn test_exists() {
        let store = ScheduleStore::new();

        assert!(!store.exists("sched_001").unwrap());

        store
            .create(create_test_schedule("sched_001", "wf_001"))
            .unwrap();
        assert!(store.exists("sched_001").unwrap());

        store.delete("sched_001").unwrap();
        assert!(!store.exists("sched_001").unwrap());
    }

    #[test]
    fn test_concurrent_access() {
        use std::thread;

        let store = ScheduleStore::new();

        // Create schedules concurrently
        let handles: Vec<_> = (0..10)
            .map(|i| {
                let store_clone = store.clone();
                thread::spawn(move || {
                    let schedule =
                        create_test_schedule(&format!("sched_{:03}", i), &format!("wf_{:03}", i));
                    store_clone.create(schedule)
                })
            })
            .collect();

        // Wait for all threads
        for handle in handles {
            handle.join().unwrap().unwrap();
        }

        // Verify all schedules created
        assert_eq!(store.count().unwrap(), 10);
    }
}
