//! Schedule Calculation - Next execution time calculation with timezone support
//!
//! Handles cron expressions, interval-based scheduling, and one-time scheduled_at times.

use anyhow::{Context, Result};
use chrono::{DateTime, Datelike, Duration, Timelike, Utc};
use chrono_tz::Tz;
use cron::Schedule as CronSchedule;
use std::str::FromStr;

/// Calculate the next execution time for a schedule
///
/// Supports three schedule types:
/// 1. Cron expression with timezone
/// 2. Interval in seconds
/// 3. One-time scheduled_at time
///
/// ## Arguments
/// * `cron_expression` - Optional cron expression in 6-field format: "sec min hour day month dow" (e.g., "0 0 0 * * *" for daily at midnight)
/// * `interval_seconds` - Optional interval in seconds for recurring execution
/// * `scheduled_at` - Optional one-time execution time
/// * `timezone` - IANA timezone identifier (e.g., "America/New_York", "UTC")
/// * `from_time` - Calculate next execution from this time (usually now)
///
/// ## Returns
/// * `Some(DateTime<Utc>)` - Next execution time in UTC
/// * `None` - No next execution (e.g., one-time schedule already passed)
pub fn calculate_next_execution(
    cron_expression: Option<&str>,
    interval_seconds: Option<u64>,
    scheduled_at: Option<DateTime<Utc>>,
    timezone: &str,
    from_time: DateTime<Utc>,
) -> Result<Option<DateTime<Utc>>> {
    // Parse timezone
    let tz: Tz = timezone
        .parse()
        .map_err(|e: String| anyhow::anyhow!("Invalid timezone '{}': {}", timezone, e))?;

    // Priority: scheduled_at > cron_expression > interval_seconds
    if let Some(scheduled_time) = scheduled_at {
        // One-time execution
        if scheduled_time > from_time {
            return Ok(Some(scheduled_time));
        } else {
            // Already passed
            return Ok(None);
        }
    }

    if let Some(cron_expr) = cron_expression {
        // Cron-based scheduling
        return calculate_next_cron_execution(cron_expr, tz, from_time);
    }

    if let Some(interval) = interval_seconds {
        // Interval-based scheduling
        let next = from_time + Duration::seconds(interval as i64);
        return Ok(Some(next));
    }

    // No schedule defined
    Ok(None)
}

/// Calculate next execution time for a cron expression in a specific timezone
fn calculate_next_cron_execution(
    cron_expr: &str,
    timezone: Tz,
    from_time: DateTime<Utc>,
) -> Result<Option<DateTime<Utc>>> {
    // Parse cron expression
    let schedule = CronSchedule::from_str(cron_expr)
        .context(format!("Invalid cron expression: {}", cron_expr))?;

    // Convert UTC time to target timezone
    let from_time_in_tz = from_time.with_timezone(&timezone);

    // Get next execution in target timezone
    let next_in_tz = schedule
        .after(&from_time_in_tz)
        .next()
        .context("Failed to calculate next cron execution")?;

    // Convert back to UTC
    let next_utc = next_in_tz.with_timezone(&Utc);

    Ok(Some(next_utc))
}

