// Copyright 2024 tison <wander4096@gmail.com>
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! # Crontab
//!
//! A library to parse and drive the crontab expression.

use std::collections::BTreeSet;
use std::collections::HashSet;
use std::str::FromStr;

use jiff::civil::Weekday;
use jiff::tz::TimeZone;
use jiff::RoundMode;
use jiff::Span;
use jiff::Timestamp;
use jiff::ToSpan;
use jiff::Unit;
use jiff::Zoned;
use jiff::ZonedRound;

mod parser;
pub use parser::normalize_crontab;
pub use parser::parse_crontab;

#[derive(Debug, Clone, thiserror::Error)]
#[error("{0}")]
pub struct Error(String);

/// A data struct representing the crontab expression.
#[derive(Debug, Clone)]
pub struct Crontab {
    minutes: PossibleLiterals,
    hours: PossibleLiterals,
    months: PossibleLiterals,
    days_of_month: ParsedDaysOfMonth,
    days_of_week: ParsedDaysOfWeek,
    timezone: TimeZone,
}

#[derive(Debug)]
enum PossibleValue {
    /// Literally match the value.
    ///
    /// For example, a possible literal of minute '15' matches when the minute is '15'.
    Literal(u8),
    /// Parsed from '<day>W' in day-of-month field.
    ///
    /// The 'W' character is allowed for the day-of-month field. This character is used to specify
    /// the weekday (Monday-Friday) nearest the given day. As an example, if "15W" is specified as
    /// the value for the day-of-month field, the meaning is: "the nearest weekday to the 15th of
    /// the month." So, if the 15th is a Saturday, the trigger fires on Friday the 14th. If the
    /// 15th is a Sunday, the trigger fires on Monday the 16th. If the 15th is a Tuesday, then it
    /// fires on Tuesday the 15th. However, if "1W" is specified as the value for day-of-month, and
    /// the 1st is a Saturday, the trigger fires on Monday the 3rd, as it does not 'jump' over the
    /// boundary of a month's days. The 'W' character can be specified only when the day-of-month
    /// is a single day, not a range or list of days.
    NearestWeekday(u8),
    /// Parsed from '<day>L' in day-of-month field.
    ///
    /// 'L' stands for "last". When used in the day-of-month field, it specifies the last day of
    /// the month.
    LastDayOfMonth,
    /// Parsed from '<weekday>L' in day-of-week field.
    ///
    /// 'L' stands for "last". When used in the day-of-week field, it allows specifying constructs
    /// such as "the last Friday" ("5L") of a given month.
    LastDayOfWeek(Weekday),
    /// Parsed from '<weekday>#<nth>' in day-of-week field.
    ///
    /// '#' is allowed for the day-of-week field, and must be followed by a number between one and
    /// five. It allows specifying constructs such as "the second Friday" of a given month. For
    /// example, entering "5#3" in the day-of-week field corresponds to the third Friday of every
    /// month.
    NthDayOfWeek(u8, Weekday),
}

#[derive(Debug, Clone)]
struct PossibleLiterals {
    values: BTreeSet<u8>,
}

impl PossibleLiterals {
    fn matches(&self, value: u8) -> bool {
        self.values.contains(&value)
    }
}

#[derive(Debug, Clone)]
struct ParsedDaysOfWeek {
    literals: BTreeSet<u8>,
    last_days_of_week: HashSet<Weekday>,
    nth_days_of_week: HashSet<(u8, Weekday)>,

    // to implement vixie's cron behavior
    // ref - https://crontab.guru/cron-bug.html
    start_with_asterisk: bool,
}

impl ParsedDaysOfWeek {
    fn matches(&self, value: &Zoned) -> bool {
        if self.literals.contains(&(value.weekday() as u8)) {
            return true;
        }

        for weekday in self.last_days_of_week.iter() {
            if value.weekday() != *weekday {
                continue;
            }

            if (value + 1.week()).month() > value.month() {
                return true;
            }
        }

        for (nth, weekday) in self.nth_days_of_week.iter() {
            if value.weekday() != *weekday {
                continue;
            }

            if let Ok(nth_weekday) = value.nth_weekday_of_month(*nth as i8, *weekday) {
                if nth_weekday.date() == value.date() {
                    return true;
                }
            }
        }

        false
    }
}

