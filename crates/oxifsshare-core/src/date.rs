//! Minimal ISO 8601 / `xs:dateTime` handling, so no date-time crate is needed.

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Inverse of the day count used by [`parse_day`]: days since 1970-01-01 to (year, month, day).
pub fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

fn days_in_month(y: i64, m: i64) -> i64 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ => {
            if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 {
                29
            } else {
                28
            }
        }
    }
}

fn num(b: &[u8]) -> Option<i64> {
    if b.is_empty() || !b.iter().all(u8::is_ascii_digit) {
        return None;
    }
    Some(b.iter().fold(0i64, |a, &c| a * 10 + i64::from(c - b'0')))
}

/// Parses the calendar date at the start of `s` (`YYYY-MM-DD`, optionally
/// followed by `T` or a space and anything else) into days since 1970-01-01.
/// The date is taken exactly as written, with no time-zone conversion.
pub fn parse_day(s: &str) -> Option<i64> {
    let b = s.trim().as_bytes();
    if b.len() < 10 || b[4] != b'-' || b[7] != b'-' {
        return None;
    }
    if b.len() > 10 && b[10] != b'T' && b[10] != b' ' {
        return None;
    }
    let (y, m, d) = (num(&b[0..4])?, num(&b[5..7])?, num(&b[8..10])?);
    if !(1..=12).contains(&m) || d < 1 || d > days_in_month(y, m) {
        return None;
    }
    Some(days_from_civil(y, m, d))
}

/// Parses an ISO 8601 date-time (`2015-06-06T09:39:48.1234567+10:00`, `Z`, no
/// offset, or a bare date) into seconds since the Unix epoch, UTC.
/// A missing offset is treated as UTC. Fractions of a second are ignored.
pub fn parse_datetime(s: &str) -> Option<i64> {
    let day = parse_day(s)?;
    let t = s.trim().as_bytes();
    let mut rest = &t[10..];
    if rest.is_empty() {
        return Some(day * 86_400);
    }
    rest = &rest[1..];
    if rest.len() < 8 || rest[2] != b':' || rest[5] != b':' {
        return None;
    }
    let (h, mi, sec) = (num(&rest[0..2])?, num(&rest[3..5])?, num(&rest[6..8])?);
    if h > 23 || mi > 59 || sec > 60 {
        return None;
    }
    rest = &rest[8..];
    if rest.first() == Some(&b'.') {
        let n = rest[1..].iter().take_while(|c| c.is_ascii_digit()).count();
        if n == 0 {
            return None;
        }
        rest = &rest[1 + n..];
    }
    let offset = match rest {
        [] | [b'Z'] => 0,
        [sign @ (b'+' | b'-'), tail @ ..] => {
            let (oh, om) = match tail {
                [a, b, b':', c, d] => (num(&[*a, *b])?, num(&[*c, *d])?),
                [a, b, c, d] => (num(&[*a, *b])?, num(&[*c, *d])?),
                [a, b] => (num(&[*a, *b])?, 0),
                _ => return None,
            };
            let secs = oh * 3600 + om * 60;
            if *sign == b'-' {
                -secs
            } else {
                secs
            }
        }
        _ => return None,
    };
    Some(day * 86_400 + h * 3600 + mi * 60 + sec - offset)
}

/// Parses a loosely formatted number (`1,234.50`, `$12`, `-3`). Returns `None`
/// for anything else, including `nan` and `inf`.
pub fn parse_number(s: &str) -> Option<f64> {
    let t = s.trim().trim_start_matches('$');
    let cleaned: String = t.chars().filter(|c| *c != ',').collect();
    if cleaned.is_empty()
        || !cleaned.chars().any(|c| c.is_ascii_digit())
        || !cleaned
            .chars()
            .all(|c| c.is_ascii_digit() || matches!(c, '.' | '+' | '-'))
    {
        return None;
    }
    cleaned.parse::<f64>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epoch_and_roundtrip() {
        assert_eq!(parse_day("1970-01-01"), Some(0));
        assert_eq!(
            civil_from_days(parse_day("2015-06-06").unwrap()),
            (2015, 6, 6)
        );
        assert_eq!(
            civil_from_days(parse_day("2024-02-29").unwrap()),
            (2024, 2, 29)
        );
    }

    #[test]
    fn offsets_are_applied() {
        let a = parse_datetime("2015-06-06T10:00:00+10:00").unwrap();
        let b = parse_datetime("2015-06-06T00:00:00Z").unwrap();
        assert_eq!(a, b);
        assert_eq!(
            parse_datetime("2015-06-06T09:39:48.1234567+10:00"),
            parse_datetime("2015-06-05T23:39:48Z")
        );
    }

    #[test]
    fn rejects_garbage_without_panicking() {
        for s in [
            "",
            "2015",
            "2015-13-01",
            "2015-02-30",
            "2015-06-06T25:00:00",
            "2015-06-06T09:39:48.",
            "é2015-06-06",
            "2015-06-06Tx",
            "2015-06-06T09:39:48+1",
        ] {
            assert_eq!(parse_datetime(s), None, "{s:?}");
        }
    }

    #[test]
    fn numbers() {
        assert_eq!(parse_number("$1,234.50"), Some(1234.5));
        assert_eq!(parse_number("nan"), None);
        assert_eq!(parse_number("inf"), None);
        assert_eq!(parse_number("12abc"), None);
    }
}
