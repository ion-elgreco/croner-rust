//! Backend-agnostic date and time support.
//!
//! Croner runs its search on the civil (wall clock) types in this module, so a
//! pattern gives the same result with every date and time library. A library is
//! connected through the [`CronDateTime`] trait.
//!
//! Croner includes these implementations:
//!
//! | Type | Crate feature |
//! |------|---------------|
//! | [`chrono::DateTime<Tz>`](https://docs.rs/chrono/0.4/chrono/struct.DateTime.html) | `chrono` (default) |
//! | [`chrono::NaiveDateTime`](https://docs.rs/chrono/0.4/chrono/struct.NaiveDateTime.html) | `chrono` (default) |
//! | [`jiff::Zoned`](https://docs.rs/jiff/0.2/jiff/struct.Zoned.html) | `jiff` |
//! | [`jiff::civil::DateTime`](https://docs.rs/jiff/0.2/jiff/civil/struct.DateTime.html) | `jiff` |
//!
//! Implement [`CronDateTime`] for your own type to use a different library.

#[cfg(feature = "chrono")]
#[cfg_attr(docsrs, doc(cfg(feature = "chrono")))]
mod chrono_impl;
#[cfg(feature = "jiff")]
#[cfg_attr(docsrs, doc(cfg(feature = "jiff")))]
mod jiff_impl;

use crate::errors::CronError;

const SECONDS_PER_DAY: i64 = 86_400;

/// Days between 1970-01-01 and 0000-03-01, the start of the era used by the
/// civil-to-day-count conversion.
const DAYS_FROM_EPOCH_TO_ERA: i64 = 719_468;

/// Days in a 400 year era of the proleptic Gregorian calendar.
const DAYS_PER_ERA: i64 = 146_097;

/// A day of the week.
///
/// The discriminants count from Sunday, which is the numbering that cron
/// patterns use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Weekday {
    Sunday = 0,
    Monday = 1,
    Tuesday = 2,
    Wednesday = 3,
    Thursday = 4,
    Friday = 5,
    Saturday = 6,
}

impl Weekday {
    /// Returns the number of days from Sunday, in the range 0 to 6.
    pub const fn num_days_from_sunday(self) -> u32 {
        self as u32
    }

    /// Returns `true` for Saturday and Sunday.
    pub const fn is_weekend(self) -> bool {
        matches!(self, Weekday::Saturday | Weekday::Sunday)
    }

    /// Creates a weekday from the number of days from Sunday, wrapping every
    /// seven days. This is the inverse of [`num_days_from_sunday`].
    ///
    /// [`num_days_from_sunday`]: Weekday::num_days_from_sunday
    pub const fn from_days_from_sunday(days: u32) -> Weekday {
        match days % 7 {
            0 => Weekday::Sunday,
            1 => Weekday::Monday,
            2 => Weekday::Tuesday,
            3 => Weekday::Wednesday,
            4 => Weekday::Thursday,
            5 => Weekday::Friday,
            _ => Weekday::Saturday,
        }
    }
}

/// A civil date: a year, a month and a day, without a time zone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CivilDate {
    year: i32,
    month: u8,
    day: u8,
}

impl CivilDate {
    /// Creates a date, or returns `None` if the date does not exist.
    pub fn from_ymd_opt(year: i32, month: u32, day: u32) -> Option<CivilDate> {
        // `days_in_month` returns 0 for a month outside 1 to 12, so this one
        // check covers both an out-of-range month and a day the month does not
        // have.
        if day == 0 || day > days_in_month(year, month) {
            return None;
        }
        Some(CivilDate {
            year,
            month: month as u8,
            day: day as u8,
        })
    }

    /// Creates a date, or returns [`CronError::InvalidDate`] if the date does
    /// not exist.
    pub fn from_ymd(year: i32, month: u32, day: u32) -> Result<CivilDate, CronError> {
        CivilDate::from_ymd_opt(year, month, day).ok_or(CronError::InvalidDate)
    }

