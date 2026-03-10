//! Format Converters for GDPR Export
//!
//! Converts discovered personal data into various export formats:
//! - JSON: Structured, machine-readable format
//! - CSV: Tabular format for spreadsheet applications
//! - XML: Hierarchical, standardized format
//! - PDF: Human-readable document format (placeholder - requires additional deps)
//!
//! ## Design
//!
//! Each converter takes DiscoveryResult and ExportRequest, then produces
//! formatted output as bytes. The converters are stateless and can be used
//! concurrently.

use super::discovery::{DataReference, DiscoveryResult};
use super::types::{DataCategory, ExportFormat, ExportRequest};
use anyhow::{Context, Result};
use serde::Serialize;
use std::collections::HashMap;
use std::io::Write;

/// Export data package - contains all data organized by category
#[derive(Debug, Clone, Serialize)]
pub struct ExportPackage {
    /// Export metadata
    pub metadata: ExportMetadata,

    /// Data organized by category
    pub data_by_category: HashMap<String, Vec<DataItem>>,

    /// Summary statistics
    pub summary: ExportSummary,
}

/// Metadata about the export
#[derive(Debug, Clone, Serialize)]
pub struct ExportMetadata {
    /// User ID this export is for
    pub user_id: String,

    /// Export format
    pub format: String,

    /// Export timestamp
    pub exported_at: String,

    /// Time range (if specified)
    pub time_range: Option<TimeRangeInfo>,

    /// Total items included
    pub total_items: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct TimeRangeInfo {
    pub start: String,
    pub end: String,
}

/// Individual data item in export
#[derive(Debug, Clone, Serialize)]
pub struct DataItem {
    /// Item ID
    pub id: String,

    /// Data type (e.g., "lineage_event", "rdf_triple")
    pub data_type: String,

    /// Category
    pub category: String,

    /// Timestamp
    pub timestamp: String,

    /// Storage location
    pub storage_location: String,

    /// Size in bytes (if available)
    pub size_bytes: Option<usize>,

    /// Additional metadata
    pub metadata: HashMap<String, String>,
}

/// Export summary statistics
#[derive(Debug, Clone, Serialize)]
pub struct ExportSummary {
    /// Items per category
    pub items_by_category: HashMap<String, usize>,

    /// Total size estimate
    pub estimated_size_bytes: usize,

    /// Any warnings
    pub warnings: Vec<String>,
}

/// Format converter service
pub struct FormatConverter;

impl FormatConverter {
    /// Create new format converter
    pub fn new() -> Self {
        Self
    }

    /// Convert discovery result to specified format
    pub fn convert(&self, discovery: &DiscoveryResult, request: &ExportRequest) -> Result<Vec<u8>> {
        match request.format {
            ExportFormat::Json => self.convert_to_json(discovery, request),
            ExportFormat::Csv => self.convert_to_csv(discovery, request),
            ExportFormat::Xml => self.convert_to_xml(discovery, request),
            ExportFormat::Pdf => self.convert_to_pdf(discovery, request),
        }
    }

    /// Convert to JSON format
    fn convert_to_json(
        &self,
        discovery: &DiscoveryResult,
        request: &ExportRequest,
    ) -> Result<Vec<u8>> {
        let package = self.build_export_package(discovery, request)?;

        let json = if request.include_metadata {
            // Pretty-printed JSON for readability
            serde_json::to_vec_pretty(&package)
                .context("Failed to serialize export package to JSON")?
        } else {
            // Compact JSON
            serde_json::to_vec(&package).context("Failed to serialize export package to JSON")?
        };

        Ok(json)
    }

