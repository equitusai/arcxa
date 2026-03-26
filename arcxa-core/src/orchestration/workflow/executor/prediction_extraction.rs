use super::super::lineage_tracker::PredictionRecord;
use super::{StepConfig, WorkflowExecutor};

/// Helper struct for extracted predictions.
pub(super) struct ExtractedPredictions {
    pub(super) model_id: String,
    pub(super) model_version: String,
    pub(super) predictions: Vec<PredictionRecord>,
}

impl WorkflowExecutor {
    /// Extract predictions from ML step output for lineage tracking.
    pub(super) fn extract_predictions(
        &self,
        output: &serde_json::Value,
        config: &StepConfig,
    ) -> Option<ExtractedPredictions> {
        let (model_id, model_version) = match config {
            StepConfig::MLPrediction(cfg) => (cfg.model_id.clone(), cfg.model_version.clone()),
            _ => return None,
        };

        let predictions_array = output.get("_predictions")?.as_array()?;

        let mut predictions = Vec::new();
        for prediction in predictions_array {
            if let Some(attribute_name) = prediction
                .get("attribute_name")
                .and_then(|value| value.as_str())
            {
                predictions.push(PredictionRecord {
                    attribute_name: attribute_name.to_string(),
                    value: prediction
                        .get("value")
                        .cloned()
                        .unwrap_or(serde_json::json!(null)),
                    confidence: prediction
                        .get("confidence")
                        .and_then(|value| value.as_f64())
                        .unwrap_or(0.0),
                });
            }
        }

        if predictions.is_empty() {
            None
        } else {
            Some(ExtractedPredictions {
                model_id,
                model_version,
                predictions,
            })
        }
    }
}
