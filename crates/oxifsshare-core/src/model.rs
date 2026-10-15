//! In-memory table of index rows. Columns are discovered from the data (and
//! the optional inline schema), so unexpected extra columns never break loading.

use crate::date;

/// How a column's values should be sorted and filtered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColumnKind {
    /// Plain text, sorted case-insensitively.
    Text,
    /// Integer (`xs:int`), sorted numerically.
    Int,
    /// Date-time, sorted chronologically.
    DateTime,
    /// Text where every value is numeric (e.g. amounts), sorted numerically.
    Number,
}

/// One column of the index.
#[derive(Clone, Debug)]
pub struct Column {
    /// Decoded element name, e.g. `Doc Type`.
    pub name: String,
    /// Display label.
    pub label: String,
    /// Sort/filter kind (final after loading).
    pub kind: ColumnKind,
    pub(crate) declared: Option<ColumnKind>,
    pub(crate) declared_string: bool,
}

/// One entry. Missing optional elements are `None`.
#[derive(Clone, Debug, Default)]
pub struct Row {
    pub(crate) values: Vec<Option<String>>,
}

impl Row {
    /// Raw value of column `col`, untrimmed. `None` if the element was omitted.
    pub fn get(&self, col: usize) -> Option<&str> {
        self.values.get(col).and_then(|v| v.as_deref())
    }
}

/// Facts about parsing, for `--check`. Contains counts only.
#[derive(Clone, Debug, Default)]
pub struct ParseStats {
    /// The text started with a UTF-8 byte order mark.
    pub bom: bool,
    /// The file was UTF-16.
    pub utf16: bool,
    /// An inline schema was present.
    pub schema_found: bool,
    /// Number of `\r\n` line endings.
    pub crlf: usize,
    /// Number of bare `\n` line endings.
    pub lf: usize,
    /// Entity references that were not understood.
    pub unknown_entities: usize,
    /// Elements nested inside a field (their text was kept, tags dropped).
    pub nested_elements: usize,
    /// Fields that appeared twice in one row (last wins).
    pub duplicate_fields: usize,
    /// Root children that were neither schema nor rows.
    pub other_root_children: usize,
}

/// The parsed index.
#[derive(Clone, Debug, Default)]
pub struct Table {
    /// Name of the row element (normally `documents`).
    pub row_element: String,
    /// Columns in display order: schema order first, then extras in order of appearance.
    pub columns: Vec<Column>,
    /// All rows in file order.
    pub rows: Vec<Row>,
    /// Parsing facts.
    pub stats: ParseStats,
}

pub(crate) fn normalize_name(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

pub(crate) fn label_for(name: &str) -> String {
    match normalize_name(name).as_str() {
        "workitemid" => "Work Item ID".into(),
        "documentid" => "Document ID".into(),
        _ => name.to_string(),
    }
}

impl Table {
    /// Finds a column ignoring case, spaces and punctuation (`"documentid"` finds `Document ID`).
    pub fn find_column(&self, name: &str) -> Option<usize> {
        let want = normalize_name(name);
        self.columns
            .iter()
            .position(|c| normalize_name(&c.name) == want)
    }

    /// Decides final column kinds from the schema and the data.
    pub(crate) fn finish(&mut self) {
        for ci in 0..self.columns.len() {
            if let Some(k) = self.columns[ci].declared {
                self.columns[ci].kind = k;
                continue;
            }
            let mut any = false;
            let (mut all_date, mut all_num) = (true, true);
            for r in &self.rows {
                let Some(v) = r.get(ci) else { continue };
                if v.trim().is_empty() {
                    continue;
                }
                any = true;
                all_date &= v.trim().len() >= 10 && date::parse_datetime(v).is_some();
                all_num &= date::parse_number(v).is_some();
                if !all_date && !all_num {
                    break;
                }
            }
            let declared_string = self.columns[ci].declared_string;
            self.columns[ci].kind = if !any {
                ColumnKind::Text
            } else if all_date && !declared_string {
                ColumnKind::DateTime
            } else if all_num {
                ColumnKind::Number
            } else {
                ColumnKind::Text
            };
        }
    }
}

/// Decodes `_xHHHH_` sequences used by .NET to escape characters in XML names.
pub fn decode_name(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'_' && i + 6 < b.len() && b[i + 1] == b'x' && b[i + 6] == b'_' {
            let hex = s.get(i + 2..i + 6);
            if let Some(code) = hex.and_then(|h| u32::from_str_radix(h, 16).ok()) {
                if let Some(c) = char::from_u32(code) {
                    out.push(c);
                    i += 7;
                    continue;
                }
            }
        }
        // Copy one full char.
        let ch_len = s[i..].chars().next().map_or(1, char::len_utf8);
        out.push_str(&s[i..i + ch_len]);
        i += ch_len;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_x_escapes() {
        assert_eq!(decode_name("Doc_x0020_Type"), "Doc Type");
        assert_eq!(
            decode_name("Show_x0020_on_x0020_Portals"),
            "Show on Portals"
        );
        assert_eq!(decode_name("A_x005F_B"), "A_B");
        assert_eq!(decode_name("plain_name"), "plain_name");
        assert_eq!(decode_name("bad_xZZZZ_"), "bad_xZZZZ_");
        assert_eq!(decode_name("trunc_x00"), "trunc_x00");
        assert_eq!(decode_name("é_x0020_é"), "é é");
    }
}
