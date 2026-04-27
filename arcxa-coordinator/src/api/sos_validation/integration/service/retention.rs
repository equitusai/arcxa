use super::*;

const DEFAULT_MAX_REPORTS_PER_SUBJECT: usize = 1_000;

#[derive(Debug, Clone)]
pub(super) struct ValidationReportRetentionConfig {
    pub pruning_enabled: bool,
    pub max_reports_per_subject: usize,
    pub max_report_age_days: Option<i64>,
}

impl Default for ValidationReportRetentionConfig {
    fn default() -> Self {
        Self {
            pruning_enabled: true,
            max_reports_per_subject: DEFAULT_MAX_REPORTS_PER_SUBJECT,
            max_report_age_days: None,
        }
    }
}

impl ValidationReportRetentionConfig {
    pub(super) fn from_env() -> Self {
        Self {
            pruning_enabled: parse_bool_env("SOS_VALIDATION_REPORT_PRUNING_ENABLED", true),
            max_reports_per_subject: parse_usize_env(
                "SOS_VALIDATION_REPORT_RETENTION_PER_SUBJECT",
                DEFAULT_MAX_REPORTS_PER_SUBJECT,
            )
            .max(1),
            max_report_age_days: parse_i64_env("SOS_VALIDATION_REPORT_RETENTION_DAYS")
                .filter(|days| *days > 0),
        }
    }
}

pub(super) fn prune_after_persist(
    service: &SosValidationService,
    report: &ValidationReport,
) -> Result<Vec<ValidationReport>, SosValidationServiceError> {
    if !service.retention_config.pruning_enabled {
        return Ok(Vec::new());
    }

    let cutoff = service
        .retention_config
        .max_report_age_days
        .and_then(|days| Utc::now().checked_sub_signed(chrono::Duration::days(days)));

    service
        .storage_manager
        .prune_validation_reports_by_subject(
            &report.subject_key,
            service.retention_config.max_reports_per_subject,
            cutoff,
        )
        .map_err(map_storage_error)
}

fn parse_bool_env(name: &str, default: bool) -> bool {
    match std::env::var(name) {
        Ok(value) => match value.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => true,
            "0" | "false" | "no" | "off" => false,
            _ => default,
        },
        Err(_) => default,
    }
}

fn parse_usize_env(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|value| value.trim().parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

fn parse_i64_env(name: &str) -> Option<i64> {
    std::env::var(name)
        .ok()
        .and_then(|value| value.trim().parse::<i64>().ok())
}