    /// Creates a date without checking that it exists.
    ///
    /// The caller must pass a real date.
    pub(crate) const fn from_parts_unchecked(year: i32, month: u32, day: u32) -> CivilDate {
        CivilDate {
            year,
            month: month as u8,
            day: day as u8,
        }
    }

    /// Returns the year.
    pub const fn year(self) -> i32 {
        self.year
    }

    /// Returns the month, in the range 1 to 12.
    pub const fn month(self) -> u32 {
        self.month as u32
    }

    /// Returns the day of the month, in the range 1 to 31.
    pub const fn day(self) -> u32 {
        self.day as u32
    }

    /// Returns the day of the week.
    pub fn weekday(self) -> Weekday {
        // Sakamoto's algorithm.
        //
        // The year is first shifted into positive numbers, so that the
        // divisions below are plain truncating ones. The shift is a whole
        // number of 400 year cycles, and each cycle holds 146097 days, which
        // is a whole number of weeks. So it does not change the result.
        const MONTH_OFFSET: [i64; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
        const CYCLE_SHIFT: i64 = 400 * ((i32::MAX as i64 / 400) + 1);

        let year = i64::from(self.year) - i64::from(self.month < 3) + CYCLE_SHIFT;
        let days = year + year / 4 - year / 100
            + year / 400
            + MONTH_OFFSET[(self.month - 1) as usize]
            + i64::from(self.day);
        Weekday::from_days_from_sunday((days % 7) as u32)
    }

    /// Returns the number of days in this date's month.
    pub fn days_in_month(self) -> u32 {
        days_in_month(self.year, self.month())
    }

    /// Adds a signed number of days, or returns `None` on overflow.
    pub fn checked_add_days(self, days: i64) -> Option<CivilDate> {
        // Croner steps a day at a time, so the result usually stays inside the
        // same month. That case needs no calendar arithmetic.
        let day = i64::from(self.day) + days;
        if day >= 1 && day <= i64::from(self.days_in_month()) {
            return Some(CivilDate {
                day: day as u8,
                ..self
            });
        }
        CivilDate::from_days(self.to_days().checked_add(days)?)
    }

    /// Returns the number of days since 1970-01-01.
    fn to_days(self) -> i64 {
        days_from_civil(self.year, self.month(), self.day())
    }

    /// Creates a date from a number of days since 1970-01-01, or returns `None`
    /// if the date is outside the supported year range.
    fn from_days(days: i64) -> Option<CivilDate> {
        let (year, month, day) = civil_from_days(days)?;
        Some(CivilDate {
            year,
            month: month as u8,
            day: day as u8,
        })
    }
}

/// A civil time of day to second precision, without a time zone.
///
/// Sub-second parts are not kept, because cron patterns never match on them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CivilTime {
    hour: u8,
    minute: u8,
    second: u8,
}

impl CivilTime {
    /// The first second of a day, 00:00:00.
    pub const MIDNIGHT: CivilTime = CivilTime::from_parts_unchecked(0, 0, 0);

    /// The last second of a day, 23:59:59.
    pub const END_OF_DAY: CivilTime = CivilTime::from_parts_unchecked(23, 59, 59);

    /// Creates a time of day, or returns `None` if any part is out of range.
    ///
    /// Leap seconds are not supported, so `second` must be in the range 0 to 59.
    pub fn from_hms_opt(hour: u32, minute: u32, second: u32) -> Option<CivilTime> {
        if hour > 23 || minute > 59 || second > 59 {
            return None;
        }
        Some(CivilTime::from_parts_unchecked(hour, minute, second))
    }

    /// Creates a time of day without checking the parts.
    ///
    /// The caller must pass a time of day in range.
    pub(crate) const fn from_parts_unchecked(hour: u32, minute: u32, second: u32) -> CivilTime {
        CivilTime {
            hour: hour as u8,
            minute: minute as u8,
            second: second as u8,
        }
    }

