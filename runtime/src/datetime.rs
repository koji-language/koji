//! Dates: the calendar computations, `format` and `parse`, and
//! the `datetime` module for compiled programs.

use crate::record::{field_record, make, KojiRecord, KojiRecordType};
use crate::string::{new_string, string_text, KojiString};

/// A date and time in UTC.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DateTime {
    pub year: i64,
    pub month: i64,
    pub day: i64,
    pub hour: i64,
    pub minute: i64,
    pub second: i64,
}

impl DateTime {
    pub fn field(&self, name: &str) -> Option<i64> {
        Some(match name {
            "year" => self.year,
            "month" => self.month,
            "day" => self.day,
            "hour" => self.hour,
            "minute" => self.minute,
            "second" => self.second,
            _ => return None,
        })
    }
}

impl std::fmt::Display for DateTime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
            self.year, self.month, self.day, self.hour, self.minute, self.second
        )
    }
}

/// Seconds since 1970-01-01 00:00:00 UTC.
pub fn unix_now() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

/// Days since 1970-01-01 to a civil date (Howard Hinnant's algorithm).
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn days_in_month(year: i64, month: i64) -> i64 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 => 29,
        _ => 28,
    }
}

pub fn datetime_from_timestamp(timestamp: i64) -> DateTime {
    let days = timestamp.div_euclid(86_400);
    let seconds = timestamp.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    DateTime {
        year,
        month,
        day,
        hour: seconds / 3600,
        minute: seconds % 3600 / 60,
        second: seconds % 60,
    }
}

#[derive(Clone, Copy)]
enum DateToken {
    Year,
    Month,
    Day,
    Hour,
    Minute,
    Second,
}

/// Splits a pattern into tokens (`yyyy`, `mm`, `dd`, `hh`, `min`, `ss`, any case) and literal text.
fn date_tokens(pattern: &str) -> Vec<Result<DateToken, char>> {
    let chars: Vec<char> = pattern.chars().collect();
    let lower: Vec<char> = pattern.to_lowercase().chars().collect();
    let mut tokens = Vec::new();
    let mut i = 0;
    let starts = |i: usize, word: &str| -> bool {
        word.chars().enumerate().all(|(k, c)| lower.get(i + k) == Some(&c))
    };
    while i < chars.len() {
        let (token, len) = if starts(i, "yyyy") {
            (Ok(DateToken::Year), 4)
        } else if starts(i, "min") {
            (Ok(DateToken::Minute), 3)
        } else if starts(i, "mm") {
            (Ok(DateToken::Month), 2)
        } else if starts(i, "dd") {
            (Ok(DateToken::Day), 2)
        } else if starts(i, "hh") {
            (Ok(DateToken::Hour), 2)
        } else if starts(i, "ss") {
            (Ok(DateToken::Second), 2)
        } else {
            (Err(chars[i]), 1)
        };
        tokens.push(token);
        i += len;
    }
    tokens
}

pub fn format_datetime(datetime: &DateTime, pattern: &str) -> String {
    let mut out = String::new();
    for token in date_tokens(pattern) {
        match token {
            Ok(DateToken::Year) => out.push_str(&format!("{:04}", datetime.year)),
            Ok(DateToken::Month) => out.push_str(&format!("{:02}", datetime.month)),
            Ok(DateToken::Day) => out.push_str(&format!("{:02}", datetime.day)),
            Ok(DateToken::Hour) => out.push_str(&format!("{:02}", datetime.hour)),
            Ok(DateToken::Minute) => out.push_str(&format!("{:02}", datetime.minute)),
            Ok(DateToken::Second) => out.push_str(&format!("{:02}", datetime.second)),
            Err(c) => out.push(c),
        }
    }
    out
}

#[derive(Debug)]
pub enum DateFailure {
    PatternMismatch,
    InvalidDate,
}

