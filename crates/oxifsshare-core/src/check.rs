//! The `--check` report: structure, counts and match rates only.
//!
//! Privacy contract (enforced by a test): the report never contains values
//! from the index (no notes, names, addresses, dates) and never contains file
//! names. Column names and file extensions are printed because they describe
//! structure. Document IDs of unmatched rows and orphan files are printed only
//! when `show_ids` is set, and only if they are short all-digit strings.

use std::{collections::BTreeMap, fmt::Write as _, fs, path::Path};

use crate::{
    archive::{Archive, FileState},
    date,
    error::Error,
    index::{norm_key, FileKind},
    model::ColumnKind,
};

fn pct(n: usize, total: usize) -> String {
    if total == 0 {
        "n/a".into()
    } else {
        format!("{:.1}%", n as f64 * 100.0 / total as f64)
    }
}

fn is_safe_id(s: &str) -> bool {
    !s.is_empty() && s.len() <= 12 && s.bytes().all(|b| b.is_ascii_digit())
}

fn kind_label(k: ColumnKind) -> &'static str {
    match k {
        ColumnKind::Text => "text",
        ColumnKind::Int => "int",
        ColumnKind::DateTime => "datetime",
        ColumnKind::Number => "number",
    }
}

/// Opens the archive at `path` and renders the report.
pub fn run(path: &Path, show_ids: bool) -> Result<String, Error> {
    let archive = Archive::open(path)?;
    Ok(render(&archive, show_ids))
}

