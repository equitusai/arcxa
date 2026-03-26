//! Runtime lineage cost-control primitives.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RuntimeLineageMode {
    Full,
    Batched,
    Sampled,
    Minimal,
}
