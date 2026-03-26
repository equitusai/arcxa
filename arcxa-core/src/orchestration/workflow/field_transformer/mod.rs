mod legacy;
mod operations;
mod rows;

pub(crate) use legacy::execute_legacy_object_transform;
pub(crate) use rows::{
    transform_object_rows, transform_object_rows_with_metadata, FieldModificationSummary,
    RowTransformationStats,
};