    /// Returns the hour, in the range 0 to 23.
    pub const fn hour(self) -> u32 {
        self.hour as u32
    }

    /// Returns the minute, in the range 0 to 59.
    pub const fn minute(self) -> u32 {
        self.minute as u32
    }

    /// Returns the second, in the range 0 to 59.
    pub const fn second(self) -> u32 {
        self.second as u32
    }

    /// Returns the number of seconds since midnight.
    const fn seconds_of_day(self) -> i64 {
        self.hour as i64 * 3600 + self.minute as i64 * 60 + self.second as i64
    }

    /// Creates a time of day from a number of seconds since midnight, which
    /// must be in the range 0 to 86399.
    const fn from_seconds_of_day(seconds: i64) -> CivilTime {
        CivilTime {
            hour: (seconds / 3600) as u8,
            minute: (seconds % 3600 / 60) as u8,
            second: (seconds % 60) as u8,
        }
    }
}

impl core::fmt::Display for CivilTime {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:02}:{:02}:{:02}", self.hour, self.minute, self.second)
    }
}

/// A civil date and time to second precision, without a time zone.
///
/// This is croner's equivalent of `chrono::NaiveDateTime` and
/// `jiff::civil::DateTime`. Sub-second parts are not kept, because cron
/// patterns never match on them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CivilDateTime {
    date: CivilDate,
    time: CivilTime,
}

impl CivilDateTime {
    /// Creates a date and time, or returns `None` if either part is invalid.
    ///
    /// Leap seconds are not supported, so `second` must be in the range 0 to 59.
    pub fn from_ymd_hms_opt(
        year: i32,
        month: u32,
        day: u32,
        hour: u32,
        minute: u32,
        second: u32,
    ) -> Option<CivilDateTime> {
        CivilDate::from_ymd_opt(year, month, day)?.and_hms_opt(hour, minute, second)
    }

    /// Joins a date and a time of day.
    pub const fn new(date: CivilDate, time: CivilTime) -> CivilDateTime {
        CivilDateTime { date, time }
    }

    /// Creates a date and time, or returns an error if either part is invalid.
    pub fn from_ymd_hms(
        year: i32,
        month: u32,
        day: u32,
        hour: u32,
        minute: u32,
        second: u32,
    ) -> Result<CivilDateTime, CronError> {
        CivilDateTime::from_ymd_hms_opt(year, month, day, hour, minute, second)
            .ok_or(CronError::InvalidTime)
    }

    /// Returns the date part.
    pub const fn date(self) -> CivilDate {
        self.date
    }

    /// Returns the year.
    pub const fn year(self) -> i32 {
        self.date.year()
    }

    /// Returns the month, in the range 1 to 12.
    pub const fn month(self) -> u32 {
        self.date.month()
    }

    /// Returns the day of the month, in the range 1 to 31.
    pub const fn day(self) -> u32 {
        self.date.day()
    }

    /// Returns the time of day.
    pub const fn time(self) -> CivilTime {
        self.time
    }

    /// Returns the hour, in the range 0 to 23.
    pub const fn hour(self) -> u32 {
        self.time.hour()
    }

    /// Returns the minute, in the range 0 to 59.
    pub const fn minute(self) -> u32 {
        self.time.minute()
    }

    /// Returns the second, in the range 0 to 59.
    pub const fn second(self) -> u32 {
        self.time.second()
    }

    /// Replaces the hour, or returns `None` if the hour is out of range.
    pub fn with_hour(self, hour: u32) -> Option<CivilDateTime> {
        self.with_time(hour, self.minute(), self.second())
    }

    /// Replaces the minute, or returns `None` if the minute is out of range.
    pub fn with_minute(self, minute: u32) -> Option<CivilDateTime> {
        self.with_time(self.hour(), minute, self.second())
    }

    /// Replaces the second, or returns `None` if the second is out of range.
    pub fn with_second(self, second: u32) -> Option<CivilDateTime> {
        self.with_time(self.hour(), self.minute(), second)
    }

