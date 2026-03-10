//! Plan cache for transformation engine
//!
//! Caches compiled execution plans to avoid re-parsing and re-compiling.

use lru::LruCache;
use parking_lot::RwLock;
use std::num::NonZeroUsize;
use std::sync::Arc;

use super::executor::ExecutionPlan;

/// Thread-safe LRU cache for execution plans
pub struct PlanCache {
    cache: Arc<RwLock<LruCache<String, ExecutionPlan>>>,
}

impl PlanCache {
    /// Create a new plan cache with given capacity
    pub fn new(capacity: usize) -> Self {
        let capacity =
            NonZeroUsize::new(capacity).unwrap_or_else(|| NonZeroUsize::new(100).unwrap());
        Self {
            cache: Arc::new(RwLock::new(LruCache::new(capacity))),
        }
    }

    /// Get a plan from the cache
    pub fn get(&self, expression: &str) -> Option<ExecutionPlan> {
        self.cache.write().get(expression).cloned()
    }

    /// Insert a plan into the cache
    pub fn insert(&self, expression: String, plan: ExecutionPlan) {
        self.cache.write().put(expression, plan);
    }

    /// Clear the cache
    pub fn clear(&self) {
        self.cache.write().clear();
    }

    /// Get cache size
    pub fn len(&self) -> usize {
        self.cache.read().len()
    }

    /// Check if cache is empty
    pub fn is_empty(&self) -> bool {
        self.cache.read().is_empty()
    }

    /// Get cache capacity
    pub fn capacity(&self) -> usize {
        self.cache.read().cap().get()
    }
}

impl Default for PlanCache {
    fn default() -> Self {
        Self::new(1000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapping::loader::transformation::ast::Expression;
    use crate::mapping::loader::transformation::functions::FunctionRegistry;

    #[test]
    fn test_cache_basic_operations() {
        let cache = PlanCache::new(10);
        assert_eq!(cache.len(), 0);
        assert!(cache.is_empty());

        // Create a dummy plan
        let expr = Expression::Variable("test".to_string());
        let functions = Arc::new(FunctionRegistry::with_builtins());
        let plan = ExecutionPlan::from_ast(expr, &functions).unwrap();

        // Insert and retrieve
        cache.insert("test_expr".to_string(), plan.clone());
        assert_eq!(cache.len(), 1);
        assert!(!cache.is_empty());

        let retrieved = cache.get("test_expr");
        assert!(retrieved.is_some());

        // Non-existent key
        assert!(cache.get("non_existent").is_none());

        // Clear cache
        cache.clear();
        assert_eq!(cache.len(), 0);
        assert!(cache.is_empty());
    }

    #[test]
    fn test_cache_lru_eviction() {
        let cache = PlanCache::new(2);

        let expr = Expression::Variable("test".to_string());
        let functions = Arc::new(FunctionRegistry::with_builtins());
        let plan = ExecutionPlan::from_ast(expr, &functions).unwrap();

        // Fill cache to capacity
        cache.insert("expr1".to_string(), plan.clone());
        cache.insert("expr2".to_string(), plan.clone());
        assert_eq!(cache.len(), 2);

        // Add third item, should evict first
        cache.insert("expr3".to_string(), plan.clone());
        assert_eq!(cache.len(), 2);
        assert!(cache.get("expr1").is_none()); // Evicted
        assert!(cache.get("expr2").is_some());
        assert!(cache.get("expr3").is_some());
    }
}
