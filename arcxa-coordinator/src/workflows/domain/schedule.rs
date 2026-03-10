//! Workflow Schedule Domain Model
//!
//! Manages workflow scheduling with cron expressions and timezone support.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Workflow schedule configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowSchedule {
    /// Unique schedule ID
    pub schedule_id: String,

    /// Associated workflow ID
    pub workflow_id: String,

    /// Workflow name (cached for display)
    pub workflow_name: String,

    /// Cron expression (e.g., "0 0 * * *") - mutually exclusive with interval_seconds
    pub cron_expression: Option<String>,

    /// Interval in seconds for recurring execution - mutually exclusive with cron_expression
    pub interval_seconds: Option<u64>,

    /// One-time scheduled execution time (for non-recurring schedules)
    pub scheduled_at: Option<DateTime<Utc>>,

    /// IANA timezone (e.g., "America/New_York")
    pub timezone: String,

    /// Input data to pass to workflow execution
    pub input: serde_json::Value,

    /// Execution context metadata
    pub context: serde_json::Value,

    /// Whether schedule is enabled
    pub enabled: bool,

    /// When schedule was created
    pub created_at: DateTime<Utc>,

    /// When schedule was last updated
    pub updated_at: DateTime<Utc>,

    /// Next scheduled execution time (UTC)
    pub next_run: Option<DateTime<Utc>>,

    /// Last execution time (UTC)
    pub last_run: Option<DateTime<Utc>>,

    /// Number of times this schedule has executed
    pub execution_count: i32,
}

impl WorkflowSchedule {
    /// Create a new schedule
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        schedule_id: String,
        workflow_id: String,
        workflow_name: String,
        cron_expression: Option<String>,
        interval_seconds: Option<u64>,
        scheduled_at: Option<DateTime<Utc>>,
        timezone: String,
        input: serde_json::Value,
        context: serde_json::Value,
        enabled: bool,
    ) -> Self {
        let now = Utc::now();
        Self {
            schedule_id,
            workflow_id,
            workflow_name,
            cron_expression,
            interval_seconds,
            scheduled_at,
            timezone,
            input,
            context,
            enabled,
            created_at: now,
            updated_at: now,
            next_run: None,
            last_run: None,
            execution_count: 0,
        }
    }

    /// Update schedule configuration
    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        cron_expression: Option<String>,
        interval_seconds: Option<u64>,
        scheduled_at: Option<DateTime<Utc>>,
        timezone: String,
        input: Option<serde_json::Value>,
        context: Option<serde_json::Value>,
        enabled: bool,
    ) {
        self.cron_expression = cron_expression;
        self.interval_seconds = interval_seconds;
        self.scheduled_at = scheduled_at;
        self.timezone = timezone;
        if let Some(input) = input {
            self.input = input;
        }
        if let Some(context) = context {
            self.context = context;
        }
        self.enabled = enabled;
        self.updated_at = Utc::now();
    }

    /// Set next run time
    pub fn set_next_run(&mut self, next_run: Option<DateTime<Utc>>) {
        self.next_run = next_run;
        self.updated_at = Utc::now();
    }

    /// Record execution
    pub fn record_execution(&mut self, executed_at: DateTime<Utc>) {
        self.last_run = Some(executed_at);
        self.execution_count += 1;
        self.updated_at = Utc::now();
    }
}

/// Validate cron expression format
pub fn validate_cron_expression(expr: &str) -> Result<(), String> {
    // Basic validation: 6 fields separated by spaces (sec min hour day month dow)
    let parts: Vec<&str> = expr.split_whitespace().collect();

    if parts.len() != 6 {
        return Err(format!(
            "Invalid cron expression: expected 6 fields (sec min hour day month dow), got {}",
            parts.len()
        ));
    }

    // Validate each field has valid characters
    for (i, part) in parts.iter().enumerate() {
        if !is_valid_cron_field(part, i) {
            return Err(format!(
                "Invalid cron field at position {}: '{}'",
                i + 1,
                part
            ));
        }
    }

    Ok(())
}

/// Check if a cron field is valid
fn is_valid_cron_field(field: &str, position: usize) -> bool {
    // Allow: numbers, *, /, -, ,
    // Position meanings: 0=sec, 1=min, 2=hour, 3=day, 4=month, 5=dow

    if field == "*" || field == "?" {
        return true;
    }

    // Check for valid characters (allow letters for day-of-week names)
    for c in field.chars() {
        if !c.is_ascii_digit()
            && !c.is_ascii_alphabetic()
            && !matches!(c, '*' | '/' | '-' | ',' | '?')
        {
            return false;
        }
    }

    // Validate ranges based on position (6-field format: sec min hour day month dow)
    match position {
        0 => validate_range(field, 0, 59), // second
        1 => validate_range(field, 0, 59), // minute
        2 => validate_range(field, 0, 23), // hour
        3 => validate_range(field, 1, 31), // day of month
        4 => validate_range(field, 1, 12), // month
        5 => validate_dow_field(field),    // day of week (allow both numeric and text)
        _ => false,
    }
}

