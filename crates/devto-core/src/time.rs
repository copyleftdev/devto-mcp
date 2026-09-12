//! RFC 3339 timestamps in UTC, without a date library.
//!
//! Forem takes `published_at` as a string and casts it in ActiveRecord, so scheduling
//! needs a real timestamp rather than an integer. The conversion is Howard Hinnant's
//! `civil_from_days` / `days_from_civil`, which is exact for the proleptic Gregorian
//! calendar and needs no tables.
//!
//! UTC only, deliberately. An offset that the caller and the server disagree about is a
//! post that publishes at the wrong hour, and dev.to stores UTC regardless.

use crate::draft::UnixSeconds;

const SECONDS_PER_DAY: i64 = 86_400;

/// Render a Unix timestamp as `YYYY-MM-DDTHH:MM:SSZ`.
///
/// Returns `None` outside years 1–9999, where the four-digit form stops being meaningful.
pub fn format_rfc3339_utc(unix: UnixSeconds) -> Option<String> {
    let days = unix.div_euclid(SECONDS_PER_DAY);
    let seconds = unix.rem_euclid(SECONDS_PER_DAY);
    let (year, month, day) = civil_from_days(days);
    if !(1..=9999).contains(&year) {
        return None;
    }
    let (hour, minute, second) = (seconds / 3600, (seconds % 3600) / 60, seconds % 60);
    Some(format!(
        "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z"
    ))
}

/// Read an RFC 3339 timestamp.
///
/// Accepts a bare `YYYY-MM-DD` (taken as midnight UTC), an optional fractional part which
/// is parsed and discarded, and either `Z` or a `±HH:MM` offset.
pub fn parse_rfc3339_utc(text: &str) -> Option<UnixSeconds> {
    let text = text.trim();
    let (date, rest) = match text.split_once(['T', 't', ' ']) {
        Some((date, rest)) => (date, Some(rest)),
        None => (text, None),
    };

    let mut date_parts = date.split('-');
    let year: i64 = date_parts.next()?.parse().ok()?;
    let month: u32 = two_digits(date_parts.next()?)?;
    let day: u32 = two_digits(date_parts.next()?)?;
    if date_parts.next().is_some() || !(1..=12).contains(&month) {
        return None;
    }
    if day < 1 || day > days_in_month(year, month) {
        return None;
    }

    let days = days_from_civil(year, month, day);
    let Some(rest) = rest else {
        return Some(days * SECONDS_PER_DAY);
    };

    // Split the offset off the end before reading the clock time.
    let (clock, offset_seconds) = match rest.chars().last()? {
        'Z' | 'z' => (&rest[..rest.len() - 1], 0),
        _ => {
            let sign_at = rest.rfind(['+', '-'])?;
            let (clock, offset) = rest.split_at(sign_at);
            (clock, parse_offset(offset)?)
        }
    };

    let clock = clock.split_once('.').map_or(clock, |(whole, frac)| {
        if frac.chars().all(|c| c.is_ascii_digit()) && !frac.is_empty() {
            whole
        } else {
            clock
        }
    });

    let mut clock_parts = clock.split(':');
    let hour: i64 = two_digits(clock_parts.next()?)? as i64;
    let minute: i64 = two_digits(clock_parts.next()?)? as i64;
    let second: i64 = match clock_parts.next() {
        Some(value) => two_digits(value)? as i64,
        None => 0,
    };
    if clock_parts.next().is_some() || hour > 23 || minute > 59 || second > 60 {
        return None;
    }

    Some(days * SECONDS_PER_DAY + hour * 3600 + minute * 60 + second - offset_seconds)
}

