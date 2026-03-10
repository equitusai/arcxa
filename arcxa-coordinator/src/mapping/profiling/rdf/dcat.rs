//! DCAT (Data Catalog Vocabulary) Serialization
//!
//! Converts profiling results to RDF using W3C DCAT vocabulary.
//! https://www.w3.org/TR/vocab-dcat-2/

use crate::mapping::profiling::types::{ColumnProfile, DatasetUri, ProfileResult};
use anyhow::Result;

/// Serializer for DCAT/VoID RDF output
pub struct DcatSerializer;

impl Default for DcatSerializer {
    fn default() -> Self {
        Self
    }
}

impl DcatSerializer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Serialize profile to Turtle RDF format
    pub fn serialize(&self, profile: &ProfileResult, dataset_uri: &DatasetUri) -> Result<String> {
        let mut turtle = String::new();

        // Prefixes
        turtle.push_str("@prefix dcat: <http://www.w3.org/ns/dcat#> .\n");
        turtle.push_str("@prefix void: <http://rdfs.org/ns/void#> .\n");
        turtle.push_str("@prefix dcterms: <http://purl.org/dc/terms/> .\n");
        turtle.push_str("@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .\n");
        turtle.push_str("@prefix gph: <http://graphica.io/ontology#> .\n");
        turtle.push_str("@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n");
        turtle.push_str("@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n");
        turtle.push_str("\n");

        // Dataset declaration
        turtle.push_str(&format!("<{}> a dcat:Dataset ;\n", dataset_uri.uri));
        turtle.push_str(&format!("    dcterms:title \"{}\" ;\n", profile.dataset_id));
        turtle.push_str(&format!(
            "    dcat:byteSize {} ;\n",
            profile.file_size_bytes
        ));
        turtle.push_str(&format!("    gph:format \"{}\" ;\n", profile.format));
        turtle.push_str(&format!(
            "    gph:sourceLocation \"{}\" ;\n",
            profile.source_location
        ));
        turtle.push_str(&format!(
            "    gph:rowsProfiled {} ;\n",
            profile.rows_profiled
        ));
        turtle.push_str(&format!("    gph:columnCount {} ;\n", profile.column_count));
        turtle.push_str(&format!(
            "    dcterms:created \"{}\"^^xsd:dateTime ;\n",
            profile.profiled_at.to_rfc3339()
        ));

        if let Some(total) = profile.total_rows {
            turtle.push_str(&format!("    void:entities {} ;\n", total));
        }

        // Candidate keys
        if !profile.candidate_keys.is_empty() {
            turtle.push_str("    gph:candidateKeys ( ");
            for key in &profile.candidate_keys {
                turtle.push_str(&format!("\"{}\" ", key));
            }
            turtle.push_str(") ;\n");
        }

        // Column references
        for (idx, column) in profile.columns.iter().enumerate() {
            let col_uri = format!(
                "{}/column/{}",
                dataset_uri.uri,
                column.name.replace(" ", "_")
            );
            if idx == profile.columns.len() - 1 {
                turtle.push_str(&format!("    gph:hasColumn <{}> .\n", col_uri));
            } else {
                turtle.push_str(&format!("    gph:hasColumn <{}> ;\n", col_uri));
            }
        }

        turtle.push_str("\n");

        // Column definitions
        for column in &profile.columns {
            turtle.push_str(&self.serialize_column(column, &dataset_uri.uri)?);
            turtle.push_str("\n");
        }

        Ok(turtle)
    }

    /// Serialize a single column profile
    fn serialize_column(&self, column: &ColumnProfile, dataset_uri: &str) -> Result<String> {
        let mut turtle = String::new();
        let col_uri = format!("{}/column/{}", dataset_uri, column.name.replace(" ", "_"));

        turtle.push_str(&format!("<{}> a gph:Column ;\n", col_uri));
        turtle.push_str(&format!("    gph:columnName \"{}\" ;\n", column.name));
        turtle.push_str(&format!("    gph:columnIndex {} ;\n", column.index));
        turtle.push_str(&format!(
            "    gph:dataType <{}> ;\n",
            column.data_type.to_xsd_uri()
        ));
        turtle.push_str(&format!("    gph:nullCount {} ;\n", column.null_count));
        turtle.push_str(&format!(
            "    gph:nullPercentage \"{}\"^^xsd:decimal ;\n",
            column.null_percentage
        ));
        turtle.push_str(&format!(
            "    void:distinctValues {} ;\n",
            column.distinct_count
        ));
        turtle.push_str(&format!(
            "    gph:cardinality \"{}\"^^xsd:decimal ;\n",
            column.cardinality
        ));

        // Numeric statistics
        if let Some(min) = &column.min_value {
            turtle.push_str(&format!("    gph:minValue \"{}\" ;\n", min));
        }
        if let Some(max) = &column.max_value {
            turtle.push_str(&format!("    gph:maxValue \"{}\" ;\n", max));
        }
        if let Some(mean) = column.mean {
            turtle.push_str(&format!("    gph:mean \"{}\"^^xsd:decimal ;\n", mean));
        }
        if let Some(median) = column.median {
            turtle.push_str(&format!("    gph:median \"{}\"^^xsd:decimal ;\n", median));
        }
        if let Some(std_dev) = column.std_dev {
            turtle.push_str(&format!("    gph:stdDev \"{}\"^^xsd:decimal ;\n", std_dev));
        }

        // String statistics
        if let Some(min_len) = column.min_length {
            turtle.push_str(&format!("    gph:minLength {} ;\n", min_len));
        }
        if let Some(max_len) = column.max_length {
            turtle.push_str(&format!("    gph:maxLength {} ;\n", max_len));
        }
        if let Some(avg_len) = column.avg_length {
            turtle.push_str(&format!(
                "    gph:avgLength \"{}\"^^xsd:decimal ;\n",
                avg_len
            ));
        }

        // Pattern information
        if let Some(pattern) = &column.pattern_example {
            turtle.push_str(&format!("    gph:patternExample \"{}\" ;\n", pattern));
        }
        if let Some(regex) = &column.pattern_regex {
            turtle.push_str(&format!("    gph:patternRegex \"{}\" ;\n", regex));
        }

        // Top values (frequency distribution)
        if !column.top_values.is_empty() {
            turtle.push_str("    gph:topValues (\n");
            for (idx, val_freq) in column.top_values.iter().enumerate() {
                let is_last = idx == column.top_values.len() - 1;
                turtle.push_str(&format!(
                    "        [ gph:value \"{}\" ; gph:count {} ; gph:percentage \"{}\"^^xsd:decimal ]{}",
                    val_freq.value.replace("\"", "\\\""),
                    val_freq.count,
                    val_freq.percentage,
                    if is_last { "" } else { " ;" }
                ));
                turtle.push_str("\n");
            }
            turtle.push_str("    ) .\n");
        } else {
            turtle.push_str("    gph:topValues () .\n");
        }

        Ok(turtle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapping::profiling::types::{DataType, ValueFrequency};
    use std::path::PathBuf;

    #[test]
    fn test_dcat_serialization() {
        let profile = ProfileResult {
            dataset_id: "test_dataset".to_string(),
            source_location: "/data/test.csv".to_string(),
            format: "csv".to_string(),
            file_size_bytes: 1024,
            total_rows: Some(100),
            rows_profiled: 100,
            column_count: 2,
            columns: vec![ColumnProfile {
                name: "id".to_string(),
                index: 0,
                data_type: DataType::Integer,
                semantic_type: None,
                null_count: 0,
                null_percentage: 0.0,
                distinct_count: 100,
                cardinality: 1.0,
                min_value: Some("1".to_string()),
                max_value: Some("100".to_string()),
                mean: Some(50.5),
                median: Some(50.0),
                std_dev: Some(28.9),
                min_length: None,
                max_length: None,
                avg_length: None,
                pattern_example: None,
                pattern_regex: None,
                top_values: vec![],
            }],
            candidate_keys: vec!["id".to_string()],
            profiled_at: chrono::Utc::now(),
            duration_seconds: 1.5,
        };

        let dataset_uri = DatasetUri::from_path(&PathBuf::from("/data/test.csv"));
        let serializer = DcatSerializer::new();
        let turtle = serializer.serialize(&profile, &dataset_uri).unwrap();

        // Verify key elements are present
        assert!(turtle.contains("@prefix dcat:"));
        assert!(turtle.contains("dcat:Dataset"));
        assert!(turtle.contains("gph:hasColumn"));
        assert!(turtle.contains("gph:columnName \"id\""));
        assert!(turtle.contains("void:distinctValues 100"));
    }
}