    /// Replaces the time of day, or returns `None` if any part is out of range.
    pub fn with_time(self, hour: u32, minute: u32, second: u32) -> Option<CivilDateTime> {
        Some(CivilDateTime {
            time: CivilTime::from_hms_opt(hour, minute, second)?,
            ..self
        })
    }

    /// Adds a signed number of seconds, or returns `None` on overflow.
    ///
    /// The arithmetic is done on the wall clock, so it never skips or repeats a
    /// time. Daylight saving time is applied later, when the result is resolved
    /// in a time zone.
    pub fn checked_add_seconds(self, seconds: i64) -> Option<CivilDateTime> {
        let total = self.time.seconds_of_day().checked_add(seconds)?;
        // Croner searches second by second, so the result almost always stays
        // on the same day. That case needs no calendar arithmetic.
        let days = total.div_euclid(SECONDS_PER_DAY);
        let rest = total.rem_euclid(SECONDS_PER_DAY);
        let date = if days == 0 {
            self.date
        } else {
            self.date.checked_add_days(days)?
        };
        Some(CivilDateTime {
            date,
            time: CivilTime::from_seconds_of_day(rest),
        })
    }

    /// Adds a signed number of days, keeping the time of day, or returns `None`
    /// on overflow.
    pub fn checked_add_days(self, days: i64) -> Option<CivilDateTime> {
        Some(CivilDateTime {
            date: self.date.checked_add_days(days)?,
            ..self
        })
    }
}

impl CivilDate {
    /// Adds a time of day, or returns `None` if any part is out of range.
    pub fn and_hms_opt(self, hour: u32, minute: u32, second: u32) -> Option<CivilDateTime> {
        Some(CivilDateTime::new(
            self,
            CivilTime::from_hms_opt(hour, minute, second)?,
        ))
    }

    /// Adds a time of day, or returns [`CronError::InvalidTime`] if any part is
    /// out of range.
    pub fn and_hms(self, hour: u32, minute: u32, second: u32) -> Result<CivilDateTime, CronError> {
        self.and_hms_opt(hour, minute, second)
            .ok_or(CronError::InvalidTime)
    }
}

impl core::fmt::Display for CivilDate {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}

impl core::fmt::Display for CivilDateTime {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}T{}", self.date, self.time)
    }
}

/// The result of resolving a [`CivilDateTime`] in a time zone.
///
/// A wall clock time is not always one instant. When daylight saving time
/// starts, a range of wall clock times never happens, and when it ends, a range
/// happens twice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution<T> {
    /// The wall clock time happens exactly once.
    Single(T),

    /// The wall clock time happens twice, because the clock was set back.
    /// The first value is the earlier instant.
    Ambiguous(T, T),

    /// The wall clock time never happens, because the clock was set forward.
    Gap,
}

/// A date and time type that croner can search over.
///
/// Croner is generic over this trait, so [`Cron::find_next_occurrence`],
/// [`Cron::is_time_matching`] and the iterators return the same type that you
/// give them. Implement it to use a date and time library that croner does not
/// include.
///
/// [`Cron::find_next_occurrence`]: crate::Cron::find_next_occurrence
/// [`Cron::is_time_matching`]: crate::Cron::is_time_matching
///
/// # Example
///
/// ```
/// # #[cfg(feature = "chrono")] {
/// use std::str::FromStr as _;
///
/// use chrono::Utc;
/// use croner::Cron;
///
/// let cron = Cron::from_str("0 0 * * FRI").unwrap();
///
/// // The return type follows the argument type.
/// let next: chrono::DateTime<Utc> = cron.find_next_occurrence(&Utc::now(), false).unwrap();
/// # }
/// ```
pub trait CronDateTime: Sized + Clone {
    /// Returns the local wall clock date and time.
    fn to_civil(&self) -> CivilDateTime;

