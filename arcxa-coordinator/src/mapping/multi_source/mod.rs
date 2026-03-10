//! Unified Mapping Module
//!
//! This module provides functionality for consolidating multiple source
//! mapping sessions into a single unified mapping that targets a normalized
//! relational database schema.
//!
//! ## Architecture
//!
//! The unified mapping workflow consists of:
//! 1. Core types - Data structures for unified sessions
//! 2. Storage layer - Persistence using RocksDB
//! 3. Coordinator - Orchestration logic for consolidation
//! 4. Conflict resolution - Handling field mapping conflicts
//!
//! ## Module Organization
//!
//! - `types` - Core data structures
//! - `storage` - Persistence layer
//! - `coordinator` - Main orchestration logic
//! - `conflict` - Conflict detection and resolution

pub mod conflict;
pub mod coordinator;
pub mod storage;
pub mod types;

pub use types::{
    ConflictResolution, ForeignKeyConfig, MappingConflict, SourceFieldRef, TargetColumnConfig,
    TargetColumnRef, TargetDatabaseConfig, TargetTableConfig, UnifiedFieldMapping, UnifiedLoadJob,
    UnifiedLoadJobStatus, UnifiedLoadProgress, UnifiedMappingSession, UnifiedSessionStatus,
};

pub use storage::{StorageStatistics, UnifiedMappingStorage};

pub use coordinator::{
    CreateUnifiedSessionRequest, CreateUnifiedSessionResponse, UnifiedMappingCoordinator,
};

pub use conflict::{ConflictResolver, ResolvedValue, SourceValue};
