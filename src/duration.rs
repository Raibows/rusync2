//! duration
//!
//! Parsing of human-friendly durations for the `--sleep-interval` option.

use anyhow::{anyhow, bail, Error};
use std::time::Duration;

/// Parse a duration such as `10`, `10m`, `3h`, `1d` or `1h30m`.
///
/// * A plain number means seconds.
/// * Each group is `<number><unit>`, where unit is one of `s`, `m`, `h`, `d`.
///   The unit may be omitted, in which case the group is a number of seconds.
/// * Groups are summed, so `1h30m` is 90 minutes. Units may repeat and may
///   appear in any order.
/// * Whitespace is allowed between groups, and around the whole value.
/// * The total must be at least one second.
pub fn parse_duration(input: &str) -> Result<Duration, Error> {
    let mut rest = input.trim();
    if rest.is_empty() {
        bail!("expected a number");
    }
    let mut total_secs: u64 = 0;
    loop {
        rest = rest.trim_start();
        if rest.is_empty() {
            break;
        }
        let num_digits = rest.chars().take_while(|c| c.is_ascii_digit()).count();
        if num_digits == 0 {
            let found = rest.chars().next().unwrap_or_default();
            bail!("expected a number, found '{}'", found);
        }
        // ASCII digits are one byte each, so splitting at the number of
        // digits is safe:
        let (digits, after_digits) = rest.split_at(num_digits);
        let value: u64 = digits
            .parse()
            .map_err(|_| anyhow!("the number '{}' is too large", digits))?;
        let (mult, after_unit) = match after_digits.chars().next() {
            Some('s') => (1, &after_digits[1..]),
            Some('m') => (60, &after_digits[1..]),
            Some('h') => (3600, &after_digits[1..]),
            Some('d') => (86400, &after_digits[1..]),
            // Unit omitted: the group is a number of seconds:
            Some(c) if c.is_whitespace() => (1, after_digits),
            None => (1, after_digits),
            Some(c) => bail!("unknown unit '{}' (expected one of: s, m, h, d)", c),
        };
        let group_secs = value
            .checked_mul(mult)
            .ok_or_else(|| anyhow!("the number '{}' is too large", digits))?;
        total_secs = total_secs
            .checked_add(group_secs)
            .ok_or_else(|| anyhow!("the value '{}' is too large", input.trim()))?;
        rest = after_unit;
    }
    if total_secs == 0 {
        bail!("the interval must be at least one second");
    }
    if total_secs > i64::MAX as u64 {
        bail!("the value '{}' is too large", input.trim());
    }
    Ok(Duration::from_secs(total_secs))
}

#[cfg(test)]
mod tests {
    use super::parse_duration;

    fn secs(input: &str) -> u64 {
        parse_duration(input)
            .unwrap_or_else(|e| panic!("'{}' should parse: {:#}", input, e))
            .as_secs()
    }

    #[test]
    fn plain_number_means_seconds() {
        assert_eq!(secs("1"), 1);
        assert_eq!(secs("10"), 10);
        assert_eq!(secs("90"), 90);
    }

    #[test]
    fn s_suffix() {
        assert_eq!(secs("10s"), 10);
    }

    #[test]
    fn m_suffix() {
        assert_eq!(secs("10m"), 600);
    }

    #[test]
    fn h_suffix() {
        assert_eq!(secs("3h"), 3 * 3600);
    }

    #[test]
    fn d_suffix() {
        assert_eq!(secs("1d"), 86400);
    }

    #[test]
    fn compound_groups_are_summed() {
        assert_eq!(secs("1h30m"), 3600 + 1800);
        assert_eq!(secs("2d3h45m10s"), 2 * 86400 + 3 * 3600 + 45 * 60 + 10);
    }

    #[test]
    fn unitless_group_in_compound_is_seconds() {
        assert_eq!(secs("1h30"), 3600 + 30);
    }

    #[test]
    fn whitespace_between_groups_is_allowed() {
        assert_eq!(secs("1h 30m"), 5400);
        assert_eq!(secs("  10  "), 10);
    }

    #[test]
    fn repeated_units_are_summed() {
        assert_eq!(secs("10m10m"), 1200);
    }

    #[test]
    fn zero_interval_is_rejected() {
        assert!(parse_duration("0").is_err());
        assert!(parse_duration("0s").is_err());
        assert!(parse_duration("0m0s").is_err());
    }

    #[test]
    fn zero_groups_are_ok_if_the_total_is_positive() {
        assert_eq!(secs("0h30m"), 1800);
    }

    #[test]
    fn empty_input_is_rejected() {
        assert!(parse_duration("").is_err());
        assert!(parse_duration("   ").is_err());
    }

    #[test]
    fn unknown_unit_is_rejected() {
        for bad in ["1x", "10S", "3W"] {
            let err = parse_duration(bad).unwrap_err();
            let msg = format!("{:#}", err);
            assert!(
                msg.contains("unknown unit"),
                "message for '{}' was: {}",
                bad,
                msg
            );
        }
    }

    #[test]
    fn missing_number_is_rejected() {
        for bad in ["m", "h30", "abc"] {
            assert!(parse_duration(bad).is_err(), "'{}' should not parse", bad);
        }
    }

    #[test]
    fn negative_and_fractional_numbers_are_rejected() {
        assert!(parse_duration("-5").is_err());
        assert!(parse_duration("1.5h").is_err());
    }

    #[test]
    fn space_between_number_and_unit_is_rejected() {
        assert!(parse_duration("10 m").is_err());
    }

    #[test]
    fn overflow_is_rejected() {
        // The number itself does not fit in a u64:
        assert!(parse_duration("999999999999999999999").is_err());
        // The value times its unit does not fit in a u64:
        assert!(parse_duration("9999999999999999d").is_err());
        // Fits in a u64 but is too large to schedule a next sync:
        assert!(parse_duration("10000000000000000000").is_err());
    }
}
