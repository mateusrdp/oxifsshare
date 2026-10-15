//! Generic, streaming parser for the `.NET DataSet.WriteXml` index.
//!
//! Layout handled: a root element, an optional inline `xs:schema` block, then
//! repeated row elements whose children are the columns. Optional columns are
//! simply absent from a row. Values are kept exactly as written (not trimmed)
//! except that line endings are normalised to `\n`.

use std::{borrow::Cow, collections::HashMap};

use quick_xml::{
    events::{BytesStart, Event},
    Reader,
};

use crate::{
    error::Error,
    model::{decode_name, label_for, Column, ColumnKind, ParseStats, Row, Table},
};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Ctx {
    Root,
    Schema,
    Row,
    Field,
    Nested,
    Skip,
}

fn line_col(bytes: &[u8], pos: usize) -> (usize, usize) {
    let pos = pos.min(bytes.len());
    let before = &bytes[..pos];
    let line = before.iter().filter(|&&b| b == b'\n').count() + 1;
    let col = before
        .iter()
        .rposition(|&b| b == b'\n')
        .map_or(pos, |nl| pos - nl - 1)
        + 1;
    (line, col)
}

fn xml_error(text: &str, pos: usize, message: impl Into<String>) -> Error {
    let (line, column) = line_col(text.as_bytes(), pos);
    Error::Xml {
        line,
        column,
        message: message.into(),
    }
}

fn decode_text(bytes: &[u8]) -> Result<(Cow<'_, str>, bool, bool), Error> {
    let utf8 = |b: &[u8]| -> Result<(), Error> {
        std::str::from_utf8(b).map(|_| ()).map_err(|e| {
            let (line, column) = line_col(b, e.valid_up_to());
            Error::Encoding {
                line,
                column,
                message: "invalid UTF-8 sequence".into(),
            }
        })
    };
    if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        utf8(rest)?;
        // Validated just above.
        return Ok((
            Cow::Borrowed(std::str::from_utf8(rest).unwrap_or_default()),
            true,
            false,
        ));
    }
    let le = bytes.starts_with(&[0xFF, 0xFE]);
    let be = bytes.starts_with(&[0xFE, 0xFF]);
    if le || be {
        let body = &bytes[2..];
        if body.len() % 2 == 1 {
            return Err(Error::Encoding {
                line: 1,
                column: 1,
                message: "odd number of bytes in UTF-16 data".into(),
            });
        }
        let units: Vec<u16> = body
            .chunks_exact(2)
            .map(|c| {
                if le {
                    u16::from_le_bytes([c[0], c[1]])
                } else {
                    u16::from_be_bytes([c[0], c[1]])
                }
            })
            .collect();
        let s = String::from_utf16(&units).map_err(|_| Error::Encoding {
            line: 1,
            column: 1,
            message: "invalid UTF-16 data".into(),
        })?;
        return Ok((Cow::Owned(s), false, true));
    }
    utf8(bytes)?;
    Ok((
        Cow::Borrowed(std::str::from_utf8(bytes).unwrap_or_default()),
        false,
        false,
    ))
}

fn local_of(name: &str) -> String {
    name.rsplit(':').next().unwrap_or(name).to_string()
}

fn attr(e: &BytesStart<'_>, key_local: &str) -> Option<String> {
    for a in e.attributes().flatten() {
        if local_of(a.key.as_ref()) == key_local {
            // Names and type names in the schema never contain entities.
            return Some(a.value.into_owned());
        }
    }
    None
}

fn normalize_newlines(s: &str) -> Cow<'_, str> {
    if s.contains('\r') {
        Cow::Owned(s.replace("\r\n", "\n").replace('\r', "\n"))
    } else {
        Cow::Borrowed(s)
    }
}

fn kind_from_xsd(t: &str) -> (Option<ColumnKind>, bool) {
    match t.rsplit(':').next().unwrap_or(t) {
        "dateTime" | "date" => (Some(ColumnKind::DateTime), false),
        "int" | "integer" | "long" | "short" | "byte" | "unsignedInt" | "unsignedLong" => {
            (Some(ColumnKind::Int), false)
        }
        "decimal" | "double" | "float" => (Some(ColumnKind::Number), false),
        "string" => (None, true),
        _ => (None, false),
    }
}

#[derive(Default)]
struct Builder {
    root: Option<String>,
    main_table: Option<String>,
    row_element: Option<String>,
    schema: Vec<(String, Option<String>)>,
    columns: Vec<Column>,
    col_index: HashMap<String, usize>,
    rows: Vec<Row>,
    cur_field: Option<String>,
    cur_row: Vec<Option<String>>,
    buf: String,
    stats: ParseStats,
}

impl Builder {
    fn ensure_column(&mut self, name: &str) -> usize {
        if let Some(&i) = self.col_index.get(name) {
            return i;
        }
        let i = self.columns.len();
        self.columns.push(Column {
            name: name.to_string(),
            label: label_for(name),
            kind: ColumnKind::Text,
            declared: None,
            declared_string: false,
        });
        self.col_index.insert(name.to_string(), i);
        i
    }