    /// Convert to CSV format
    fn convert_to_csv(
        &self,
        discovery: &DiscoveryResult,
        request: &ExportRequest,
    ) -> Result<Vec<u8>> {
        let mut writer = csv::Writer::from_writer(vec![]);

        // Write header
        writer
            .write_record(&[
                "ID",
                "Category",
                "Data Type",
                "Timestamp",
                "Storage Location",
                "Size (bytes)",
                "Metadata",
            ])
            .context("Failed to write CSV header")?;

        // Write data rows
        for (category, items) in &discovery.items_by_category {
            for item in items {
                let metadata_json =
                    serde_json::to_string(&item.metadata).unwrap_or_else(|_| "{}".to_string());

                writer
                    .write_record(&[
                        &item.id,
                        &format!("{:?}", category),
                        &item.data_type,
                        &item.timestamp.to_rfc3339(),
                        &item.storage_location,
                        &item.size_bytes.map(|s| s.to_string()).unwrap_or_default(),
                        &metadata_json,
                    ])
                    .context("Failed to write CSV record")?;
            }
        }

        let csv_data = writer
            .into_inner()
            .context("Failed to finalize CSV output")?;

        Ok(csv_data)
    }

    /// Convert to XML format
    fn convert_to_xml(
        &self,
        discovery: &DiscoveryResult,
        request: &ExportRequest,
    ) -> Result<Vec<u8>> {
        let mut xml = String::new();

        // XML declaration
        xml.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
        xml.push_str("<export>\n");

        // Metadata
        xml.push_str("  <metadata>\n");
        xml.push_str(&format!("    <user_id>{}</user_id>\n", discovery.user_id));
        xml.push_str(&format!("    <format>XML</format>\n"));
        xml.push_str(&format!(
            "    <exported_at>{}</exported_at>\n",
            discovery.discovered_at.to_rfc3339()
        ));
        xml.push_str(&format!(
            "    <total_items>{}</total_items>\n",
            discovery.total_items
        ));
        xml.push_str("  </metadata>\n");

        // Data by category
        xml.push_str("  <data>\n");
        for (category, items) in &discovery.items_by_category {
            xml.push_str(&format!("    <category name=\"{:?}\">\n", category));

            for item in items {
                xml.push_str("      <item>\n");
                xml.push_str(&format!("        <id>{}</id>\n", item.id));
                xml.push_str(&format!(
                    "        <data_type>{}</data_type>\n",
                    item.data_type
                ));
                xml.push_str(&format!(
                    "        <timestamp>{}</timestamp>\n",
                    item.timestamp.to_rfc3339()
                ));
                xml.push_str(&format!(
                    "        <storage_location>{}</storage_location>\n",
                    Self::xml_escape(&item.storage_location)
                ));

                if let Some(size) = item.size_bytes {
                    xml.push_str(&format!("        <size_bytes>{}</size_bytes>\n", size));
                }

                if !item.metadata.is_empty() {
                    xml.push_str("        <metadata>\n");
                    for (key, value) in &item.metadata {
                        xml.push_str(&format!(
                            "          <{}>{}</{}>\n",
                            key,
                            Self::xml_escape(value),
                            key
                        ));
                    }
                    xml.push_str("        </metadata>\n");
                }

                xml.push_str("      </item>\n");
            }

            xml.push_str("    </category>\n");
        }
        xml.push_str("  </data>\n");

        // Summary
        xml.push_str("  <summary>\n");
        xml.push_str(&format!(
            "    <estimated_size_bytes>{}</estimated_size_bytes>\n",
            discovery.estimated_size_bytes()
        ));

        if !discovery.warnings.is_empty() {
            xml.push_str("    <warnings>\n");
            for warning in &discovery.warnings {
                xml.push_str(&format!(
                    "      <warning>{}</warning>\n",
                    Self::xml_escape(warning)
                ));
            }
            xml.push_str("    </warnings>\n");
        }

        xml.push_str("  </summary>\n");
        xml.push_str("</export>\n");

        Ok(xml.into_bytes())
    }