/// Renders the report for an already opened archive.
pub fn render(a: &Archive, show_ids: bool) -> String {
    let mut o = String::new();
    let t = &a.table;
    let st = a.stats();
    let rows = t.rows.len();

    let _ = writeln!(
        o,
        "oxifsshare check report (structure and counts only; no values)"
    );
    let _ = writeln!(
        o,
        "================================================================"
    );
    let _ = writeln!(o, "\n[Index]");
    let size = fs::metadata(&a.location.index_path).map_or(0, |m| m.len());
    let _ = writeln!(o, "  index file size: {size} bytes");
    let enc = if t.stats.utf16 {
        "UTF-16"
    } else if t.stats.bom {
        "UTF-8 with BOM"
    } else {
        "UTF-8"
    };
    let _ = writeln!(o, "  encoding: {enc}");
    let _ = writeln!(
        o,
        "  line endings: CRLF {}, LF {}",
        t.stats.crlf, t.stats.lf
    );
    let _ = writeln!(
        o,
        "  inline schema: {}",
        if t.stats.schema_found { "yes" } else { "no" }
    );
    let _ = writeln!(
        o,
        "  found via fsshare sub-folder: {}",
        if a.location.via_subfolder {
            "yes"
        } else {
            "no"
        }
    );
    let _ = writeln!(o, "  rows: {rows}");
    let _ = writeln!(o, "  columns: {}", t.columns.len());
    let _ = writeln!(
        o,
        "  oddities: unknown entities {}, nested elements {}, duplicate fields {}, other root children {}",
        t.stats.unknown_entities, t.stats.nested_elements, t.stats.duplicate_fields, t.stats.other_root_children
    );

    let _ = writeln!(o, "\n[Columns] name | kind | populated | unparsable dates");
    for (i, c) in t.columns.iter().enumerate() {
        let filled = t
            .rows
            .iter()
            .filter(|r| r.get(i).is_some_and(|v| !v.trim().is_empty()))
            .count();
        let bad = if c.kind == ColumnKind::DateTime {
            let n = t
                .rows
                .iter()
                .filter_map(|r| r.get(i))
                .filter(|v| !v.trim().is_empty() && date::parse_datetime(v).is_none())
                .count();
            n.to_string()
        } else {
            "-".into()
        };
        let _ = writeln!(
            o,
            "  {} | {} | {} ({filled}) | {bad}",
            c.name,
            kind_label(c.kind),
            pct(filled, rows)
        );
    }

    let _ = writeln!(o, "\n[Folder]");
    let f = &a.folder;
    let _ = writeln!(o, "  document files (index excluded): {}", f.files.len());
    let mut by_ext: BTreeMap<&str, usize> = BTreeMap::new();
    for file in &f.files {
        *by_ext.entry(file.ext.as_str()).or_default() += 1;
    }
    for (ext, n) in &by_ext {
        let _ = writeln!(o, "    .{ext}: {n}");
    }
    let _ = writeln!(
        o,
        "  ignored: sub-folders {}, symlinks {}, hidden {}, no extension {}, unreadable {}",
        f.subdirs, f.symlinks, f.hidden, f.no_extension, f.unreadable
    );
    let mut stem_len: BTreeMap<usize, usize> = BTreeMap::new();
    for file in &f.files {
        *stem_len.entry(file.stem.chars().count()).or_default() += 1;
    }
    let _ = writeln!(
        o,
        "  file-name stem lengths (length: count): {}",
        hist(&stem_len)
    );
    let digit_stems = f
        .files
        .iter()
        .filter(|x| x.stem.bytes().all(|b| b.is_ascii_digit()))
        .count();
    let _ = writeln!(o, "  stems that are all digits: {digit_stems}");
    let lead0 = f
        .files
        .iter()
        .filter(|x| x.stem.starts_with('0') && x.stem.len() > 1)
        .count();
    let _ = writeln!(o, "  stems with leading zeros: {lead0}");

    let _ = writeln!(o, "\n[Key column]");
    match a.key.chosen {
        None => {
            let _ = writeln!(o, "  no columns, nothing to match");
        }
        Some(c) => {
            let name = &t.columns[c].name;
            let how = if a.key.candidates.is_empty() {
                "default"
            } else {
                "auto-detected"
            };
            let _ = writeln!(
                o,
                "  chosen: {name} ({how}), resolves {:.1}% of rows",
                a.key.chosen_rate * 100.0
            );
            match a.key.default {
                Some(d) => {
                    let _ = writeln!(
                        o,
                        "  'Document ID' present at column {}: {:.1}% resolve",
                        d + 1,
                        a.key.default_rate * 100.0
                    );
                }
                None => {
                    let _ = writeln!(o, "  'Document ID' column not present");
                }
            }
            if !a.key.candidates.is_empty() {
                let _ = writeln!(o, "  every column tested as key (name: match rate):");
                for &(ci, r) in &a.key.candidates {
                    let _ = writeln!(o, "    {}: {:.1}%", t.columns[ci].name, r * 100.0);
                }
            }
            let mut key_len: BTreeMap<usize, usize> = BTreeMap::new();
            let mut seen: BTreeMap<String, usize> = BTreeMap::new();
            let mut padded_diff = 0usize;
            for (i, r) in t.rows.iter().enumerate() {
                let v = r.get(c).unwrap_or("").trim();
                *key_len.entry(v.chars().count()).or_default() += 1;
                if !v.is_empty() {
                    *seen.entry(norm_key(v)).or_default() += 1;
                }
                if let Some(file) = a.row_file(i) {
                    if !file.stem.eq_ignore_ascii_case(v) {
                        padded_diff += 1;
                    }
                }
            }
            let _ = writeln!(o, "  key value lengths (length: count): {}", hist(&key_len));
            let dups = seen.values().filter(|&&n| n > 1).count();
            let _ = writeln!(o, "  key values used by more than one row: {dups}");
            let _ = writeln!(o, "  matched rows where key text differs from file stem (padding/case): {padded_diff}");
        }
    }

    let _ = writeln!(o, "\n[Matching]");
    let _ = writeln!(
        o,
        "  rows with a file: {} ({})",
        st.matched,
        pct(st.matched, rows)
    );
    let _ = writeln!(
        o,
        "  rows without a file: {} ({})",
        st.missing,
        pct(st.missing, rows)
    );
    let _ = writeln!(o, "  rows with an empty key: {}", st.empty_key);
    let _ = writeln!(
        o,
        "  rows with more than one candidate file: {}",
        st.ambiguous
    );
    let _ = writeln!(o, "  orphan files (no row): {}", st.orphans);
    let _ = writeln!(
        o,
        "  shadowed files (lost the deterministic choice): {}",
        st.shadowed
    );
    let mut kinds: BTreeMap<FileKind, usize> = BTreeMap::new();
    for i in 0..rows {
        *kinds.entry(a.row_kind(i)).or_default() += 1;
    }
    let parts: Vec<String> = kinds
        .iter()
        .map(|(k, n)| format!("{} {n}", k.label()))
        .collect();
    let _ = writeln!(o, "  rows by file kind: {}", parts.join(", "));
    let mut orphan_ext: BTreeMap<&str, usize> = BTreeMap::new();
    for (i, file) in f.files.iter().enumerate() {
        if a.file_state(i) == FileState::Orphan {
            *orphan_ext.entry(file.ext.as_str()).or_default() += 1;
        }
    }
    let parts: Vec<String> = orphan_ext
        .iter()
        .map(|(e, n)| format!(".{e} {n}"))
        .collect();
    let _ = writeln!(
        o,
        "  orphan files by extension: {}",
        if parts.is_empty() {
            "none".into()
        } else {
            parts.join(", ")
        }
    );

    messages_section(&mut o, a);

    if show_ids {
        let _ = writeln!(o, "\n[IDs] (printed because --show-ids was given)");
        let mut ids: Vec<&str> = Vec::new();
        let mut unsafe_n = 0usize;
        if let Some(c) = a.key.chosen {
            for (i, r) in t.rows.iter().enumerate() {
                let v = r.get(c).unwrap_or("").trim();
                if a.resolution(i).file.is_none() && !v.is_empty() {
                    if is_safe_id(v) {
                        ids.push(v);
                    } else {
                        unsafe_n += 1;
                    }
                }
            }
        }
        let _ = writeln!(
            o,
            "  rows without a file: {}{}",
            list(&ids),
            suffix(unsafe_n)
        );
        let mut orphans: Vec<&str> = Vec::new();
        let mut unsafe_o = 0usize;
        for (i, file) in f.files.iter().enumerate() {
            if a.file_state(i) == FileState::Orphan {
                if is_safe_id(&file.stem) {
                    orphans.push(file.stem.as_str());
                } else {
                    unsafe_o += 1;
                }
            }
        }
        let _ = writeln!(
            o,
            "  orphan file stems: {}{}",
            list(&orphans),
            suffix(unsafe_o)
        );
    } else {
        let _ = writeln!(
            o,
            "\n(IDs of unmatched rows and files are hidden; add --show-ids to print them.)"
        );
    }
    o
}

