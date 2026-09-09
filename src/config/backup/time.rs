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
    let invalid = || {
        SlateError::BackupFailed("Invalid restore timestamp; expected a real UTC date in YYYY-MM-DDTHH-MM-SSZ format (1970-9999)".into())
    };
    let bytes = ts_str.as_bytes();
    if bytes.len() != 20
        || !bytes.iter().enumerate().all(|(i, &b)| match i {
            4 | 7 | 13 | 16 => b == b'-',
            10 => b == b'T',
            19 => b == b'Z',
            _ => b.is_ascii_digit(),
        })
    {
        return Err(invalid());
    }
    // Every byte is now ASCII in a validated position; no Unicode slicing or
    // unchecked component parsing. Do not echo the untrusted saved timestamp.
    let number = |start: usize, end: usize| {
        bytes[start..end]
            .iter()
            .fold(0u64, |n, b| n * 10 + u64::from(b - b'0'))
    };
    let (year, month, day) = (number(0, 4), number(5, 7), number(8, 10));
    let (hour, minute, second) = (number(11, 13), number(14, 16), number(17, 19));
    if year < 1970 || hour > 23 || minute > 59 || second > 59 {
        return Err(invalid());
    }
    let days_since_epoch = days_from_unix_epoch(year, month, day).ok_or_else(invalid)?;

    let total_seconds = days_since_epoch * 86400 + hour * 3600 + minute * 60 + second;
    UNIX_EPOCH
        .checked_add(Duration::from_secs(total_seconds))
        .ok_or_else(invalid)
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

    let is_leap = |y: u64| (y.is_multiple_of(4) && !y.is_multiple_of(100)) || y.is_multiple_of(400);
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
    (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400)
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

    #[test]
    fn restore_timestamps_validate_ascii_calendar_and_clock_without_echoing_input() {
        for timestamp in [
            "202é-09-05T00-00-0Z",
            "1969-12-31T23-59-59Z",
            "2026-01-01T24-00-00Z",
            "2026-01-01T00-60-00Z",
            "2026-01-01T00-00-60Z",
            "2026-02-29T00-00-00Z",
            "2026-00-01T00-00-00Z",
            "2026-01-00T00-00-00Z",
            "2026-04-31T00-00-00Z",
            "2026-+1-01T00-00-00Z",
            "PRIVATE_SAVED_CONTENT",
        ] {
            let error = timestamp_from_string(timestamp).unwrap_err().to_string();
            assert!(!error.contains(timestamp));
        }
        for timestamp in [
            "1970-01-01T00-00-00Z",
            "2000-02-29T23-59-59Z",
            "2024-02-29T12-34-56Z",
            "9999-12-31T23-59-59Z",
        ] {
            assert_eq!(
                format_iso8601_timestamp(timestamp_from_string(timestamp).unwrap()),
                timestamp
            );
        }
    }
}
