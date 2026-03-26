//! Batch-oriented operator surface placeholder.

mod aggregator;
mod csv_export;
mod data_validator;
mod deduplicator;
mod field_transformer;
mod semantic_mapper;

pub trait RuntimeOperator {
    fn name(&self) -> &'static str;
}

pub use aggregator::AggregatorBatchOperator;
pub use csv_export::CsvExportBatchOperator;
pub use data_validator::{DataValidatorBatchOperator, DataValidatorBatchResult};
pub use deduplicator::DeduplicatorBatchOperator;
pub use field_transformer::{FieldTransformerBatchOperator, FieldTransformerBatchResult};
pub use semantic_mapper::SemanticMapperBatchOperator;