/// `[Messages]`: tries the `.msg` conversion on every `.msg` file and reports counts only.
fn messages_section(o: &mut String, a: &Archive) {
    use crate::msg::{convert_file, Info};
    let f = &a.folder;
    let paths: Vec<&std::path::Path> = f
        .files
        .iter()
        .filter(|x| x.ext == "msg")
        .map(|x| x.path.as_path())
        .collect();
    if paths.is_empty() {
        return;
    }
    let mut ok = 0usize;
    let mut failed: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut t = Info::default();
    let (mut text, mut html, mut rtf_only, mut ansi, mut headers, mut no_date) =
        (0usize, 0usize, 0usize, 0usize, 0usize, 0usize);
    for p in &paths {
        match convert_file(p) {
            Ok(c) => {
                ok += 1;
                let i = c.info;
                text += usize::from(i.text_body);
                html += usize::from(i.html_body);
                rtf_only += usize::from(i.rtf_only);
                ansi += usize::from(i.ansi_strings);
                headers += usize::from(i.transport_headers);
                no_date += usize::from(i.no_date);
                t.recipients += i.recipients;
                t.recipients_no_smtp += i.recipients_no_smtp;
                t.attachments += i.attachments;
                t.embedded_messages += i.embedded_messages;
                t.skipped_attachments += i.skipped_attachments;
            }
            Err(e) => *failed.entry(e.category()).or_default() += 1,
        }
    }
    let _ = writeln!(
        o,
        "\n[Messages] (.msg to .eml conversion test; nothing is written)"
    );
    let _ = writeln!(o, "  .msg files: {}", paths.len());
    let _ = writeln!(
        o,
        "  converted without error: {ok} ({})",
        pct(ok, paths.len())
    );
    if failed.is_empty() {
        let _ = writeln!(o, "  failures: none");
    } else {
        let parts: Vec<String> = failed.iter().map(|(k, n)| format!("{k} {n}")).collect();
        let _ = writeln!(o, "  failures by reason: {}", parts.join(", "));
    }
    let _ = writeln!(
        o,
        "  with plain-text body: {text}; with HTML body: {html}; RTF-only (body not converted): {rtf_only}"
    );
    let _ = writeln!(
        o,
        "  recipients: {} (without an SMTP address: {})",
        t.recipients, t.recipients_no_smtp
    );
    let _ = writeln!(
        o,
        "  attachments: {} (embedded messages {}, skipped {})",
        t.attachments, t.embedded_messages, t.skipped_attachments
    );
    let _ = writeln!(
        o,
        "  8-bit text: {ansi}; original internet headers present: {headers}; no usable date: {no_date}"
    );
}

fn hist(m: &BTreeMap<usize, usize>) -> String {
    if m.is_empty() {
        return "none".into();
    }
    m.iter()
        .map(|(l, n)| format!("{l}: {n}"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn list(ids: &[&str]) -> String {
    const CAP: usize = 500;
    if ids.is_empty() {
        return "none".into();
    }
    let shown = ids.iter().take(CAP).copied().collect::<Vec<_>>().join(", ");
    if ids.len() > CAP {
        format!("{shown} ... (+{} more)", ids.len() - CAP)
    } else {
        shown
    }
}

fn suffix(n: usize) -> String {
    if n == 0 {
        String::new()
    } else {
        format!(" (+{n} non-numeric not shown)")
    }
}