#[derive(Debug, Clone)]
struct ParsedDaysOfMonth {
    literals: BTreeSet<u8>,
    last_day_of_month: bool,
    nearest_weekdays: BTreeSet<u8>,

    // to implement vixie's cron behavior
    // ref - https://crontab.guru/cron-bug.html
    start_with_asterisk: bool,
}

impl ParsedDaysOfMonth {
    fn matches(&self, value: &Zoned) -> bool {
        if self.literals.contains(&(value.day() as u8)) {
            return true;
        }

        if self.last_day_of_month && (value + 1.day()).month() > value.month() {
            return true;
        }

        for day in self.nearest_weekdays.iter() {
            let day = *day as i8;

            match value.weekday() {
                // 'nearest weekday' matcher can never match weekends
                Weekday::Saturday | Weekday::Sunday => {
                    continue;
                }
                // if today is Tuesday, Wednesday, or Thursday, only if the day matches today can
                // today be the nearest weekday
                Weekday::Tuesday | Weekday::Wednesday | Weekday::Thursday => {
                    if value.day() == day {
                        return true;
                    }
                }
                Weekday::Monday => {
                    // if the day matches today, today is the nearest weekday
                    if value.day() == day {
                        return true;
                    }

                    // matches the last Sunday
                    if value.day() - 1 == day {
                        return true;
                    }

                    // matches the edge case: 1W and the 1st is Saturday
                    if value.day() == 3 && day == 1 {
                        return true;
                    }
                }
                Weekday::Friday => {
                    // if the day matches today, today is the nearest weekday
                    if value.day() == day {
                        return true;
                    }

                    let last_day_of_this_month = value.days_in_month();

                    // matches the next Saturday
                    if value.day() + 1 == day && day <= last_day_of_this_month {
                        return true;
                    }

                    // matches the edge case: last day of month is Sunday
                    if value.day() + 2 == day && day == last_day_of_this_month {
                        return true;
                    }
                }
            }
        }

        false
    }
}

impl FromStr for Crontab {
    type Err = Error;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        parse_crontab(input)
    }
}

impl<'a> TryFrom<&'a str> for Crontab {
    type Error = Error;

    fn try_from(input: &'a str) -> Result<Self, Self::Error> {
        FromStr::from_str(input)
    }
}

impl Crontab {
    pub fn drive(&self, timestamp: Timestamp) -> Driver {
        Driver {
            crontab: self.clone(),
            timestamp,
        }
    }

    pub fn find_next(&self, timestamp: impl TryInto<Timestamp>) -> Result<Zoned, Error> {
        let timestamp = timestamp
            .try_into()
            .map_err(|_| Error("failed to parse timestamp".to_string()))?;
        let zoned = timestamp.to_zoned(self.timezone.clone());

        // checked at most 4 years to cover the leap year case
        let bound = &zoned + 4.years();

        // at least should be the next minutes
        let mut next = zoned;
        next = advance_time_and_round(next, 1.minute(), Some(Unit::Minute))?;

        loop {
            if next > bound {
                return Err(Error(format!(
                    "failed to find next timestamp in four years; end with {next}"
                )));
            }

            if !self.months.matches(next.month() as u8) {
                let rest_days = next.days_in_month() - next.day() + 1;
                next = advance_time_and_round(next, rest_days.days(), Some(Unit::Day))?;
                continue;
            }

            // TODO(tisonkun): there is a bug in crontab to behavior differently from the next lines
            //  when both days_of_month and days_of_week are set; figure out how to handle it:
            //  https://crontab.guru/cron-bug.html

            if !self.days_of_month.matches(&next) {
                next = advance_time_and_round(next, 1.day(), Some(Unit::Day))?;
                continue;
            }

            if !self.hours.matches(next.hour() as u8) {
                next = advance_time_and_round(next, 1.hour(), Some(Unit::Hour))?;
                continue;
            }

            if !self.minutes.matches(next.minute() as u8) {
                next = advance_time_and_round(next, 1.minute(), Some(Unit::Minute))?;
                continue;
            }

            if !self.days_of_week.matches(&next) {
                next = advance_time_and_round(next, 1.day(), Some(Unit::Day))?;
                continue;
            }

            break Ok(next);
        }
    }
}