    /// Convert to PDF format (placeholder)
    ///
    /// PDF generation requires additional dependencies like printpdf or genpdf.
    /// For now, this returns a text-based representation.
    fn convert_to_pdf(
        &self,
        discovery: &DiscoveryResult,
        request: &ExportRequest,
    ) -> Result<Vec<u8>> {
        // TODO: Implement actual PDF generation with printpdf or genpdf
        // For now, return a formatted text document

        let mut text = String::new();

        text.push_str("==================================================\n");
        text.push_str("         GDPR DATA EXPORT REPORT\n");
        text.push_str("==================================================\n\n");

        text.push_str(&format!("User ID: {}\n", discovery.user_id));
        text.push_str(&format!(
            "Export Date: {}\n",
            discovery.discovered_at.to_rfc3339()
        ));
        text.push_str(&format!("Total Items: {}\n", discovery.total_items));
        text.push_str(&format!(
            "Estimated Size: {} bytes\n\n",
            discovery.estimated_size_bytes()
        ));

        if let Some((start, end)) = discovery.time_range {
            text.push_str(&format!(
                "Time Range: {} to {}\n\n",
                start.to_rfc3339(),
                end.to_rfc3339()
            ));
        }

        text.push_str("--------------------------------------------------\n");
        text.push_str("DATA BY CATEGORY\n");
        text.push_str("--------------------------------------------------\n\n");

        for (category, items) in &discovery.items_by_category {
            text.push_str(&format!("Category: {:?}\n", category));
            text.push_str(&format!("Items: {}\n\n", items.len()));

            for (i, item) in items.iter().enumerate() {
                text.push_str(&format!("  {}. ID: {}\n", i + 1, item.id));
                text.push_str(&format!("     Type: {}\n", item.data_type));
                text.push_str(&format!(
                    "     Timestamp: {}\n",
                    item.timestamp.to_rfc3339()
                ));
                text.push_str(&format!("     Location: {}\n", item.storage_location));

                if let Some(size) = item.size_bytes {
                    text.push_str(&format!("     Size: {} bytes\n", size));
                }

                text.push_str("\n");
            }
        }

        if !discovery.warnings.is_empty() {
            text.push_str("--------------------------------------------------\n");
            text.push_str("WARNINGS\n");
            text.push_str("--------------------------------------------------\n\n");
            for warning in &discovery.warnings {
                text.push_str(&format!("  - {}\n", warning));
            }
        }

        text.push_str("\n==================================================\n");
        text.push_str("            END OF REPORT\n");
        text.push_str("==================================================\n");

        Ok(text.into_bytes())
    }

    /// Build export package from discovery result
    fn build_export_package(
        &self,
        discovery: &DiscoveryResult,
        request: &ExportRequest,
    ) -> Result<ExportPackage> {
        let mut data_by_category = HashMap::new();

        for (category, items) in &discovery.items_by_category {
            let category_str = format!("{:?}", category);
            let converted_items: Vec<DataItem> = items
                .iter()
                .map(|item| DataItem {
                    id: item.id.clone(),
                    data_type: item.data_type.clone(),
                    category: category_str.clone(),
                    timestamp: item.timestamp.to_rfc3339(),
                    storage_location: item.storage_location.clone(),
                    size_bytes: item.size_bytes,
                    metadata: item.metadata.clone(),
                })
                .collect();

            data_by_category.insert(category_str, converted_items);
        }

        let metadata = ExportMetadata {
            user_id: discovery.user_id.clone(),
            format: format!("{:?}", request.format),
            exported_at: discovery.discovered_at.to_rfc3339(),
            time_range: discovery.time_range.map(|(start, end)| TimeRangeInfo {
                start: start.to_rfc3339(),
                end: end.to_rfc3339(),
            }),
            total_items: discovery.total_items,
        };

        let items_by_category: HashMap<String, usize> = discovery
            .category_counts()
            .into_iter()
            .map(|(cat, count)| (format!("{:?}", cat), count))
            .collect();

        let summary = ExportSummary {
            items_by_category,
            estimated_size_bytes: discovery.estimated_size_bytes(),
            warnings: discovery.warnings.clone(),
        };

        Ok(ExportPackage {
            metadata,
            data_by_category,
            summary,
        })
    }