/// Update next_run time after execution
///
/// Recalculates next execution based on current time and schedule configuration.
pub fn update_next_run(
    cron_expression: Option<&str>,
    interval_seconds: Option<u64>,
    scheduled_at: Option<DateTime<Utc>>,
    timezone: &str,
) -> Result<Option<DateTime<Utc>>> {
    calculate_next_execution(
        cron_expression,
        interval_seconds,
        scheduled_at,
        timezone,
        Utc::now(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn test_calculate_next_cron_utc() {
        // Daily at midnight UTC (6-field format: sec min hour day month dow)
        let cron_expr = "0 0 0 * * *";
        let from_time = Utc.with_ymd_and_hms(2024, 10, 15, 12, 0, 0).unwrap();

        let next = calculate_next_execution(Some(cron_expr), None, None, "UTC", from_time)
            .unwrap()
            .unwrap();

        // Should be next day at midnight
        assert_eq!(next.hour(), 0);
        assert_eq!(next.minute(), 0);
        assert!(next > from_time);
    }

    #[test]
    fn test_calculate_next_cron_with_timezone() {
        // Daily at 9 AM Eastern Time (6-field format: sec min hour day month dow)
        let cron_expr = "0 0 9 * * *";
        let from_time = Utc.with_ymd_and_hms(2024, 10, 15, 12, 0, 0).unwrap();

        let next =
            calculate_next_execution(Some(cron_expr), None, None, "America/New_York", from_time)
                .unwrap()
                .unwrap();

        // Convert to Eastern to verify
        let eastern_tz: Tz = "America/New_York".parse().unwrap();
        let next_eastern = next.with_timezone(&eastern_tz);

        assert_eq!(next_eastern.hour(), 9);
        assert_eq!(next_eastern.minute(), 0);
    }

    #[test]
    fn test_calculate_next_interval() {
        let from_time = Utc::now();
        let interval_seconds = 3600; // 1 hour

        let next = calculate_next_execution(None, Some(interval_seconds), None, "UTC", from_time)
            .unwrap()
            .unwrap();

        let expected = from_time + Duration::seconds(3600);

        // Allow 1 second tolerance
        assert!((next - expected).num_seconds().abs() < 1);
    }

    #[test]
    fn test_calculate_next_scheduled_at_future() {
        let from_time = Utc::now();
        let scheduled_at = from_time + Duration::hours(24);

        let next = calculate_next_execution(None, None, Some(scheduled_at), "UTC", from_time)
            .unwrap()
            .unwrap();

        assert_eq!(next, scheduled_at);
    }

    #[test]
    fn test_calculate_next_scheduled_at_past() {
        let from_time = Utc::now();
        let scheduled_at = from_time - Duration::hours(24);

        let next =
            calculate_next_execution(None, None, Some(scheduled_at), "UTC", from_time).unwrap();

        // Should be None since it's in the past
        assert!(next.is_none());
    }

    #[test]
    fn test_priority_scheduled_at_over_cron() {
        let from_time = Utc::now();
        let scheduled_at = from_time + Duration::hours(24);

        // Both scheduled_at and cron provided - scheduled_at takes priority
        let next = calculate_next_execution(
            Some("0 0 0 * * *"),
            None,
            Some(scheduled_at),
            "UTC",
            from_time,
        )
        .unwrap()
        .unwrap();

        assert_eq!(next, scheduled_at);
    }

    #[test]
    fn test_priority_cron_over_interval() {
        let from_time = Utc.with_ymd_and_hms(2024, 10, 15, 12, 0, 0).unwrap();

        // Both cron and interval provided - cron takes priority
        let next =
            calculate_next_execution(Some("0 0 0 * * *"), Some(3600), None, "UTC", from_time)
                .unwrap()
                .unwrap();

        // Should follow cron (midnight), not interval (1 hour from now)
        assert_eq!(next.hour(), 0);
        assert_eq!(next.minute(), 0);
    }

    #[test]
    fn test_invalid_cron_expression() {
        let from_time = Utc::now();

        let result = calculate_next_execution(Some("invalid cron"), None, None, "UTC", from_time);

        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_timezone() {
        let from_time = Utc::now();

        let result = calculate_next_execution(
            Some("0 0 0 * * *"),
            None,
            None,
            "Invalid/Timezone",
            from_time,
        );

        assert!(result.is_err());
    }

    #[test]
    fn test_no_schedule_defined() {
        let from_time = Utc::now();

        let next = calculate_next_execution(None, None, None, "UTC", from_time).unwrap();

        assert!(next.is_none());
    }

    #[test]
    fn test_every_5_minutes() {
        // Every 5 minutes (6-field format: sec min hour day month dow)
        let cron_expr = "0 */5 * * * *";
        let from_time = Utc.with_ymd_and_hms(2024, 10, 15, 12, 3, 0).unwrap();

        let next = calculate_next_execution(Some(cron_expr), None, None, "UTC", from_time)
            .unwrap()
            .unwrap();

        // Should be at 12:05:00
        assert_eq!(next.hour(), 12);
        assert_eq!(next.minute(), 5);
    }

    #[test]
    fn test_weekday_only() {
        // Monday-Friday at 9 AM (6-field format: sec min hour day month dow)
        let cron_expr = "0 0 9 * * MON-FRI";
        let from_time = Utc.with_ymd_and_hms(2024, 10, 18, 12, 0, 0).unwrap(); // Friday

        let next = calculate_next_execution(Some(cron_expr), None, None, "UTC", from_time)
            .unwrap()
            .unwrap();

        // Should be Monday at 9 AM
        let next_day = next.weekday();
        assert!(next_day.num_days_from_monday() < 5); // Monday-Friday
    }
}
