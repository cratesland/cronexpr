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

use std::collections::BTreeSet;
use std::collections::HashSet;
use std::ops::RangeInclusive;

use jiff::civil::Weekday;
use winnow::ascii::dec_uint;
use winnow::combinator::alt;
use winnow::combinator::eof;
use winnow::combinator::separated;
use winnow::error::ContextError;
use winnow::error::ErrMode;
use winnow::error::ErrorKind;
use winnow::error::FromExternalError;
use winnow::stream::Stream;
use winnow::token::take_while;
use winnow::PResult;
use winnow::Parser;

use crate::Crontab;
use crate::Error;
use crate::PossibleDaysOfWeek;
use crate::PossibleLiterals;
use crate::PossibleValue;

/// Normalize a crontab expression to compact form.
///
/// ```rust
/// use cronexpr::normalize_crontab;
///
/// assert_eq!(
///     normalize_crontab("  *   * * * * Asia/Shanghai  "),
///     "* * * * * Asia/Shanghai"
/// );
/// assert_eq!(
///     normalize_crontab("  2\t4 * * *\nAsia/Shanghai  "),
///     "2 4 * * * Asia/Shanghai"
/// );
/// ```
pub fn normalize_crontab(input: &str) -> String {
    input
        .split_ascii_whitespace()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Parse a crontab expression to [`Crontab`].
///
/// ```rust
/// use cronexpr::parse_crontab;
///
/// parse_crontab("* * * * * Asia/Shanghai").unwrap();
/// parse_crontab("2 4 * * * Asia/Shanghai").unwrap();
/// parse_crontab("2 4 * * 0-6 Asia/Shanghai").unwrap();
/// parse_crontab("2 4 */3 * 0-6 Asia/Shanghai").unwrap();
/// ```
pub fn parse_crontab(input: &str) -> Result<Crontab, Error> {
    let normalized = normalize_crontab(input);

    log::debug!("normalized input {input:?} to {normalized:?}");

    let minutes_start = 0;
    let minutes_end = normalized.find(' ').unwrap_or(normalized.len());
    let minutes_part = &normalized[..minutes_end];
    let minutes = parse_minutes
        .parse(minutes_part)
        .map_err(|err| format_parse_error(&normalized, minutes_start, err))?;

    let hours_start = minutes_end + 1;
    let hours_end = normalized[hours_start..]
        .find(' ')
        .map(|i| i + hours_start)
        .unwrap_or_else(|| normalized.len());
    let hours_part = &normalized[hours_start..hours_end];
    let hours = parse_hours
        .parse(hours_part)
        .map_err(|err| format_parse_error(&normalized, hours_start, err))?;

    let days_of_month_start = hours_end + 1;
    let days_of_month_end = normalized[days_of_month_start..]
        .find(' ')
        .map(|i| i + days_of_month_start)
        .unwrap_or_else(|| normalized.len());
    let days_of_month_part = &normalized[days_of_month_start..days_of_month_end];
    let days_of_month = parse_days_of_month
        .parse(days_of_month_part)
        .map_err(|err| format_parse_error(&normalized, days_of_month_start, err))?;

    let months_start = days_of_month_end + 1;
    let months_end = normalized[months_start..]
        .find(' ')
        .map(|i| i + months_start)
        .unwrap_or_else(|| normalized.len());
    let months_part = &normalized[months_start..months_end];
    let months = parse_months
        .parse(months_part)
        .map_err(|err| format_parse_error(&normalized, months_start, err))?;

    let days_of_week_start = months_end + 1;
    let days_of_week_end = normalized[days_of_week_start..]
        .find(' ')
        .map(|i| i + days_of_week_start)
        .unwrap_or_else(|| normalized.len());
    let days_of_week_part = &normalized[days_of_week_start..days_of_week_end];
    let days_of_week = parse_days_of_week
        .parse(days_of_week_part)
        .map_err(|err| format_parse_error(&normalized, days_of_week_start, err))?;

    let timezone_start = days_of_week_end + 1;
    let timezone_end = normalized.len();
    let timezone_part = &normalized[timezone_start..timezone_end];
    let timezone = parse_timezone
        .parse(timezone_part)
        .map_err(|err| format_parse_error(&normalized, timezone_start, err))?;

    Ok(Crontab {
        minutes,
        hours,
        days_of_month,
        months,
        days_of_week,
        timezone,
    })
}

fn format_parse_error(
    input: &str,
    start: usize,
    parse_error: winnow::error::ParseError<&str, ContextError>,
) -> Error {
    let context = "failed to parse crontab expression";

    let offset = start + parse_error.offset();
    let indent = " ".repeat(offset);

    let error = parse_error.into_inner().to_string();
    let error = if error.is_empty() {
        "malformed expression"
    } else {
        &error
    };

    Error(format!("{context}:\n{input}\n{indent}^ {error}"))
}

fn parse_minutes(input: &mut &str) -> PResult<PossibleLiterals> {
    do_parse_number_only(|| 0..=59, input)
}

fn parse_hours(input: &mut &str) -> PResult<PossibleLiterals> {
    do_parse_number_only(|| 0..=23, input)
}

fn parse_months(input: &mut &str) -> PResult<PossibleLiterals> {
    do_parse_number_only(|| 1..=12, input)
}

fn parse_days_of_week(input: &mut &str) -> PResult<PossibleDaysOfWeek> {
    let range = || 0..=7;

    fn norm_sunday(n: u8) -> u8 {
        if n != 0 {
            n
        } else {
            7
        }
    }

    fn make_weekday(n: u8) -> Weekday {
        let weekday = norm_sunday(n) as i8;
        Weekday::from_monday_one_offset(weekday).expect("{weekday} must be in range 1..=7")
    }

    fn parse_single_day_of_week<'a>(
        range: fn() -> RangeInclusive<u8>,
    ) -> impl Parser<&'a str, u8, ContextError> {
        alt((
            "SUN".map(|_| 0),
            "MON".map(|_| 1),
            "TUE".map(|_| 2),
            "WED".map(|_| 3),
            "THU".map(|_| 4),
            "FRI".map(|_| 5),
            "SAT".map(|_| 6),
            parse_single_number(range),
        ))
    }

    fn parse_single_day_of_week_ext<'a>(
        range: fn() -> RangeInclusive<u8>,
    ) -> impl Parser<&'a str, PossibleValue, ContextError> {
        alt((
            (parse_single_day_of_week(range), "L")
                .map(|(n, _)| PossibleValue::LastDayOfWeek(make_weekday(n))),
            (parse_single_day_of_week(range), "#", dec_uint)
                .map(|(n, _, nth): (u8, _, u8)| PossibleValue::NthDayOfWeek(nth, make_weekday(n))),
            parse_single_day_of_week(range).map(|n| PossibleValue::Literal(norm_sunday(n))),
        ))
    }

    let values = parse_list(alt((
        parse_step(range, parse_single_day_of_week).map(|r| {
            r.into_iter()
                .map(norm_sunday)
                .map(PossibleValue::Literal)
                .collect::<Vec<_>>()
        }),
        parse_range(range, parse_single_day_of_week).map(|r| {
            r.into_iter()
                .map(norm_sunday)
                .map(PossibleValue::Literal)
                .collect::<Vec<_>>()
        }),
        parse_single_day_of_week_ext(range).map(|n| vec![n]),
        parse_asterisk(range).map(|r| {
            r.into_iter()
                .map(norm_sunday)
                .map(PossibleValue::Literal)
                .collect::<Vec<_>>()
        }),
    )))
    .parse_next(input)?;

    let mut literals = BTreeSet::new();
    let mut last_days_of_week = HashSet::new();
    let mut nth_days_of_week = HashSet::new();
    for value in values {
        match value {
            PossibleValue::Literal(value) => {
                literals.insert(value);
            }
            PossibleValue::LastDayOfWeek(weekday) => {
                last_days_of_week.insert(weekday);
            }
            PossibleValue::NthDayOfWeek(nth, weekday) => {
                nth_days_of_week.insert((nth, weekday));
            }
        }
    }
    Ok(PossibleDaysOfWeek {
        literals,
        last_days_of_week,
        nth_days_of_week,
    })
}

