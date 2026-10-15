//! Search, filter and sort over an [`Archive`]. Everything returns row indices,
//! so 100,000 rows cost one `Vec<usize>` per query.

use std::cmp::Ordering;

use crate::{archive::Archive, date, index::FileKind, model::ColumnKind};

/// Query state shared by the UI and tests.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Filter {
    /// Free text. Whitespace-separated terms are combined with AND and matched
    /// case-insensitively, as substrings, across all columns.
    pub text: String,
    /// Exact (case-insensitive) `Doc Type`. `None` or empty means any.
    pub doc_type: Option<String>,
    /// Allowed file kinds. Empty means any.
    pub kinds: Vec<FileKind>,
    /// First day to include, as days since 1970-01-01 (see [`date::parse_day`]).
    pub date_from: Option<i64>,
    /// Last day to include (inclusive).
    pub date_to: Option<i64>,
}

/// Sort request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sort {
    /// Column index.
    pub column: usize,
    /// Descending order.
    pub descending: bool,
}

enum Key {
    I(i64),
    F(f64),
    S(String),
}

fn cmp_key(a: &Key, b: &Key) -> Ordering {
    match (a, b) {
        (Key::I(x), Key::I(y)) => x.cmp(y),
        (Key::F(x), Key::F(y)) => x.total_cmp(y),
        (Key::S(x), Key::S(y)) => x.cmp(y),
        _ => Ordering::Equal,
    }
}

impl Archive {
    /// Rows matching `filter`, in file order, then sorted if `sort` is given.
    /// Rows with no sortable value always go last. Ties keep file order.
    pub fn query(&self, filter: &Filter, sort: Option<Sort>) -> Vec<usize> {
        let terms: Vec<String> = filter
            .text
            .split_whitespace()
            .map(str::to_lowercase)
            .collect();
        let doc_type = filter
            .doc_type
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_lowercase);
        let date_active = filter.date_from.is_some() || filter.date_to.is_some();
        let mut out: Vec<usize> = (0..self.table.rows.len())
            .filter(|&i| {
                let hay = &self.haystack[i];
                if !terms.iter().all(|t| hay.contains(t.as_str())) {
                    return false;
                }
                if let (Some(want), Some(col)) = (&doc_type, self.doc_type_col) {
                    let have = self.table.rows[i].get(col).map(|v| v.trim().to_lowercase());
                    if have.as_deref() != Some(want.as_str()) {
                        return false;
                    }
                }
                if !filter.kinds.is_empty() && !filter.kinds.contains(&self.row_kind(i)) {
                    return false;
                }
                if date_active {
                    let day = self
                        .date_col
                        .and_then(|c| self.table.rows[i].get(c))
                        .and_then(date::parse_day);
                    match day {
                        None => return false,
                        Some(d) => {
                            if filter.date_from.is_some_and(|f| d < f)
                                || filter.date_to.is_some_and(|t| d > t)
                            {
                                return false;
                            }
                        }
                    }
                }
                true
            })
            .collect();
        if let Some(s) = sort {
            self.sort_rows(&mut out, s);
        }
        out
    }

    /// Sorts `rows` in place by a column, using the column's kind.
    pub fn sort_rows(&self, rows: &mut [usize], sort: Sort) {
        let Some(col) = self.table.columns.get(sort.column) else {
            return;
        };
        let kind = col.kind;
        let mut keyed: Vec<(Option<Key>, usize)> = rows
            .iter()
            .map(|&r| {
                let v = self.table.rows[r]
                    .get(sort.column)
                    .map(str::trim)
                    .filter(|s| !s.is_empty());
                let key = v.and_then(|v| match kind {
                    ColumnKind::Int => v.parse::<i64>().ok().map(Key::I),
                    ColumnKind::DateTime => date::parse_datetime(v).map(Key::I),
                    ColumnKind::Number => date::parse_number(v).map(Key::F),
                    ColumnKind::Text => Some(Key::S(v.to_lowercase())),
                });
                (key, r)
            })
            .collect();
        keyed.sort_by(|(ka, ra), (kb, rb)| match (ka, kb) {
            (None, None) => ra.cmp(rb),
            (None, Some(_)) => Ordering::Greater,
            (Some(_), None) => Ordering::Less,
            (Some(a), Some(b)) => {
                let o = cmp_key(a, b);
                let o = if sort.descending { o.reverse() } else { o };
                o.then(ra.cmp(rb))
            }
        });
        for (slot, (_, r)) in rows.iter_mut().zip(keyed) {
            *slot = r;
        }
    }
}