    fn start(&mut self, parent: Option<Ctx>, e: &BytesStart<'_>) -> Ctx {
        let local = local_of(e.name().as_ref());
        match parent {
            None => {
                self.root = Some(local);
                Ctx::Root
            }
            Some(Ctx::Root) => {
                if local == "schema" {
                    self.stats.schema_found = true;
                    return Ctx::Schema;
                }
                match &self.row_element {
                    None => self.row_element = Some(local),
                    Some(r) if *r != local => {
                        self.stats.other_root_children += 1;
                        return Ctx::Skip;
                    }
                    Some(_) => {}
                }
                self.cur_row.clear();
                Ctx::Row
            }
            Some(Ctx::Schema) => {
                if local == "element" {
                    if let Some(m) = attr(e, "MainDataTable") {
                        self.main_table = Some(m);
                    }
                    if let Some(name) = attr(e, "name") {
                        self.schema.push((name, attr(e, "type")));
                    }
                }
                Ctx::Schema
            }
            Some(Ctx::Row) => {
                self.cur_field = Some(decode_name(&local));
                self.buf.clear();
                Ctx::Field
            }
            Some(Ctx::Field | Ctx::Nested) => {
                self.stats.nested_elements += 1;
                Ctx::Nested
            }
            Some(Ctx::Skip) => Ctx::Skip,
        }
    }

    fn end(&mut self, ctx: Ctx, parent: Option<Ctx>) {
        match ctx {
            Ctx::Schema if parent != Some(Ctx::Schema) => self.apply_schema(),
            Ctx::Field => {
                if let Some(name) = self.cur_field.take() {
                    let idx = self.ensure_column(&name);
                    if self.cur_row.len() <= idx {
                        self.cur_row.resize(idx + 1, None);
                    }
                    if self.cur_row[idx].is_some() {
                        self.stats.duplicate_fields += 1;
                    }
                    self.cur_row[idx] = Some(std::mem::take(&mut self.buf));
                }
            }
            Ctx::Row => self.rows.push(Row {
                values: std::mem::take(&mut self.cur_row),
            }),
            _ => {}
        }
    }

    fn apply_schema(&mut self) {
        let schema = std::mem::take(&mut self.schema);
        for (raw, ty) in schema {
            if Some(&raw) == self.root.as_ref() || Some(&raw) == self.main_table.as_ref() {
                continue;
            }
            let name = decode_name(&raw);
            let i = self.ensure_column(&name);
            if let Some(t) = ty {
                let (k, s) = kind_from_xsd(&t);
                self.columns[i].declared = k;
                self.columns[i].declared_string = s;
            }
        }
    }

    fn text(&mut self, s: &str) {
        self.buf.push_str(&normalize_newlines(s));
    }

    fn entity(&mut self, name: &str, char_ref: Option<Result<Option<char>, ()>>) {
        match (char_ref, name) {
            (Some(Ok(Some(c))), _) => self.buf.push(c),
            (Some(Err(())), _) => {
                self.stats.unknown_entities += 1;
                self.buf.push('\u{FFFD}');
            }
            (_, "amp") => self.buf.push('&'),
            (_, "lt") => self.buf.push('<'),
            (_, "gt") => self.buf.push('>'),
            (_, "quot") => self.buf.push('"'),
            (_, "apos") => self.buf.push('\''),
            _ => {
                // Undefined entity: keep it visible instead of failing the whole load.
                self.stats.unknown_entities += 1;
                self.buf.push('&');
                self.buf.push_str(name);
                self.buf.push(';');
            }
        }
    }
}

/// Parses the index bytes into a [`Table`].
///
/// Accepts UTF-8 (with or without BOM) and UTF-16. Never panics on bad input;
/// malformed or truncated XML yields [`Error::Xml`] with a line and column.
pub fn parse_index(bytes: &[u8]) -> Result<Table, Error> {
    let (text, bom, utf16) = decode_text(bytes)?;
    let text: &str = &text;

    let mut b = Builder::default();
    b.stats.bom = bom;
    b.stats.utf16 = utf16;
    let tb = text.as_bytes();
    let crlf = tb.windows(2).filter(|w| w == b"\r\n").count();
    b.stats.crlf = crlf;
    b.stats.lf = tb.iter().filter(|&&c| c == b'\n').count() - crlf;

    let mut reader = Reader::from_str(text);
    let mut stack: Vec<Ctx> = Vec::new();
    loop {
        let ev = match reader.read_event() {
            Ok(ev) => ev,
            Err(e) => {
                return Err(xml_error(
                    text,
                    reader.error_position() as usize,
                    e.to_string(),
                ))
            }
        };
        let in_field = matches!(stack.last(), Some(Ctx::Field | Ctx::Nested));
        match ev {
            Event::Start(e) => {
                let ctx = b.start(stack.last().copied(), &e);
                stack.push(ctx);
            }
            Event::Empty(e) => {
                let parent = stack.last().copied();
                let ctx = b.start(parent, &e);
                b.end(ctx, parent);
            }
            Event::End(_) => {
                if let Some(ctx) = stack.pop() {
                    b.end(ctx, stack.last().copied());
                }
            }
            Event::Text(t) if in_field => b.text(&t),
            Event::CData(c) if in_field => b.text(&c),
            Event::GeneralRef(r) if in_field => {
                let cr = if r.starts_with('#') {
                    Some(r.resolve_char_ref().map_err(|_| ()))
                } else {
                    None
                };
                let name = r.to_string();
                b.entity(&name, cr);
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if b.root.is_none() {
        return Err(xml_error(text, text.len(), "no root element found"));
    }
    if !stack.is_empty() {
        return Err(xml_error(
            text,
            text.len(),
            "unexpected end of file: document is truncated",
        ));
    }

    let row_element = b.row_element.clone().unwrap_or_default();
    // A schema without MainDataTable would register the row element as a column.
    if let Some(&idx) = b.col_index.get(&row_element) {
        if b.rows.iter().all(|r| r.get(idx).is_none()) {
            b.columns.remove(idx);
            for r in &mut b.rows {
                if r.values.len() > idx {
                    r.values.remove(idx);
                }
            }
        }
    }
    let mut table = Table {
        row_element,
        columns: b.columns,
        rows: b.rows,
        stats: b.stats,
    };
    table.finish();
    Ok(table)
}