    /// Resolves a wall clock date and time in the same time zone as `self`.
    ///
    /// The returned values must carry the given wall clock time. Croner relies
    /// on this to check a pattern once for both halves of an ambiguous time.
    ///
    /// Types without a time zone always return [`Resolution::Single`].
    fn resolve_civil(&self, civil: CivilDateTime) -> Result<Resolution<Self>, CronError>;

    /// Adds a signed number of seconds of elapsed time.
    ///
    /// This moves along the absolute time line, so a daylight saving time shift
    /// changes the wall clock result. Returns `None` on overflow.
    fn checked_add_seconds(&self, seconds: i64) -> Option<Self>;
}

/// Croner's own wall clock type works with the search directly, so a pattern
/// can be evaluated with no date and time library at all.
impl CronDateTime for CivilDateTime {
    fn to_civil(&self) -> CivilDateTime {
        *self
    }

    fn resolve_civil(&self, civil: CivilDateTime) -> Result<Resolution<Self>, CronError> {
        // A wall clock time without a time zone always happens exactly once.
        Ok(Resolution::Single(civil))
    }

    fn checked_add_seconds(&self, seconds: i64) -> Option<Self> {
        CivilDateTime::checked_add_seconds(*self, seconds)
    }
}

/// Returns `true` if `year` is a leap year in the proleptic Gregorian calendar.
pub const fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

/// Returns the number of days in a month, or 0 if the month is out of range.
pub const fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

/// Converts a civil date to the number of days since 1970-01-01.
///
/// This is Howard Hinnant's `days_from_civil` algorithm, which is exact for the
/// whole proleptic Gregorian calendar.
fn days_from_civil(year: i32, month: u32, day: u32) -> i64 {
    let year = i64::from(year) - i64::from(month <= 2);
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400; // [0, 399]
    let day_of_year =
        (153 * (i64::from(month) + if month > 2 { -3 } else { 9 }) + 2) / 5 + i64::from(day) - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * DAYS_PER_ERA + day_of_era - DAYS_FROM_EPOCH_TO_ERA
}

