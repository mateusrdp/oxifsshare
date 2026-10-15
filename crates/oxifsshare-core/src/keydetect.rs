//! Choosing which column links a row to its file.

use crate::{index::FolderIndex, model::Table};

/// Outcome of key detection. Rates are fractions of all rows (0.0 to 1.0).
#[derive(Clone, Debug)]
pub struct KeyReport {
    /// The column used as the key, if the table has any columns.
    pub chosen: Option<usize>,
    /// The `Document ID` column, if present.
    pub default: Option<usize>,
    /// Match rate of the default column (0.0 when absent).
    pub default_rate: f64,
    /// Match rate of the chosen column.
    pub chosen_rate: f64,
    /// Every column tested, with its rate. Empty when the default was good enough.
    pub candidates: Vec<(usize, f64)>,
}

/// Fraction of rows whose value in `col` resolves to at least one file.
pub fn match_rate(table: &Table, col: usize, index: &FolderIndex) -> f64 {
    if table.rows.is_empty() {
        return 0.0;
    }
    let hits = table
        .rows
        .iter()
        .filter(|r| r.get(col).is_some_and(|v| !index.lookup(v).is_empty()))
        .count();
    hits as f64 / table.rows.len() as f64
}

/// Picks the key column: `Document ID` when at least 50% of rows resolve,
/// otherwise the best-matching column (ties go to the earlier column).
pub fn detect(table: &Table, index: &FolderIndex) -> KeyReport {
    let default = table.find_column("Document ID");
    let default_rate = default.map_or(0.0, |c| match_rate(table, c, index));
    if default.is_some() && default_rate >= 0.5 {
        return KeyReport {
            chosen: default,
            default,
            default_rate,
            chosen_rate: default_rate,
            candidates: Vec::new(),
        };
    }
    let candidates: Vec<(usize, f64)> = (0..table.columns.len())
        .map(|c| (c, match_rate(table, c, index)))
        .collect();
    let mut best: Option<(usize, f64)> = default.map(|c| (c, default_rate));
    for &(c, r) in &candidates {
        if best.is_none_or(|(_, br)| r > br) {
            best = Some((c, r));
        }
    }
    KeyReport {
        chosen: best.map(|b| b.0),
        default,
        default_rate,
        chosen_rate: best.map_or(0.0, |b| b.1),
        candidates,
    }
}
