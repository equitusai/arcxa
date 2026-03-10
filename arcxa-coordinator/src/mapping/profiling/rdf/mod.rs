//! RDF Serialization for Dataset Profiles
//!
//! Converts profiling results to RDF triples using DCAT and VoID vocabularies.

pub mod dcat;
pub mod void_vocab;

pub use dcat::DcatSerializer;