/// Converts a number of days since 1970-01-01 to a civil date, or returns
/// `None` if the year does not fit in an `i32`.
///
/// This is Howard Hinnant's `civil_from_days` algorithm.
fn civil_from_days(days: i64) -> Option<(i32, u32, u32)> {
    let days = days.checked_add(DAYS_FROM_EPOCH_TO_ERA)?;
    let era = if days >= 0 {
        days
    } else {
        days - (DAYS_PER_ERA - 1)
    } / DAYS_PER_ERA;
    let day_of_era = days - era * DAYS_PER_ERA; // [0, 146096]
    let year_of_era = (day_of_era - day_of_era / 1460 + day_of_era / 36524
        - day_of_era / (DAYS_PER_ERA - 1))
        / 365; // [0, 399]
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153; // [0, 11]
    let day = (day_of_year - (153 * month_prime + 2) / 5 + 1) as u32; // [1, 31]
    let month = (month_prime + if month_prime < 10 { 3 } else { -9 }) as u32; // [1, 12]
    let year = year + i64::from(month <= 2);
    Some((i32::try_from(year).ok()?, month, day))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weekday_of_known_dates() {
        // 1970-01-01 was a Thursday.
        assert_eq!(
            CivilDate::from_ymd_opt(1970, 1, 1).unwrap().weekday(),
            Weekday::Thursday
        );
        // 2000-01-01 was a Saturday.
        assert_eq!(
            CivilDate::from_ymd_opt(2000, 1, 1).unwrap().weekday(),
            Weekday::Saturday
        );
        // 1969-12-31 was a Wednesday, one day before the epoch.
        assert_eq!(
            CivilDate::from_ymd_opt(1969, 12, 31).unwrap().weekday(),
            Weekday::Wednesday
        );
        // 0001-01-01 was a Monday in the proleptic Gregorian calendar.
        assert_eq!(
            CivilDate::from_ymd_opt(1, 1, 1).unwrap().weekday(),
            Weekday::Monday
        );
    }

    #[test]
    fn rejects_days_that_do_not_exist() {
        assert!(CivilDate::from_ymd_opt(2023, 2, 29).is_none());
        assert!(CivilDate::from_ymd_opt(2024, 2, 29).is_some());
        assert!(CivilDate::from_ymd_opt(2024, 4, 31).is_none());
        assert!(CivilDate::from_ymd_opt(2024, 13, 1).is_none());
        assert!(CivilDate::from_ymd_opt(2024, 0, 1).is_none());
        assert!(CivilDate::from_ymd_opt(2024, 1, 0).is_none());
    }

    #[test]
    fn day_arithmetic_crosses_month_and_year() {
        let date = CivilDate::from_ymd_opt(2024, 2, 28).unwrap();
        assert_eq!(
            date.checked_add_days(1).unwrap(),
            CivilDate::from_ymd_opt(2024, 2, 29).unwrap()
        );
        assert_eq!(
            date.checked_add_days(2).unwrap(),
            CivilDate::from_ymd_opt(2024, 3, 1).unwrap()
        );
        assert_eq!(
            CivilDate::from_ymd_opt(2023, 1, 1)
                .unwrap()
                .checked_add_days(-1)
                .unwrap(),
            CivilDate::from_ymd_opt(2022, 12, 31).unwrap()
        );
    }

    #[test]
    fn second_arithmetic_carries_into_the_next_day() {
        let time = CivilDateTime::from_ymd_hms(2023, 12, 31, 23, 59, 59).unwrap();
        assert_eq!(
            time.checked_add_seconds(1).unwrap(),
            CivilDateTime::from_ymd_hms(2024, 1, 1, 0, 0, 0).unwrap()
        );
        assert_eq!(
            CivilDateTime::from_ymd_hms(2024, 1, 1, 0, 0, 0)
                .unwrap()
                .checked_add_seconds(-1)
                .unwrap(),
            time
        );
    }

    #[test]
    fn second_arithmetic_before_the_epoch() {
        let time = CivilDateTime::from_ymd_hms(1900, 1, 1, 0, 0, 0).unwrap();
        assert_eq!(
            time.checked_add_seconds(-1).unwrap(),
            CivilDateTime::from_ymd_hms(1899, 12, 31, 23, 59, 59).unwrap()
        );
    }

    #[test]
    fn days_in_month_handles_leap_years() {
        assert_eq!(days_in_month(2023, 2), 28);
        assert_eq!(days_in_month(2024, 2), 29);
        assert_eq!(days_in_month(2000, 2), 29);
        assert_eq!(days_in_month(1900, 2), 28);
        assert_eq!(days_in_month(2024, 4), 30);
        // A month outside 1 to 12 has no days, which callers use as the single
        // check for both an out-of-range month and an out-of-range day.
        assert_eq!(days_in_month(2023, 0), 0);
        assert_eq!(days_in_month(2023, 13), 0);
    }

    #[test]
    fn day_count_round_trips_over_a_wide_range() {
        // Walk every day from -0100-01-01 to 3000-12-31 and check that the
        // conversion to and from a day count agrees with a plain counter, and
        // that the weekday advances by exactly one day each step. The range
        // starts before year 1 to cover the negative side of the calendar.
        let mut date = CivilDate::from_ymd_opt(-100, 1, 1).unwrap();
        let mut days = date.to_days();
        let mut weekday = date.weekday();
        while date.year() < 3001 {
            assert_eq!(CivilDate::from_days(days).unwrap(), date);
            assert_eq!(date.to_days(), days);
            assert_eq!(date.weekday(), weekday, "weekday at {date}");
            // The day count gives the weekday independently of the formula
            // that `weekday()` uses.
            assert_eq!(
                Weekday::from_days_from_sunday((days + 4).rem_euclid(7) as u32),
                weekday,
                "weekday from day count at {date}"
            );
            date = date.checked_add_days(1).unwrap();
            days += 1;
            weekday = Weekday::from_days_from_sunday((weekday.num_days_from_sunday() + 1) % 7);
        }
    }
}