/// Validate day-of-week field (supports both numeric 0-7 and text MON-SUN)
fn validate_dow_field(field: &str) -> bool {
    // Handle wildcards
    if field == "*" || field == "?" {
        return true;
    }

    // Valid day-of-week names
    let valid_names = ["SUN", "MON", "TUE", "WED", "THU", "FRI", "SAT"];

    // Check if it's a text name or numeric value
    if field.chars().any(|c| c.is_ascii_alphabetic()) {
        // Text format - validate names
        // Handle ranges (MON-FRI)
        if field.contains('-') {
            let parts: Vec<&str> = field.split('-').collect();
            if parts.len() == 2 {
                return valid_names.contains(&parts[0]) && valid_names.contains(&parts[1]);
            }
            return false;
        }
        // Handle comma-separated (MON,WED,FRI)
        if field.contains(',') {
            return field
                .split(',')
                .all(|part| valid_names.contains(&part.trim()));
        }
        // Single name
        return valid_names.contains(&field);
    } else {
        // Numeric format - use standard validation
        return validate_range(field, 0, 7);
    }
}

/// Validate numeric range in cron field
fn validate_range(field: &str, min: u32, max: u32) -> bool {
    // Handle wildcards
    if field == "*" || field == "?" {
        return true;
    }

    // Handle step values (*/5, 0-30/5)
    if field.contains('/') {
        let parts: Vec<&str> = field.split('/').collect();
        if parts.len() != 2 {
            return false;
        }

        let step: u32 = match parts[1].parse() {
            Ok(n) => n,
            Err(_) => return false,
        };

        if step == 0 || step > max {
            return false;
        }

        // Validate base (before /)
        if parts[0] == "*" {
            return true;
        }
        return validate_range(parts[0], min, max);
    }

    // Handle ranges (0-30)
    if field.contains('-') {
        let parts: Vec<&str> = field.split('-').collect();
        if parts.len() != 2 {
            return false;
        }

        let start: u32 = match parts[0].parse() {
            Ok(n) => n,
            Err(_) => return false,
        };

        let end: u32 = match parts[1].parse() {
            Ok(n) => n,
            Err(_) => return false,
        };

        return start >= min && start <= max && end >= min && end <= max && start <= end;
    }

    // Handle comma-separated values (1,15,30)
    if field.contains(',') {
        return field
            .split(',')
            .all(|part| validate_range(part.trim(), min, max));
    }

    // Handle single number
    match field.parse::<u32>() {
        Ok(n) => n >= min && n <= max,
        Err(_) => false,
    }
}