fn parse_days_of_month(input: &mut &str) -> PResult<PossibleLiterals> {
    do_parse_number_only(|| 1..=31, input)
}

fn parse_timezone(input: &mut &str) -> PResult<jiff::tz::TimeZone> {
    take_while(0.., |_| true)
        .try_map_cut(|timezone| {
            jiff::tz::TimeZone::get(timezone).map_err(|_| {
                Error(format!(
                    "failed to find timezone {timezone}; \
                for a list of time zones, see the list of tz database time zones on Wikipedia: \
                https://en.wikipedia.org/wiki/List_of_tz_database_time_zones#List"
                ))
            })
        })
        .parse_next(input)
}

// number only = minutes, hours, or months
fn do_parse_number_only(
    range: fn() -> RangeInclusive<u8>,
    input: &mut &str,
) -> PResult<PossibleLiterals> {
    let values = parse_list(alt((
        parse_step(range, parse_single_number).map(|r| {
            r.into_iter()
                .map(PossibleValue::Literal)
                .collect::<Vec<_>>()
        }),
        parse_range(range, parse_single_number).map(|r| {
            r.into_iter()
                .map(PossibleValue::Literal)
                .collect::<Vec<_>>()
        }),
        parse_single_number(range).map(|n| vec![PossibleValue::Literal(n)]),
        parse_asterisk(range).map(|r| {
            r.into_iter()
                .map(PossibleValue::Literal)
                .collect::<Vec<_>>()
        }),
    )))
    .parse_next(input)?;

    let mut literals = BTreeSet::new();
    for value in values {
        match value {
            PossibleValue::Literal(value) => {
                literals.insert(value);
            }
            _ => unreachable!("unexpected value: {value:?}"),
        }
    }
    Ok(PossibleLiterals { values: literals })
}

