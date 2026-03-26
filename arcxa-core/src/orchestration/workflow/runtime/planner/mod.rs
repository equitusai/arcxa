//! Planner submodule placeholder for pushdown and execution planning.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PushdownCapability {
    Projection,
    Filter,
    Limit,
    Aggregate,
}