#[derive(Debug)]
pub struct DriverOption {
    start: Timestamp,
    end: Option<Timestamp>,
}

impl Default for DriverOption {
    fn default() -> Self {
        DriverOption {
            start: Timestamp::now(),
            end: None,
        }
    }
}

/// Driver to find the next timestamp from the given crontab and timestamp,
/// or iterate the next timestamps.
///
/// Call [Crontab::drive_with_timestamp] or [Crontab::drive_with_timestamp_millis]
/// to obtain an instance of [`Driver`].
#[derive(Debug)]
pub struct Driver {
    crontab: Crontab,
    timestamp: Timestamp,
}

impl Driver {
    /// Iterate to the next timestamp as a [`Zoned`] struct.
    pub fn next_zoned(&mut self) -> Result<Zoned, Error> {
        let timestamp = self.find_next_timestamp()?;
        self.timestamp = timestamp;
        Ok(timestamp.to_zoned(self.crontab.timezone.clone()))
    }

    /// Iterate to the next timestamp as a [`Timestamp`] struct.
    pub fn next_timestamp(&mut self) -> Result<Timestamp, Error> {
        let timestamp = self.find_next_timestamp()?;
        self.timestamp = timestamp;
        Ok(timestamp)
    }

    /// Find the next timestamp as a [`Zoned`] struct.
    pub fn find_next_zoned(&self) -> Result<Zoned, Error> {
        let timezone = self.crontab.timezone.clone();
        self.find_next_timestamp().map(|ts| ts.to_zoned(timezone))
    }

    /// Find the next timestamp as a [`Timestamp`] struct.
    pub fn find_next_timestamp(&self) -> Result<Timestamp, Error> {
        let zoned = self.crontab.find_next(self.timestamp)?;
        Ok(zoned.timestamp())
    }
}

fn advance_time_and_round(zoned: Zoned, span: Span, unit: Option<Unit>) -> Result<Zoned, Error> {
    let mut next = zoned;

    next = next
        .checked_add(span)
        .map_err(time_error_with_context(&format!(
            "failed to advance timestamp; end with {next}"
        )))?;

    if let Some(unit) = unit {
        next = next
            .round(ZonedRound::new().mode(RoundMode::Trunc).smallest(unit))
            .map_err(time_error_with_context(&format!(
                "failed to round timestamp; end with {next}"
            )))?;
    }

    Ok(next)
}

fn time_error_with_context(context: &str) -> impl FnOnce(jiff::Error) -> Error + '_ {
    move |error| Error(format!("{context}: {error}"))
}