fn parse_asterisk<'a>(
    range: fn() -> RangeInclusive<u8>,
) -> impl Parser<&'a str, Vec<u8>, ContextError> {
    "*".map(move |_| range().collect())
}

fn parse_single_number<'a>(
    range: fn() -> RangeInclusive<u8>,
) -> impl Parser<&'a str, u8, ContextError> {
    dec_uint.try_map_cut(move |n: u64| {
        let range = range();

        if n > u8::MAX as u64 {
            return Err(Error(format!(
                "value must be in range {range:?}; found {n}"
            )));
        }

        let n = n as u8;
        if range.contains(&n) {
            Ok(n)
        } else {
            Err(Error(format!(
                "value must be in range {range:?}; found {n}"
            )))
        }
    })
}

fn parse_range<'a, P>(
    range: fn() -> RangeInclusive<u8>,
    parse_single_range_bound: fn(fn() -> RangeInclusive<u8>) -> P,
) -> impl Parser<&'a str, Vec<u8>, ContextError>
where
    P: Parser<&'a str, u8, ContextError>,
{
    (
        parse_single_range_bound(range),
        "-",
        parse_single_range_bound(range),
    )
        .try_map_cut(move |(lo, _, hi): (u8, _, u8)| {
            let range = range();

            if lo > hi {
                return Err(Error(format!(
                    "range must be in ascending order; found {lo}-{hi}"
                )));
            }

            if range.contains(&lo) && range.contains(&hi) {
                Ok((lo..=hi).collect())
            } else {
                Err(Error(format!(
                    "range must be in range {range:?}; found {lo}-{hi}"
                )))
            }
        })
}

fn parse_step<'a, P>(
    range: fn() -> RangeInclusive<u8>,
    parse_single_range_bound: fn(fn() -> RangeInclusive<u8>) -> P,
) -> impl Parser<&'a str, Vec<u8>, ContextError>
where
    P: Parser<&'a str, u8, ContextError>,
{
    let range_end = *range().end();

    let possible_values = alt((
        parse_asterisk(range),
        parse_range(range, parse_single_range_bound),
        parse_single_range_bound(range).map(move |n| (n..=range_end).collect()),
    ));

    (possible_values, "/", dec_uint).try_map_cut(move |(candidates, _, step): (Vec<u8>, _, u64)| {
        let range = range();

        if step == 0 {
            return Err(Error("step must be greater than 0".to_string()));
        }

        if step > u8::MAX as u64 {
            return Err(Error(format!(
                "step must be in range {range:?}; found {step}"
            )));
        }

        let step = step as u8;
        if !range.contains(&step) {
            return Err(Error(format!(
                "step must be in range {range:?}; found {step}"
            )));
        }

        let mut values = Vec::new();
        for n in candidates.into_iter().step_by(step as usize) {
            values.push(n);
        }
        Ok(values)
    })
}

fn parse_list<'a, P>(parse_list_item: P) -> impl Parser<&'a str, Vec<PossibleValue>, ContextError>
where
    P: Parser<&'a str, Vec<PossibleValue>, ContextError>,
{
    (separated(1.., parse_list_item, ","), eof)
        .map(move |(ns, _): (Vec<Vec<PossibleValue>>, _)| ns.into_iter().flatten().collect())
}

trait ParserExt<I, O, E>: Parser<I, O, E> {
    #[inline(always)]
    fn try_map_cut<G, O2, E2>(self, map: G) -> TryMapCut<Self, G, I, O, O2, E, E2>
    where
        Self: Sized,
        G: FnMut(O) -> Result<O2, E2>,
        I: Stream,
        E: FromExternalError<I, E2>,
    {
        TryMapCut::new(self, map)
    }
}

