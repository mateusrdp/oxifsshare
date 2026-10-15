//! Small text helpers for the table and details pane. No GUI types, so they are unit-tested.

/// Collapses all whitespace (including newlines) to single spaces and shortens
/// to at most `max` characters, adding `…` when cut. Cheap on huge values.
pub fn display_text(s: &str, max: usize) -> String {
    let mut out = String::new();
    let mut count = 0usize;
    let mut pending_space = false;
    for c in s.chars().take(max.saturating_mul(4).max(16)) {
        if c.is_whitespace() {
            pending_space = !out.is_empty();
            continue;
        }
        if pending_space {
            out.push(' ');
            count += 1;
            pending_space = false;
        }
        if count >= max {
            out.push('…');
            return out;
        }
        out.push(c);
        count += 1;
    }
    if s.chars().nth(max.saturating_mul(4).max(16)).is_some() {
        out.push('…');
    }
    out
}

/// Compact form of an ISO timestamp for table cells (`2016-07-07T06:30:00.123+10:00`
/// becomes `2016-07-07 06:30`). Anything that does not look like one is returned unchanged.
pub fn short_datetime(v: &str) -> String {
    let b = v.trim().as_bytes();
    let t = v.trim();
    if b.len() >= 16
        && b[10] == b'T'
        && b[13] == b':'
        && t.is_char_boundary(10)
        && t.is_char_boundary(16)
    {
        format!("{} {}", &t[..10], &t[11..16])
    } else {
        v.to_string()
    }
}

/// Formats a byte count for humans (`1.5 MB`).
pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = bytes as f64;
    let mut u = 0;
    while v >= 1024.0 && u < UNITS.len() - 1 {
        v /= 1024.0;
        u += 1;
    }
    if u == 0 {
        format!("{bytes} B")
    } else {
        format!("{v:.1} {}", UNITS[u])
    }
}

/// Parses a date field. Empty means "no limit"; anything else must be `YYYY-MM-DD`.
pub fn parse_date_field(s: &str) -> Result<Option<i64>, ()> {
    let t = s.trim();
    if t.is_empty() {
        return Ok(None);
    }
    if t.len() != 10 {
        return Err(());
    }
    oxifsshare_core::date::parse_day(t).map(Some).ok_or(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collapses_and_truncates() {
        assert_eq!(display_text("a\n\n  b\tc ", 50), "a b c");
        assert_eq!(display_text("abcdefghij", 5), "abcde…");
        assert_eq!(display_text("abcde", 5), "abcde");
        assert_eq!(display_text("", 5), "");
        assert_eq!(display_text("   ", 5), "");
        let huge = "word ".repeat(100_000);
        let d = display_text(&huge, 40);
        assert!(d.chars().count() <= 41);
        assert!(d.ends_with('…'));
    }

    #[test]
    fn multibyte_is_safe() {
        assert_eq!(display_text("ééééé", 3), "ééé…");
    }

    #[test]
    fn short_dates() {
        assert_eq!(
            short_datetime("2016-07-07T06:30:00.8123456+10:00"),
            "2016-07-07 06:30"
        );
        assert_eq!(short_datetime("2016-07-07"), "2016-07-07");
        assert_eq!(
            short_datetime("héllo wörld, this is long"),
            "héllo wörld, this is long"
        );
        assert_eq!(short_datetime(""), "");
    }

    #[test]
    fn sizes() {
        assert_eq!(human_size(0), "0 B");
        assert_eq!(human_size(1023), "1023 B");
        assert_eq!(human_size(1536), "1.5 KB");
        assert_eq!(human_size(5 * 1024 * 1024), "5.0 MB");
    }

    #[test]
    fn date_fields() {
        assert_eq!(parse_date_field(""), Ok(None));
        assert_eq!(parse_date_field(" "), Ok(None));
        assert!(parse_date_field("2020-01-31").unwrap().is_some());
        assert!(parse_date_field("2020-02-30").is_err());
        assert!(parse_date_field("2020-1-3").is_err());
        assert!(parse_date_field("2020-01-31T00:00").is_err());
        assert!(parse_date_field("garbage").is_err());
    }
}
