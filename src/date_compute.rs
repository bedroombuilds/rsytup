//! Collection of helper functions to do necessary date computation
// SPDX-License-Identifier: GPL-3.0-or-later
// SPDX-FileCopyrightText: © 2021 Michael Kefeder
use chrono::{Datelike, NaiveDate, NaiveDateTime, Weekday};

/// NaiveDate from ISO 8601 string
pub fn parse_iso_date(d: &str) -> Result<NaiveDate, chrono::ParseError> {
    NaiveDate::parse_from_str(d, "%Y-%m-%d")
}

/// NaiveDateTime from ISO 8601 string, time optional
pub fn parse_iso_datetime(d: &str) -> Result<NaiveDateTime, chrono::ParseError> {
    let mut d = d.to_string();
    if d.len() == 10 {
        d.push_str(" 00:00:00");
    }
    NaiveDateTime::parse_from_str(&d, "%Y-%m-%d %H:%M:%S")
}

/// computes the next upcoming requested weekday from start-date
pub fn coming_weekday(start: NaiveDate, weekday: Weekday) -> NaiveDate {
    if start.weekday() == weekday {
        return add_weeks(start, 1);
    }
    for d in start.iter_days().take(7) {
        if d.weekday() == weekday {
            return d;
        }
    }
    start
}

/// Weeks after start-date
pub fn add_weeks(start: NaiveDate, weeks: u8) -> NaiveDate {
    start + chrono::Duration::weeks(weeks as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_coming_weekdays() {
        let thu = parse_iso_date("2021-09-02").unwrap();
        let fri = coming_weekday(thu, Weekday::Fri);
        assert_eq!(fri, parse_iso_date("2021-09-03").unwrap());
        let already_fri = parse_iso_date("2021-09-03").unwrap();
        let fri = coming_weekday(already_fri, Weekday::Fri);
        assert_eq!(fri, parse_iso_date("2021-09-10").unwrap());
    }

    #[test]
    fn test_add_weeks() {
        let thu = parse_iso_date("2021-09-02").unwrap();
        let thu_p1w = add_weeks(thu, 1);
        assert_eq!(thu_p1w, parse_iso_date("2021-09-09").unwrap());
    }

    #[test]
    fn test_parse_wo_time() {
        let thu1 = parse_iso_datetime("2021-09-02").unwrap();
        let thu2 = parse_iso_datetime("2021-09-02 00:00:00").unwrap();
        assert_eq!(thu1, thu2);
    }
}