struct TryMapCut<F, G, I, O, O2, E, E2>
where
    F: Parser<I, O, E>,
    G: FnMut(O) -> Result<O2, E2>,
    I: Stream,
    E: FromExternalError<I, E2>,
{
    parser: F,
    map: G,
    i: core::marker::PhantomData<I>,
    o: core::marker::PhantomData<O>,
    o2: core::marker::PhantomData<O2>,
    e: core::marker::PhantomData<E>,
    e2: core::marker::PhantomData<E2>,
}

impl<F, G, I, O, O2, E, E2> TryMapCut<F, G, I, O, O2, E, E2>
where
    F: Parser<I, O, E>,
    G: FnMut(O) -> Result<O2, E2>,
    I: Stream,
    E: FromExternalError<I, E2>,
{
    #[inline(always)]
    fn new(parser: F, map: G) -> Self {
        Self {
            parser,
            map,
            i: Default::default(),
            o: Default::default(),
            o2: Default::default(),
            e: Default::default(),
            e2: Default::default(),
        }
    }
}

impl<F, G, I, O, O2, E, E2> Parser<I, O2, E> for TryMapCut<F, G, I, O, O2, E, E2>
where
    F: Parser<I, O, E>,
    G: FnMut(O) -> Result<O2, E2>,
    I: Stream,
    E: FromExternalError<I, E2>,
{
    #[inline]
    fn parse_next(&mut self, input: &mut I) -> PResult<O2, E> {
        let start = input.checkpoint();
        let o = self.parser.parse_next(input)?;

        (self.map)(o).map_err(|err| {
            input.reset(&start);
            ErrMode::from_external_error(input, ErrorKind::Verify, err).cut()
        })
    }
}

impl<I, O, E, P> ParserExt<I, O, E> for P where P: Parser<I, O, E> {}

#[cfg(test)]
mod tests {
    use insta::assert_debug_snapshot;
    use insta::assert_snapshot;

    use super::*;
    use crate::setup_logging;

    #[test]
    fn test_parse_crontab_success() {
        setup_logging();

        assert_debug_snapshot!(parse_crontab("* * * * * Asia/Shanghai").unwrap());
        assert_debug_snapshot!(parse_crontab("2 4 * * * Asia/Shanghai").unwrap());
        assert_debug_snapshot!(parse_crontab("2 4 * * 0-6 Asia/Shanghai").unwrap());
        assert_debug_snapshot!(parse_crontab("2 4 */3 * 0-6 Asia/Shanghai").unwrap());
        assert_debug_snapshot!(parse_crontab("*/2 1 1 1 * Asia/Shanghai").unwrap());
        assert_debug_snapshot!(parse_crontab("1/2 1 1 1 * Asia/Shanghai").unwrap());
        assert_debug_snapshot!(parse_crontab("1-29/2 1 1 1 * Asia/Shanghai").unwrap());
        assert_debug_snapshot!(parse_crontab("1-30/2 1 1 1 * Asia/Shanghai").unwrap());
        assert_debug_snapshot!(parse_crontab("1,2,10 1 1 1 * Asia/Shanghai").unwrap());
        assert_debug_snapshot!(parse_crontab("1-10,2,10,50 1 1 1 * Asia/Shanghai").unwrap());
        assert_debug_snapshot!(parse_crontab("1-10,2,10,50 1 * 1 TUE Asia/Shanghai").unwrap());
    }

    #[test]
    fn test_parse_crontab_failed() {
        setup_logging();

        assert_snapshot!(parse_crontab("invalid 4 * * * Asia/Shanghai").unwrap_err());
        assert_snapshot!(parse_crontab("* * * * * Unknown/Timezone").unwrap_err());
        assert_snapshot!(parse_crontab("* 5-4 * * * Asia/Shanghai").unwrap_err());
        assert_snapshot!(parse_crontab("10086 * * * * Asia/Shanghai").unwrap_err());
        assert_snapshot!(parse_crontab("* 0-24 * * * Asia/Shanghai").unwrap_err());
        assert_snapshot!(parse_crontab("* * * 25 * Asia/Shanghai").unwrap_err());
        assert_snapshot!(parse_crontab("32-300 * * * * Asia/Shanghai").unwrap_err());
        assert_snapshot!(parse_crontab("129-300 * * * * Asia/Shanghai").unwrap_err());
        assert_snapshot!(parse_crontab("29- * * * * Asia/Shanghai").unwrap_err());
        assert_snapshot!(parse_crontab("29 ** * * * Asia/Shanghai").unwrap_err());
        assert_snapshot!(parse_crontab("29--30 * * * * Asia/Shanghai").unwrap_err());
        assert_snapshot!(parse_crontab("1,2,10,100 1 1 1 * Asia/Shanghai").unwrap_err());
        assert_snapshot!(parse_crontab("104,2,10,100 1 1 1 * Asia/Shanghai").unwrap_err());
        assert_snapshot!(parse_crontab("1,2,10 * * 104,2,10,100 * Asia/Shanghai").unwrap_err());
        assert_snapshot!(parse_crontab("1-10,2,10,50 1 * 1 TTT Asia/Shanghai").unwrap_err());
    }