/// Validate IANA timezone
pub fn validate_timezone(tz: &str) -> bool {
    // Common timezone validation
    // In production, use chrono_tz or similar for full validation

    // Basic format check
    if tz.is_empty() {
        return false;
    }

    // Allow UTC variants
    if tz == "UTC" || tz == "GMT" {
        return true;
    }

    // Allow standard IANA format (e.g., "America/New_York")
    if tz.contains('/') {
        let parts: Vec<&str> = tz.split('/').collect();
        if parts.len() >= 2 && !parts[0].is_empty() && !parts[1].is_empty() {
            return true;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_schedule() {
        let schedule = WorkflowSchedule::new(
            "sched_001".to_string(),
            "wf_001".to_string(),
            "Test Workflow".to_string(),
            Some("0 0 * * *".to_string()),
            None,
            None,
            "UTC".to_string(),
            serde_json::json!({}),
            serde_json::json!({}),
            true,
        );

        assert_eq!(schedule.schedule_id, "sched_001");
        assert_eq!(schedule.workflow_id, "wf_001");
        assert_eq!(schedule.cron_expression, Some("0 0 * * *".to_string()));
        assert_eq!(schedule.timezone, "UTC");
        assert!(schedule.enabled);
        assert!(schedule.next_run.is_none());
        assert!(schedule.last_run.is_none());
        assert_eq!(schedule.execution_count, 0);
    }

    #[test]
    fn test_update_schedule() {
        let mut schedule = WorkflowSchedule::new(
            "sched_001".to_string(),
            "wf_001".to_string(),
            "Test".to_string(),
            Some("0 0 * * *".to_string()),
            None,
            None,
            "UTC".to_string(),
            serde_json::json!({}),
            serde_json::json!({}),
            true,
        );

        let original_updated = schedule.updated_at;

        // Update schedule
        schedule.update(
            Some("0 */2 * * *".to_string()),
            None,
            None,
            "America/New_York".to_string(),
            Some(serde_json::json!({"key": "value"})),
            None,
            false,
        );

        assert_eq!(schedule.cron_expression, Some("0 */2 * * *".to_string()));
        assert_eq!(schedule.timezone, "America/New_York");
        assert!(!schedule.enabled);
        assert_eq!(schedule.input, serde_json::json!({"key": "value"}));
        assert!(schedule.updated_at > original_updated);
    }

    #[test]
    fn test_record_execution() {
        let mut schedule = WorkflowSchedule::new(
            "sched_001".to_string(),
            "wf_001".to_string(),
            "Test".to_string(),
            Some("0 0 * * *".to_string()),
            None,
            None,
            "UTC".to_string(),
            serde_json::json!({}),
            serde_json::json!({}),
            true,
        );

        assert_eq!(schedule.execution_count, 0);

        let exec_time = Utc::now();
        schedule.record_execution(exec_time);

        assert_eq!(schedule.last_run, Some(exec_time));
        assert_eq!(schedule.execution_count, 1);
    }

    #[test]
    fn test_validate_cron_expression_valid() {
        assert!(validate_cron_expression("0 0 0 * * *").is_ok()); // Daily at midnight
        assert!(validate_cron_expression("0 */5 * * * *").is_ok()); // Every 5 minutes
        assert!(validate_cron_expression("0 0 9 * * MON-FRI").is_ok()); // 9 AM on weekdays
        assert!(validate_cron_expression("0 0 0 1 * *").is_ok()); // Monthly on 1st
        assert!(validate_cron_expression("0 0 0,12 * * *").is_ok()); // Midnight and noon
        assert!(validate_cron_expression("0 0 0 * * MON").is_ok()); // Monday at midnight
    }

    #[test]
    fn test_validate_cron_expression_invalid() {
        assert!(validate_cron_expression("").is_err());
        assert!(validate_cron_expression("0").is_err());
        assert!(validate_cron_expression("0 0").is_err());
        assert!(validate_cron_expression("0 0 0").is_err());
        assert!(validate_cron_expression("0 0 0 0").is_err());
        assert!(validate_cron_expression("invalid cron expression").is_err());
    }

    #[test]
    fn test_validate_cron_field_wildcards() {
        assert!(is_valid_cron_field("*", 0));
        assert!(is_valid_cron_field("?", 2));
    }

    #[test]
    fn test_validate_cron_field_numbers() {
        // Minutes (0-59)
        assert!(is_valid_cron_field("0", 0));
        assert!(is_valid_cron_field("30", 0));
        assert!(is_valid_cron_field("59", 0));

        // Hours (0-23)
        assert!(is_valid_cron_field("0", 1));
        assert!(is_valid_cron_field("12", 1));
        assert!(is_valid_cron_field("23", 1));
    }

    #[test]
    fn test_validate_cron_field_ranges() {
        assert!(is_valid_cron_field("0-30", 0));
        assert!(is_valid_cron_field("9-17", 1));
        assert!(is_valid_cron_field("1-5", 4));
    }

    #[test]
    fn test_validate_cron_field_steps() {
        assert!(is_valid_cron_field("*/5", 0));
        assert!(is_valid_cron_field("0-30/5", 0));
        assert!(is_valid_cron_field("*/2", 1));
    }

    #[test]
    fn test_validate_cron_field_lists() {
        assert!(is_valid_cron_field("0,15,30,45", 0));
        assert!(is_valid_cron_field("9,12,18", 1));
        assert!(is_valid_cron_field("1,15", 2));
    }

    #[test]
    fn test_validate_timezone_valid() {
        assert!(validate_timezone("UTC"));
        assert!(validate_timezone("GMT"));
        assert!(validate_timezone("America/New_York"));
        assert!(validate_timezone("Europe/London"));
        assert!(validate_timezone("Asia/Tokyo"));
        assert!(validate_timezone("Australia/Sydney"));
    }

    #[test]
    fn test_validate_timezone_invalid() {
        assert!(!validate_timezone(""));
        assert!(!validate_timezone("InvalidTimezone"));
        assert!(!validate_timezone("America"));
        assert!(!validate_timezone("/New_York"));
        assert!(!validate_timezone("America/"));
    }
}