#[cfg(test)]
fn setup_logging() {
    use logforth::append;
    use logforth::filter::EnvFilter;
    use logforth::layout::TextLayout;
    use logforth::Dispatch;
    use logforth::Logger;

    static SETUP_LOGGING: std::sync::Once = std::sync::Once::new();
    SETUP_LOGGING.call_once(|| {
        Logger::new()
            .dispatch(
                Dispatch::new()
                    .filter(EnvFilter::from_default_env_or("DEBUG"))
                    .layout(TextLayout::default())
                    .append(append::Stderr),
            )
            .apply()
            .unwrap();
    });
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use insta::assert_snapshot;
    use jiff::Timestamp;

    use crate::setup_logging;
    use crate::Crontab;
    use crate::Driver;

    fn make_driver(crontab: &str, timestamp: &str) -> Driver {
        let timestamp = Timestamp::from_str(timestamp).unwrap();
        let crontab = Crontab::from_str(crontab).unwrap();
        crontab.drive(timestamp)
    }

    #[test]
    fn test_next_timestamp() {
        setup_logging();

        let driver = make_driver("0 0 1 1 * Asia/Shanghai", "2024-01-01T00:00:00+08:00");
        assert_snapshot!(driver.find_next_zoned().unwrap(), @"2025-01-01T00:00:00+08:00[Asia/Shanghai]");

        let mut driver = make_driver("2 4 * * * Asia/Shanghai", "2024-09-11T19:08:35+08:00");
        assert_snapshot!(driver.next_zoned().unwrap(), @"2024-09-12T04:02:00+08:00[Asia/Shanghai]");
        assert_snapshot!(driver.next_zoned().unwrap(), @"2024-09-13T04:02:00+08:00[Asia/Shanghai]");
        assert_snapshot!(driver.next_zoned().unwrap(), @"2024-09-14T04:02:00+08:00[Asia/Shanghai]");
        assert_snapshot!(driver.next_zoned().unwrap(), @"2024-09-15T04:02:00+08:00[Asia/Shanghai]");
        assert_snapshot!(driver.next_zoned().unwrap(), @"2024-09-16T04:02:00+08:00[Asia/Shanghai]");

        let mut driver = make_driver("0 0 31 * * Asia/Shanghai", "2024-09-11T19:08:35+08:00");
        assert_snapshot!(driver.next_zoned().unwrap(), @"2024-10-31T00:00:00+08:00[Asia/Shanghai]");
        assert_snapshot!(driver.next_zoned().unwrap(), @"2024-12-31T00:00:00+08:00[Asia/Shanghai]");
        assert_snapshot!(driver.next_zoned().unwrap(), @"2025-01-31T00:00:00+08:00[Asia/Shanghai]");
        assert_snapshot!(driver.next_zoned().unwrap(), @"2025-03-31T00:00:00+08:00[Asia/Shanghai]");
        assert_snapshot!(driver.next_zoned().unwrap(), @"2025-05-31T00:00:00+08:00[Asia/Shanghai]");
        assert_snapshot!(driver.next_zoned().unwrap(), @"2025-07-31T00:00:00+08:00[Asia/Shanghai]");
        assert_snapshot!(driver.next_zoned().unwrap(), @"2025-08-31T00:00:00+08:00[Asia/Shanghai]");
        assert_snapshot!(driver.next_zoned().unwrap(), @"2025-10-31T00:00:00+08:00[Asia/Shanghai]");
        assert_snapshot!(driver.next_zoned().unwrap(), @"2025-12-31T00:00:00+08:00[Asia/Shanghai]");
        assert_snapshot!(driver.next_zoned().unwrap(), @"2026-01-31T00:00:00+08:00[Asia/Shanghai]");
        assert_snapshot!(driver.next_zoned().unwrap(), @"2026-03-31T00:00:00+08:00[Asia/Shanghai]");
        assert_snapshot!(driver.next_zoned().unwrap(), @"2026-05-31T00:00:00+08:00[Asia/Shanghai]");

        let mut driver = make_driver("0 18 * * 1-5 Asia/Shanghai", "2024-09-11T19:08:35+08:00");
        assert_snapshot!(driver.next_zoned().unwrap(), @"2024-09-12T18:00:00+08:00[Asia/Shanghai]");
        assert_snapshot!(driver.next_zoned().unwrap(), @"2024-09-13T18:00:00+08:00[Asia/Shanghai]");
        assert_snapshot!(driver.next_zoned().unwrap(), @"2024-09-16T18:00:00+08:00[Asia/Shanghai]");
        assert_snapshot!(driver.next_zoned().unwrap(), @"2024-09-17T18:00:00+08:00[Asia/Shanghai]");
        assert_snapshot!(driver.next_zoned().unwrap(), @"2024-09-18T18:00:00+08:00[Asia/Shanghai]");
        assert_snapshot!(driver.next_zoned().unwrap(), @"2024-09-19T18:00:00+08:00[Asia/Shanghai]");

        let mut driver = make_driver("0 18 * * TUE#1 Asia/Shanghai", "2024-09-24T00:08:35+08:00");
        assert_snapshot!(driver.next_zoned().unwrap(), @"2024-10-01T18:00:00+08:00[Asia/Shanghai]");
        assert_snapshot!(driver.next_zoned().unwrap(), @"2024-11-05T18:00:00+08:00[Asia/Shanghai]");
        assert_snapshot!(driver.next_zoned().unwrap(), @"2024-12-03T18:00:00+08:00[Asia/Shanghai]");
        assert_snapshot!(driver.next_zoned().unwrap(), @"2025-01-07T18:00:00+08:00[Asia/Shanghai]");
        assert_snapshot!(driver.next_zoned().unwrap(), @"2025-02-04T18:00:00+08:00[Asia/Shanghai]");
        assert_snapshot!(driver.next_zoned().unwrap(), @"2025-03-04T18:00:00+08:00[Asia/Shanghai]");
        assert_snapshot!(driver.next_zoned().unwrap(), @"2025-04-01T18:00:00+08:00[Asia/Shanghai]");

        let mut driver = make_driver("4 2 * * 1L Asia/Shanghai", "2024-09-24T00:08:35+08:00");
        assert_snapshot!(driver.next_zoned().unwrap(), @"2024-09-30T02:04:00+08:00[Asia/Shanghai]");
        assert_snapshot!(driver.next_zoned().unwrap(), @"2024-10-28T02:04:00+08:00[Asia/Shanghai]");
        assert_snapshot!(driver.next_zoned().unwrap(), @"2024-11-25T02:04:00+08:00[Asia/Shanghai]");
        assert_snapshot!(driver.next_zoned().unwrap(), @"2025-01-27T02:04:00+08:00[Asia/Shanghai]");
        assert_snapshot!(driver.next_zoned().unwrap(), @"2025-02-24T02:04:00+08:00[Asia/Shanghai]");
        assert_snapshot!(driver.next_zoned().unwrap(), @"2025-03-31T02:04:00+08:00[Asia/Shanghai]");
        assert_snapshot!(driver.next_zoned().unwrap(), @"2025-04-28T02:04:00+08:00[Asia/Shanghai]");

        let mut driver = make_driver("0 18 * * FRI#5 Asia/Shanghai", "2024-09-24T00:08:35+08:00");
        assert_snapshot!(driver.next_zoned().unwrap(), @"2024-11-29T18:00:00+08:00[Asia/Shanghai]");
        assert_snapshot!(driver.next_zoned().unwrap(), @"2025-01-31T18:00:00+08:00[Asia/Shanghai]");
        assert_snapshot!(driver.next_zoned().unwrap(), @"2025-05-30T18:00:00+08:00[Asia/Shanghai]");
        assert_snapshot!(driver.next_zoned().unwrap(), @"2025-08-29T18:00:00+08:00[Asia/Shanghai]");
        assert_snapshot!(driver.next_zoned().unwrap(), @"2025-10-31T18:00:00+08:00[Asia/Shanghai]");
        assert_snapshot!(driver.next_zoned().unwrap(), @"2026-01-30T18:00:00+08:00[Asia/Shanghai]");
        assert_snapshot!(driver.next_zoned().unwrap(), @"2026-05-29T18:00:00+08:00[Asia/Shanghai]");

        let mut driver = make_driver(
            "3 11 L JAN-FEB,5 * Asia/Shanghai",
            "2024-09-24T00:08:35+08:00",
        );
        assert_snapshot!(driver.next_zoned().unwrap(), @"2025-01-31T11:03:00+08:00[Asia/Shanghai]");
        assert_snapshot!(driver.next_zoned().unwrap(), @"2025-02-28T11:03:00+08:00[Asia/Shanghai]");
        assert_snapshot!(driver.next_zoned().unwrap(), @"2025-05-31T11:03:00+08:00[Asia/Shanghai]");
        assert_snapshot!(driver.next_zoned().unwrap(), @"2026-01-31T11:03:00+08:00[Asia/Shanghai]");
        assert_snapshot!(driver.next_zoned().unwrap(), @"2026-02-28T11:03:00+08:00[Asia/Shanghai]");
        assert_snapshot!(driver.next_zoned().unwrap(), @"2026-05-31T11:03:00+08:00[Asia/Shanghai]");

        let mut driver = make_driver("3 11 17W,L * * Asia/Shanghai", "2024-09-24T00:08:35+08:00");
        assert_snapshot!(driver.next_zoned().unwrap(), @"2024-09-30T11:03:00+08:00[Asia/Shanghai]");
        assert_snapshot!(driver.next_zoned().unwrap(), @"2024-10-17T11:03:00+08:00[Asia/Shanghai]");
        assert_snapshot!(driver.next_zoned().unwrap(), @"2024-10-31T11:03:00+08:00[Asia/Shanghai]");
        assert_snapshot!(driver.next_zoned().unwrap(), @"2024-11-18T11:03:00+08:00[Asia/Shanghai]");
        assert_snapshot!(driver.next_zoned().unwrap(), @"2024-11-30T11:03:00+08:00[Asia/Shanghai]");
        assert_snapshot!(driver.next_zoned().unwrap(), @"2024-12-17T11:03:00+08:00[Asia/Shanghai]");

        let mut driver = make_driver("3 11 1W * * Asia/Shanghai", "2024-09-24T00:08:35+08:00");
        assert_snapshot!(driver.next_zoned().unwrap(), @"2024-10-01T11:03:00+08:00[Asia/Shanghai]");
        assert_snapshot!(driver.next_zoned().unwrap(), @"2024-11-01T11:03:00+08:00[Asia/Shanghai]");
        assert_snapshot!(driver.next_zoned().unwrap(), @"2024-12-02T11:03:00+08:00[Asia/Shanghai]");
        assert_snapshot!(driver.next_zoned().unwrap(), @"2025-01-01T11:03:00+08:00[Asia/Shanghai]");
        assert_snapshot!(driver.next_zoned().unwrap(), @"2025-02-03T11:03:00+08:00[Asia/Shanghai]");
        assert_snapshot!(driver.next_zoned().unwrap(), @"2025-03-03T11:03:00+08:00[Asia/Shanghai]");

        let mut driver = make_driver("3 11 31W * * Asia/Shanghai", "2024-09-24T00:08:35+08:00");
        assert_snapshot!(driver.next_zoned().unwrap(), @"2024-10-31T11:03:00+08:00[Asia/Shanghai]");
        assert_snapshot!(driver.next_zoned().unwrap(), @"2024-12-31T11:03:00+08:00[Asia/Shanghai]");
        assert_snapshot!(driver.next_zoned().unwrap(), @"2025-01-31T11:03:00+08:00[Asia/Shanghai]");
        assert_snapshot!(driver.next_zoned().unwrap(), @"2025-03-31T11:03:00+08:00[Asia/Shanghai]");
        assert_snapshot!(driver.next_zoned().unwrap(), @"2025-05-30T11:03:00+08:00[Asia/Shanghai]");
        assert_snapshot!(driver.next_zoned().unwrap(), @"2025-07-31T11:03:00+08:00[Asia/Shanghai]");
        assert_snapshot!(driver.next_zoned().unwrap(), @"2025-08-29T11:03:00+08:00[Asia/Shanghai]");
        assert_snapshot!(driver.next_zoned().unwrap(), @"2025-10-31T11:03:00+08:00[Asia/Shanghai]");
    }
}