    #[test]
    fn test_crontab_guru_examples() {
        // crontab.guru examples: https://crontab.guru/examples.html

        assert_debug_snapshot!(parse_crontab("* * * * * UTC").unwrap());
        assert_debug_snapshot!(parse_crontab("*/2 * * * * UTC").unwrap());
        assert_debug_snapshot!(parse_crontab("1-59/2 * * * * UTC").unwrap());
        assert_debug_snapshot!(parse_crontab("*/3 * * * * UTC").unwrap());
        assert_debug_snapshot!(parse_crontab("*/4 * * * * UTC").unwrap());
        assert_debug_snapshot!(parse_crontab("*/5 * * * * UTC").unwrap());
        assert_debug_snapshot!(parse_crontab("*/6 * * * * UTC").unwrap());
        assert_debug_snapshot!(parse_crontab("*/10 * * * * UTC").unwrap());
        assert_debug_snapshot!(parse_crontab("*/15 * * * * UTC").unwrap());
        assert_debug_snapshot!(parse_crontab("*/20 * * * * UTC").unwrap());
        assert_debug_snapshot!(parse_crontab("*/30 * * * * UTC").unwrap());
        assert_debug_snapshot!(parse_crontab("30 * * * * UTC").unwrap());
        assert_debug_snapshot!(parse_crontab("0 * * * * UTC").unwrap());
        assert_debug_snapshot!(parse_crontab("0 */2 * * * UTC").unwrap());
        assert_debug_snapshot!(parse_crontab("0 */3 * * * UTC").unwrap());
        assert_debug_snapshot!(parse_crontab("0 */4 * * * UTC").unwrap());
        assert_debug_snapshot!(parse_crontab("0 */6 * * * UTC").unwrap());
        assert_debug_snapshot!(parse_crontab("0 */8 * * * UTC").unwrap());
        assert_debug_snapshot!(parse_crontab("0 */12 * * * UTC").unwrap());
        assert_debug_snapshot!(parse_crontab("0 9-17 * * * UTC").unwrap());
        assert_debug_snapshot!(parse_crontab("0 0 * * * UTC").unwrap());
        assert_debug_snapshot!(parse_crontab("0 1 * * * UTC").unwrap());
        assert_debug_snapshot!(parse_crontab("0 2 * * * UTC").unwrap());
        assert_debug_snapshot!(parse_crontab("0 8 * * * UTC").unwrap());
        assert_debug_snapshot!(parse_crontab("0 9 * * * UTC").unwrap());
        assert_debug_snapshot!(parse_crontab("0 0 * * 0 UTC").unwrap());
        assert_debug_snapshot!(parse_crontab("0 0 * * 1 UTC").unwrap());
        assert_debug_snapshot!(parse_crontab("0 0 * * 2 UTC").unwrap());
        assert_debug_snapshot!(parse_crontab("0 0 * * 3 UTC").unwrap());
        assert_debug_snapshot!(parse_crontab("0 0 * * 4 UTC").unwrap());
        assert_debug_snapshot!(parse_crontab("0 0 * * 5 UTC").unwrap());
        assert_debug_snapshot!(parse_crontab("0 0 * * 6 UTC").unwrap());
        assert_debug_snapshot!(parse_crontab("0 0 * * 1-5 UTC").unwrap());
        assert_debug_snapshot!(parse_crontab("0 0 * * 6,0 UTC").unwrap());
        assert_debug_snapshot!(parse_crontab("0 0 1 * * UTC").unwrap());
        assert_debug_snapshot!(parse_crontab("0 0 1 * * UTC").unwrap());
        assert_debug_snapshot!(parse_crontab("0 0 1 */2 * UTC").unwrap());
        assert_debug_snapshot!(parse_crontab("0 0 1 */3 * UTC").unwrap());
        assert_debug_snapshot!(parse_crontab("0 0 1 */6 * UTC").unwrap());
        assert_debug_snapshot!(parse_crontab("0 0 1 1 * UTC").unwrap());
    }
}
