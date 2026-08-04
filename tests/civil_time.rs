//! Checks croner's own calendar arithmetic against `chrono`.
//!
//! Croner runs its search on [`croner::CivilDate`] and [`croner::CivilDateTime`]
//! so that every backend gets the same result. These tests confirm that the
//! calendar behind those types agrees with an independent implementation.

#![cfg(feature = "chrono")]

use chrono::{Datelike as _, Days, NaiveDate, Timelike as _};
use croner::{CivilDate, CivilDateTime, Weekday};

/// The first and last year that the comparison walks through.
const FIRST_YEAR: i32 = 1;
const LAST_YEAR: i32 = 3000;

fn chrono_weekday(date: NaiveDate) -> Weekday {
    Weekday::from_days_from_sunday(date.weekday().num_days_from_sunday())
}

#[test]
fn every_day_agrees_with_chrono() {
    let mut civil = CivilDate::from_ymd_opt(FIRST_YEAR, 1, 1).unwrap();
    let mut chrono = NaiveDate::from_ymd_opt(FIRST_YEAR, 1, 1).unwrap();

    while chrono.year() <= LAST_YEAR {
        assert_eq!(civil.year(), chrono.year(), "year at {chrono}");
        assert_eq!(civil.month(), chrono.month(), "month at {chrono}");
        assert_eq!(civil.day(), chrono.day(), "day at {chrono}");
        assert_eq!(
            civil.weekday(),
            chrono_weekday(chrono),
            "weekday at {chrono}"
        );

        civil = civil.checked_add_days(1).unwrap();
        chrono = chrono.checked_add_days(Days::new(1)).unwrap();
    }
}

#[test]
fn month_lengths_agree_with_chrono() {
    for year in FIRST_YEAR..=LAST_YEAR {
        for month in 1..=12 {
            let first = CivilDate::from_ymd_opt(year, month, 1).unwrap();
            let next_month = if month == 12 {
                NaiveDate::from_ymd_opt(year + 1, 1, 1)
            } else {
                NaiveDate::from_ymd_opt(year, month + 1, 1)
            }
            .unwrap();
            let last_day = next_month.pred_opt().unwrap().day();

            assert_eq!(first.days_in_month(), last_day, "{year}-{month:02}");
            assert!(CivilDate::from_ymd_opt(year, month, last_day).is_some());
            assert!(CivilDate::from_ymd_opt(year, month, last_day + 1).is_none());
        }
    }
}

#[test]
fn second_arithmetic_agrees_with_chrono() {
    // Steps that cross minute, hour, day, month and year boundaries, in both
    // directions.
    let steps: [i64; 10] = [
        1,
        -1,
        59,
        -59,
        3_600,
        -3_600,
        86_400,
        -86_400,
        86_400 * 366,
        -86_400 * 366,
    ];
    let starts = [
        (2024, 2, 28, 23, 59, 59),
        (2023, 12, 31, 23, 59, 59),
        (2024, 3, 1, 0, 0, 0),
        (1970, 1, 1, 0, 0, 0),
        (1969, 12, 31, 23, 59, 59),
        (1900, 1, 1, 0, 0, 0),
    ];

    for (year, month, day, hour, minute, second) in starts {
        let civil = CivilDateTime::from_ymd_hms(year, month, day, hour, minute, second).unwrap();
        let chrono = NaiveDate::from_ymd_opt(year, month, day)
            .unwrap()
            .and_hms_opt(hour, minute, second)
            .unwrap();

        for step in steps {
            let moved_civil = civil.checked_add_seconds(step).unwrap();
            let moved_chrono = chrono
                .checked_add_signed(chrono::TimeDelta::try_seconds(step).unwrap())
                .unwrap();

            assert_eq!(moved_civil.year(), moved_chrono.year(), "{civil} + {step}s");
            assert_eq!(
                moved_civil.month(),
                moved_chrono.month(),
                "{civil} + {step}s"
            );
            assert_eq!(moved_civil.day(), moved_chrono.day(), "{civil} + {step}s");
            assert_eq!(moved_civil.hour(), moved_chrono.hour(), "{civil} + {step}s");
            assert_eq!(
                moved_civil.minute(),
                moved_chrono.minute(),
                "{civil} + {step}s"
            );
            assert_eq!(
                moved_civil.second(),
                moved_chrono.second(),
                "{civil} + {step}s"
            );
        }
    }
}