    /// Escape XML special characters
    fn xml_escape(s: &str) -> String {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
            .replace('\'', "&apos;")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gdpr::export::types::TimeRange;
    use chrono::Utc;
    use std::collections::HashMap;

    fn create_test_discovery() -> DiscoveryResult {
        let mut items_by_category = HashMap::new();

        let data_ref = DataReference {
            id: "test-id-123".to_string(),
            data_type: "lineage_event".to_string(),
            category: DataCategory::Behavioral,
            storage_location: "rocksdb://lineage/events/123".to_string(),
            timestamp: Utc::now(),
            size_bytes: Some(1024),
            metadata: {
                let mut m = HashMap::new();
                m.insert("dataset".to_string(), "test.dataset".to_string());
                m
            },
        };

        items_by_category.insert(DataCategory::Behavioral, vec![data_ref]);

        DiscoveryResult {
            user_id: "alice".to_string(),
            total_items: 1,
            items_by_category,
            discovered_at: Utc::now(),
            time_range: None,
            warnings: vec![],
        }
    }

    fn create_test_request(format: ExportFormat) -> ExportRequest {
        ExportRequest {
            user_id: "alice".to_string(),
            format,
            categories: vec![],
            include_derived: false,
            include_metadata: true,
            include_audit_trail: false,
            time_range: None,
            filters: HashMap::new(),
        }
    }

    #[test]
    fn test_convert_to_json() {
        let converter = FormatConverter::new();
        let discovery = create_test_discovery();
        let request = create_test_request(ExportFormat::Json);

        let result = converter.convert(&discovery, &request).unwrap();

        // Should be valid JSON
        let parsed: serde_json::Value = serde_json::from_slice(&result).unwrap();
        assert!(parsed["metadata"]["user_id"].as_str().unwrap() == "alice");
        assert!(parsed["data_by_category"].is_object());
    }

    #[test]
    fn test_convert_to_csv() {
        let converter = FormatConverter::new();
        let discovery = create_test_discovery();
        let request = create_test_request(ExportFormat::Csv);

        let result = converter.convert(&discovery, &request).unwrap();

        // Should contain CSV header
        let csv_str = String::from_utf8(result).unwrap();
        assert!(csv_str.contains("ID,Category,Data Type"));
        assert!(csv_str.contains("test-id-123"));
        assert!(csv_str.contains("lineage_event"));
    }

    #[test]
    fn test_convert_to_xml() {
        let converter = FormatConverter::new();
        let discovery = create_test_discovery();
        let request = create_test_request(ExportFormat::Xml);

        let result = converter.convert(&discovery, &request).unwrap();

        // Should be valid XML
        let xml_str = String::from_utf8(result).unwrap();
        assert!(xml_str.starts_with("<?xml"));
        assert!(xml_str.contains("<export>"));
        assert!(xml_str.contains("<user_id>alice</user_id>"));
        assert!(xml_str.contains("<id>test-id-123</id>"));
        assert!(xml_str.contains("</export>"));
    }

    #[test]
    fn test_convert_to_pdf() {
        let converter = FormatConverter::new();
        let discovery = create_test_discovery();
        let request = create_test_request(ExportFormat::Pdf);

        let result = converter.convert(&discovery, &request).unwrap();

        // Should be text-based PDF placeholder
        let text = String::from_utf8(result).unwrap();
        assert!(text.contains("GDPR DATA EXPORT REPORT"));
        assert!(text.contains("User ID: alice"));
        assert!(text.contains("Category: Behavioral"));
    }

    #[test]
    fn test_xml_escape() {
        let escaped = FormatConverter::xml_escape("<test & 'value'>");
        assert_eq!(escaped, "&lt;test &amp; &apos;value&apos;&gt;");
    }
}