pub fn parse_datetime(text: &str, pattern: &str) -> Result<i64, DateFailure> {
    let chars: Vec<char> = text.chars().collect();
    let mut position = 0;
    let mut datetime = DateTime {
        year: 1970,
        month: 1,
        day: 1,
        hour: 0,
        minute: 0,
        second: 0,
    };
    let mismatch = || DateFailure::PatternMismatch;
    for token in date_tokens(pattern) {
        match token {
            Err(c) => {
                if chars.get(position) != Some(&c) {
                    return Err(mismatch());
                }
                position += 1;
            }
            Ok(token) => {
                let width = if matches!(token, DateToken::Year) { 4 } else { 2 };
                let digits: String = chars.iter().skip(position).take(width).collect();
                if digits.len() != width || !digits.chars().all(|c| c.is_ascii_digit()) {
                    return Err(mismatch());
                }
                position += width;
                let n: i64 = digits.parse().map_err(|_| mismatch())?;
                match token {
                    DateToken::Year => datetime.year = n,
                    DateToken::Month => datetime.month = n,
                    DateToken::Day => datetime.day = n,
                    DateToken::Hour => datetime.hour = n,
                    DateToken::Minute => datetime.minute = n,
                    DateToken::Second => datetime.second = n,
                }
            }
        }
    }
    if position != chars.len() {
        return Err(mismatch());
    }
    let valid = (1..=12).contains(&datetime.month)
        && datetime.day >= 1
        && datetime.day <= days_in_month(datetime.year, datetime.month)
        && datetime.hour < 24
        && datetime.minute < 60
        && datetime.second < 60;
    if !valid {
        return Err(DateFailure::InvalidDate);
    }
    Ok(days_from_civil(datetime.year, datetime.month, datetime.day) * 86_400
        + datetime.hour * 3600
        + datetime.minute * 60
        + datetime.second)
}


// The `datetime` module for compiled programs.

fn slot(string: *mut KojiString) -> u64 {
    string as usize as u64
}

/// `datetime.now()`: a `DateTime` record (its fields in order).
#[no_mangle]
pub extern "C" fn koji_std_datetime_now(ty: *const KojiRecordType) -> *mut KojiRecord {
    let now = datetime_from_timestamp(unix_now().floor() as i64);
    let fields = [now.year, now.month, now.day, now.hour, now.minute, now.second].map(|n| n as u64);
    make(ty, "", &fields)
}

#[no_mangle]
pub extern "C" fn koji_std_datetime_today() -> *mut KojiString {
    let now = datetime_from_timestamp(unix_now().floor() as i64);
    new_string(format!("{:04}-{:02}-{:02}", now.year, now.month, now.day))
}

#[no_mangle]
pub extern "C" fn koji_std_datetime_format(timestamp: crate::decimal::KojiDecimal, pattern: *const KojiString) -> *mut KojiString {
    let seconds = crate::decimal::from_c(timestamp).to_int(0).unwrap_or(0);
    let datetime = datetime_from_timestamp(seconds);
    new_string(format_datetime(&datetime, string_text(pattern)))
}

/// `datetime.parse(text, pattern)`: `Ok(timestamp)` or `Err(DateError)`.
#[no_mangle]
pub extern "C" fn koji_std_datetime_parse(text: *const KojiString, pattern: *const KojiString, result: *const KojiRecordType) -> *mut KojiRecord {
    let (text, pattern) = (string_text(text), string_text(pattern));
    match parse_datetime(text, pattern) {
        Ok(timestamp) => make(result, "Ok", &[timestamp as u64]),
        Err(failure) => {
            let error_type = field_record(result, "Err", 0);
            let error = match failure {
                DateFailure::PatternMismatch => make(
                    error_type,
                    "PatternMismatch",
                    &[slot(new_string(text.to_string())), slot(new_string(pattern.to_string()))],
                ),
                DateFailure::InvalidDate => make(error_type, "InvalidDate", &[slot(new_string(text.to_string()))]),
            };
            make(result, "Err", &[error as usize as u64])
        }
    }
}

