use crate::error::{Result, SlateError};
use std::sync::atomic::Ordering;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub(crate) fn generate_restore_point_id(now: SystemTime) -> String {
    let seq = super::RESTORE_POINT_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!(
        "{}-{}-{:04}",
        format_iso8601_timestamp(now),
        std::process::id(),
        seq % 10_000
    )
}

pub(crate) fn timestamp_from_string(ts_str: &str) -> Result<SystemTime> {
    if ts_str.len() != 20 || !ts_str.ends_with('Z') {
        return Err(SlateError::BackupFailed(format!(
            "Invalid timestamp format: {}",
            ts_str
        )));
    }

    let chars: Vec<char> = ts_str.chars().collect();
    if chars[4] != '-'
        || chars[7] != '-'
        || chars[10] != 'T'
        || chars[13] != '-'
        || chars[16] != '-'
    {
        return Err(SlateError::BackupFailed(format!(
            "Invalid timestamp format: {}",
            ts_str
        )));
    }

    let year: u64 = ts_str[0..4]
        .parse()
        .map_err(|_| SlateError::BackupFailed(format!("Invalid year in timestamp: {}", ts_str)))?;
    let month: u64 = ts_str[5..7]
        .parse()
        .map_err(|_| SlateError::BackupFailed(format!("Invalid month in timestamp: {}", ts_str)))?;
    let day: u64 = ts_str[8..10]
        .parse()
        .map_err(|_| SlateError::BackupFailed(format!("Invalid day in timestamp: {}", ts_str)))?;
    let hour: u64 = ts_str[11..13]
        .parse()
        .map_err(|_| SlateError::BackupFailed(format!("Invalid hour in timestamp: {}", ts_str)))?;
    let minute: u64 = ts_str[14..16].parse().map_err(|_| {
        SlateError::BackupFailed(format!("Invalid minute in timestamp: {}", ts_str))
    })?;
    let second: u64 = ts_str[17..19].parse().map_err(|_| {
        SlateError::BackupFailed(format!("Invalid second in timestamp: {}", ts_str))
    })?;

    let days_since_epoch = days_from_unix_epoch(year, month, day).ok_or_else(|| {
        SlateError::BackupFailed(format!("Invalid date in timestamp: {}", ts_str))
    })?;

    let total_seconds = days_since_epoch * 86400 + hour * 3600 + minute * 60 + second;
    Ok(UNIX_EPOCH + Duration::from_secs(total_seconds))
}

pub(crate) fn format_iso8601_timestamp(time: SystemTime) -> String {
    let duration = time.duration_since(UNIX_EPOCH).unwrap_or_default();
    let secs = duration.as_secs();
    let days_since_epoch = secs / 86400;
    let secs_today = secs % 86400;

    let hours = secs_today / 3600;
    let minutes = (secs_today % 3600) / 60;
    let seconds = secs_today % 60;

    let (year, month, day) = calculate_date(days_since_epoch);

    format!(
        "{:04}-{:02}-{:02}T{:02}-{:02}-{:02}Z",
        year, month, day, hours, minutes, seconds
    )
}

fn days_from_unix_epoch(year: u64, month: u64, day: u64) -> Option<u64> {
    if !(1..=12).contains(&month) || day < 1 {
        return None;
    }

    let is_leap = |y: u64| (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0);
    let days_in_month = if is_leap(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    if day > days_in_month[month as usize - 1] {
        return None;
    }

    let mut days = 0u64;
    for y in 1970..year {
        days += if is_leap(y) { 366 } else { 365 };
    }
    for m in 1..month {
        days += days_in_month[m as usize - 1];
    }
    days += day - 1;

    Some(days)
}

pub(crate) fn calculate_date(mut days: u64) -> (u64, u64, u64) {
    let mut year = 1970;

    loop {
        let days_in_year = if is_leap_year(year) { 366 } else { 365 };
        if days < days_in_year {
            break;
        }
        days -= days_in_year;
        year += 1;
    }

    let is_leap = is_leap_year(year);
    let days_in_months = if is_leap {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    let mut month = 1;
    let mut day = days + 1;
    for &days_in_month in &days_in_months {
        if day <= days_in_month as u64 {
            break;
        }
        day -= days_in_month as u64;
        month += 1;
    }

    (year, month, day)
}

pub(crate) fn is_leap_year(year: u64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_iso8601_timestamp_format() {
        let timestamp = format_iso8601_timestamp(SystemTime::UNIX_EPOCH);
        assert!(timestamp.starts_with("1970-"));
        assert!(timestamp.ends_with('Z'));
        assert!(!timestamp.contains(':'));
        assert_eq!(timestamp.len(), 20);
    }

    #[test]
    fn test_calculate_date_epoch() {
        let (year, month, day) = calculate_date(0);
        assert_eq!(year, 1970);
        assert_eq!(month, 1);
        assert_eq!(day, 1);
    }

    #[test]
    fn test_calculate_date_after_year() {
        let (year, month, day) = calculate_date(365);
        assert_eq!(year, 1971);
        assert_eq!(month, 1);
        assert_eq!(day, 1);
    }

    #[test]
    fn test_is_leap_year() {
        assert!(is_leap_year(2000));
        assert!(is_leap_year(2004));
        assert!(!is_leap_year(1900));
        assert!(!is_leap_year(2001));
    }

    #[test]
    fn test_timestamp_from_string_valid() {
        let ts_str = "2026-04-09T10-00-00Z";
        let result = timestamp_from_string(ts_str);
        assert!(result.is_ok());
    }

    #[test]
    fn test_timestamp_from_string_invalid_format() {
        let ts_str = "2026-04-09 10:00:00Z";
        let result = timestamp_from_string(ts_str);
        assert!(result.is_err());
    }

    #[test]
    fn test_timestamp_from_string_invalid_length() {
        let ts_str = "2026-04-09T10-00-00";
        let result = timestamp_from_string(ts_str);
        assert!(result.is_err());
    }
}