fn two_digits(text: &str) -> Option<u32> {
    if text.len() != 2 || !text.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

fn parse_offset(text: &str) -> Option<i64> {
    let sign = match text.chars().next()? {
        '+' => 1,
        '-' => -1,
        _ => return None,
    };
    let body = &text[1..];
    let (hours, minutes) = match body.split_once(':') {
        Some((h, m)) => (two_digits(h)?, two_digits(m)?),
        None if body.len() == 4 => (two_digits(&body[..2])?, two_digits(&body[2..])?),
        _ => return None,
    };
    if hours > 23 || minutes > 59 {
        return None;
    }
    Some(sign * (hours as i64 * 3600 + minutes as i64 * 60))
}

fn is_leap_year(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_in_month(year: i64, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

/// Days since 1970-01-01 to a civil date. Hinnant's algorithm, shifted to an era starting
/// on 0000-03-01 so that the leap day lands at the end of a year.
fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let year = year - if month <= 2 { 1 } else { 0 };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let month_shifted = if month > 2 { month - 3 } else { month + 9 } as i64;
    let day_of_year = (153 * month_shifted + 2) / 5 + day as i64 - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let shifted = days + 719_468;
    let era = if shifted >= 0 {
        shifted
    } else {
        shifted - 146_096
    } / 146_097;
    let day_of_era = shifted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_shifted = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * month_shifted + 2) / 5 + 1) as u32;
    let month = if month_shifted < 10 {
        month_shifted + 3
    } else {
        month_shifted - 9
    } as u32;
    (year + if month <= 2 { 1 } else { 0 }, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_epoch_renders_as_the_epoch() {
        assert_eq!(
            format_rfc3339_utc(0).as_deref(),
            Some("1970-01-01T00:00:00Z")
        );
        assert_eq!(parse_rfc3339_utc("1970-01-01T00:00:00Z"), Some(0));
    }

    /// Pinned against a known value so a shifted era constant cannot hide.
    #[test]
    fn a_known_timestamp_renders_to_its_known_date() {
        assert_eq!(
            format_rfc3339_utc(1_757_000_000).as_deref(),
            Some("2025-09-04T15:33:20Z")
        );
        assert_eq!(
            parse_rfc3339_utc("2025-09-04T15:33:20Z"),
            Some(1_757_000_000)
        );
    }

    #[test]
    fn dates_before_the_epoch_work_too() {
        assert_eq!(
            format_rfc3339_utc(-1).as_deref(),
            Some("1969-12-31T23:59:59Z")
        );
        assert_eq!(parse_rfc3339_utc("1969-12-31T23:59:59Z"), Some(-1));
    }

    /// The century rules are where a hand-rolled calendar goes wrong: 2000 is a leap year,
    /// 1900 is not.
    #[test]
    fn the_leap_year_rules_are_the_gregorian_ones() {
        assert!(is_leap_year(2024));
        assert!(is_leap_year(2000));
        assert!(!is_leap_year(1900));
        assert!(!is_leap_year(2023));

        assert_eq!(days_in_month(2024, 2), 29);
        assert_eq!(days_in_month(2023, 2), 28);
        assert_eq!(days_in_month(1900, 2), 28);
        assert_eq!(days_in_month(2000, 2), 29);

        assert_eq!(
            format_rfc3339_utc(parse_rfc3339_utc("2024-02-29T12:00:00Z").unwrap()).as_deref(),
            Some("2024-02-29T12:00:00Z")
        );
        assert_eq!(parse_rfc3339_utc("2023-02-29T12:00:00Z"), None);
    }

    #[test]
    fn a_bare_date_is_midnight_utc() {
        assert_eq!(
            parse_rfc3339_utc("2026-09-12"),
            parse_rfc3339_utc("2026-09-12T00:00:00Z")
        );
    }

    #[test]
    fn an_offset_is_applied_rather_than_ignored() {
        let utc = parse_rfc3339_utc("2026-09-12T12:00:00Z").unwrap();
        assert_eq!(parse_rfc3339_utc("2026-09-12T14:00:00+02:00"), Some(utc));
        assert_eq!(parse_rfc3339_utc("2026-09-12T07:00:00-05:00"), Some(utc));
        assert_eq!(parse_rfc3339_utc("2026-09-12T14:00:00+0200"), Some(utc));
    }

    #[test]
    fn a_fractional_second_is_read_and_discarded() {
        let whole = parse_rfc3339_utc("2026-09-12T12:00:00Z");
        assert_eq!(parse_rfc3339_utc("2026-09-12T12:00:00.123Z"), whole);
        assert_eq!(parse_rfc3339_utc("2026-09-12T12:00:00.000001Z"), whole);
    }

    #[test]
    fn lowercase_separators_and_a_space_are_accepted() {
        let expected = parse_rfc3339_utc("2026-09-12T12:00:00Z");
        assert_eq!(parse_rfc3339_utc("2026-09-12t12:00:00z"), expected);
        assert_eq!(parse_rfc3339_utc("2026-09-12 12:00:00Z"), expected);
        assert_eq!(parse_rfc3339_utc("  2026-09-12T12:00:00Z  "), expected);
    }

    #[test]
    fn nonsense_is_refused_rather_than_guessed_at() {
        for bad in [
            "",
            "not a date",
            "2026-13-01T00:00:00Z",
            "2026-00-01T00:00:00Z",
            "2026-09-32T00:00:00Z",
            "2026-09-00T00:00:00Z",
            "2026-09-12T24:00:00Z",
            "2026-09-12T12:60:00Z",
            "2026-9-12T00:00:00Z",
            "2026-09-12T12:00:00",
            "2026-09-12T12:00:00+99:00",
            "2026-09-12T12:00:00:00Z",
            "2026-09-12-01T00:00:00Z",
        ] {
            assert_eq!(parse_rfc3339_utc(bad), None, "{bad:?} should not parse");
        }
    }

    #[test]
    fn years_outside_the_four_digit_range_are_refused() {
        assert_eq!(format_rfc3339_utc(-62_167_219_201), None);
        assert_eq!(format_rfc3339_utc(253_402_300_800), None);
        assert!(format_rfc3339_utc(253_402_300_799).is_some());
    }

    /// A table spanning four centuries, checked in both directions.
    ///
    /// A random round-trip property proves the two conversions agree with *each other*; it
    /// cannot prove they agree with the calendar. These pairs are the oracle — and the
    /// century terms in the era arithmetic only show up when the year is far from a
    /// multiple of 400, which a sampled range can miss entirely.
    #[test]
    fn known_dates_convert_to_their_known_timestamps() {
        let known: [(&str, i64); 12] = [
            ("1970-01-01T00:00:00Z", 0),
            ("2000-03-01T00:00:00Z", 951_868_800),
            ("1900-01-01T00:00:00Z", -2_208_988_800),
            ("2100-01-01T00:00:00Z", 4_102_444_800),
            ("2038-01-19T03:14:07Z", 2_147_483_647),
            ("1999-12-31T23:59:59Z", 946_684_799),
            ("2024-02-29T12:34:56Z", 1_709_210_096),
            ("1600-02-29T00:00:00Z", -11_670_998_400),
            ("2400-02-29T00:00:00Z", 13_574_563_200),
            ("9999-12-31T23:59:59Z", 253_402_300_799),
            ("0001-01-01T00:00:00Z", -62_135_596_800),
            ("1969-07-20T20:17:00Z", -14_182_980),
        ];

        for (text, unix) in known {
            assert_eq!(parse_rfc3339_utc(text), Some(unix), "parsing {text}");
            assert_eq!(
                format_rfc3339_utc(unix).as_deref(),
                Some(text),
                "rendering {unix}"
            );
        }
    }

    /// January and February are shifted into the previous year by the era arithmetic, so a
    /// date in year 0000 drives the year negative and takes a branch no later date reaches.
    /// The expected values are cross-checked against Python's proleptic Gregorian ordinals,
    /// not against this algorithm.
    #[test]
    fn dates_in_year_zero_take_the_negative_era_branch_correctly() {
        // 719528 days before the epoch: 719162 from 0001-01-01, plus 366 for year 0, which
        // is a leap year because 0 is divisible by 400.
        assert_eq!(
            parse_rfc3339_utc("0000-01-01T00:00:00Z"),
            Some(-62_167_219_200)
        );
        assert_eq!(
            parse_rfc3339_utc("0000-02-29T00:00:00Z"),
            Some(-62_167_219_200 + 59 * 86_400),
            "year zero is a leap year"
        );
        // March moves out of the shifted branch, so this one is reached the other way.
        assert_eq!(
            parse_rfc3339_utc("0000-03-01T00:00:00Z"),
            Some(-62_162_035_200)
        );
        assert_eq!(
            parse_rfc3339_utc("0001-01-01T00:00:00Z"),
            Some(-62_135_596_800)
        );

        // The renderer stops at year 1, so these parse but do not render.
        assert!(format_rfc3339_utc(-62_167_219_200).is_none());
        assert!(format_rfc3339_utc(-62_135_596_800).is_some());
    }

    /// An offset with both parts non-zero. Every offset in the earlier tests had zero
    /// minutes, which hides the whole minutes term of the arithmetic.
    #[test]
    fn an_offset_with_minutes_is_applied_in_full() {
        let utc = parse_rfc3339_utc("2026-09-12T06:30:00Z").unwrap();
        assert_eq!(parse_rfc3339_utc("2026-09-12T12:00:00+05:30"), Some(utc));
        assert_eq!(parse_rfc3339_utc("2026-09-12T12:00:00+0530"), Some(utc));
        assert_eq!(
            parse_rfc3339_utc("2026-09-12T01:00:00-05:30"),
            parse_rfc3339_utc("2026-09-12T06:30:00Z")
        );
        assert_eq!(
            parse_rfc3339_utc("2026-09-12T12:00:00+00:45"),
            parse_rfc3339_utc("2026-09-12T11:15:00Z")
        );
    }

    #[test]
    fn the_offset_bounds_are_the_last_valid_values() {
        assert!(parse_rfc3339_utc("2026-09-12T12:00:00+23:59").is_some());
        assert!(parse_rfc3339_utc("2026-09-12T12:00:00+24:00").is_none());
        assert!(parse_rfc3339_utc("2026-09-12T12:00:00+23:60").is_none());
        assert!(parse_rfc3339_utc("2026-09-12T12:00:00-23:59").is_some());
    }

    /// An offset must be exactly `±HH:MM` or `±HHMM`. Anything else is refused rather than
    /// sliced into, which would read past the end of a short one.
    #[test]
    fn a_malformed_offset_is_refused_without_reading_past_it() {
        for bad in [
            "2026-09-12T12:00:00+5",
            "2026-09-12T12:00:00+",
            "2026-09-12T12:00:00+5:00",
            "2026-09-12T12:00:00+050",
            "2026-09-12T12:00:00+050000",
        ] {
            assert_eq!(parse_rfc3339_utc(bad), None, "{bad:?}");
        }
    }

    /// A leap second is a real value in RFC 3339, and 61 is not.
    #[test]
    fn the_second_may_reach_sixty_but_no_further() {
        assert!(parse_rfc3339_utc("2026-09-12T23:59:60Z").is_some());
        assert!(parse_rfc3339_utc("2026-09-12T23:59:61Z").is_none());
        assert!(parse_rfc3339_utc("2026-09-12T23:59:59Z").is_some());
    }

    /// A decimal point with no digits after it is not a fractional second.
    #[test]
    fn an_empty_fraction_is_not_treated_as_one() {
        assert_eq!(parse_rfc3339_utc("2026-09-12T12:00:00.Z"), None);
        assert_eq!(parse_rfc3339_utc("2026-09-12T12:00:00.abcZ"), None);
        assert!(parse_rfc3339_utc("2026-09-12T12:00:00.5Z").is_some());
    }

    /// The round trip is the contract: what we render, we can read back.
    #[hegel::test]
    fn rendering_and_reading_a_timestamp_returns_it_unchanged(tc: hegel::TestCase) {
        // 0001-01-01 to 9999-12-31: exactly the range `format_rfc3339_utc` will render.
        let unix = tc.draw(
            hegel::generators::integers::<i64>()
                .min_value(-62_135_596_800)
                .max_value(253_402_300_799),
        );
        let rendered = format_rfc3339_utc(unix).expect("inside the representable range");
        assert_eq!(
            parse_rfc3339_utc(&rendered),
            Some(unix),
            "{unix} rendered as {rendered}"
        );
    }

    /// The calendar conversion has to be a bijection, or some day of some year is
    /// unreachable and every date after it is off by one.
    #[hegel::test]
    fn days_and_civil_dates_convert_both_ways(tc: hegel::TestCase) {
        let days = tc.draw(
            hegel::generators::integers::<i64>()
                .min_value(-719_162)
                .max_value(2_932_896),
        );
        let (year, month, day) = civil_from_days(days);
        assert!((1..=12).contains(&month), "{days} gave month {month}");
        assert!(
            day >= 1 && day <= days_in_month(year, month),
            "{days} gave {year}-{month}-{day}"
        );
        assert_eq!(days_from_civil(year, month, day), days);
    }

    #[hegel::test]
    fn parsing_arbitrary_text_never_panics(tc: hegel::TestCase) {
        let text = tc.draw(hegel::generators::text().max_size(40));
        let _ = parse_rfc3339_utc(&text);
    }
}
