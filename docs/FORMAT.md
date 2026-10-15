# Archive format (as known)

This describes the export format that oxifsshare reads. It was inferred from
structure-only diagnostics of three real archives; no vendor code or binaries
were used. Status: **observed**, not specified by the vendor.

## Layout

A folder (originally named `fsshare`) containing:

- one XML index, `fs-share.xml`;
- the documents, in the same folder (flat, no sub-folders seen);
- in every archive examined, one extra `.txt` file with no matching row (reported as an orphan).

You may point oxifsshare at the XML file, at the folder holding it, or at a
parent folder that contains an `fsshare` sub-folder (matched case-insensitively).
If `fs-share.xml` is absent but the folder holds exactly one `.xml` file, that file is used.

Document files seen: PDF (versions 1.3 to 1.7, some with compressed object
streams), TIFF (CCITT Group 4, bilevel, A4/Letter-size scans, some written by a
scanner utility), Outlook `.msg` (compound files), and rarely others.
Extensions are lower-case in the archives examined; matching ignores case anyway.

## Index: `fs-share.xml`

Written by .NET `DataSet.WriteXml`. Read here generically.

```
<NewDataSet>
  <xs:schema ...>   declares columns, order and types (optional for oxifsshare)
  <documents> ... </documents>   one per entry, repeated
</NewDataSet>
```

- Element names escape characters as `_xHHHH_` (`Doc_x0020_Type` is `Doc Type`). All such sequences are decoded.
- A column whose value is null is **omitted** from that row. An element that is present but empty (`<Notes />`) is an empty string. These are different and both are preserved.
- Values are not trimmed (a leading space was observed in real data); trimming happens only for display, search and matching.
- Line endings are mixed CRLF/LF in some files and CRLF only in others; both are accepted and values are normalised to `\n`.
- Encoding: ASCII/UTF-8 observed. A UTF-8 BOM and UTF-16 with BOM are also accepted. Other encodings produce an error with a position.
- Entities: the five predefined ones and numeric character references. An undefined entity is kept as literal text and counted rather than failing the load.
- The column set differs between archives. Columns are discovered from the schema (order and `xs:dateTime`/`xs:int` types) and from the data, so unexpected columns do not break loading. A column that appears only in the data is appended after the schema columns.

### Columns

Present in every archive examined:

| Column | Type | Notes |
|---|---|---|
| Date | dateTime, optional | ISO 8601 with fraction and offset |
| Doc Type | string, optional | used for the type filter |
| Plan Number | string, optional | |
| Address | string, optional | |
| Manager | string, optional | |
| Notes | string, optional | can be long and multi-line |
| Show on Portals | string, optional | |
| Portal Description | string, optional | |
| Archive Date | dateTime, **required** | |
| User Name | string, optional | |
| WorkItemId | GUID as string, optional | |
| Document ID | int, **required** | links a row to its file |

Each archive adds its own domain-specific string columns between the common
ones. Only column **names** are listed here, never values. oxifsshare needs no
knowledge of them; they are discovered from the schema and the data.

| Archive | Extra columns (in schema order, after `Manager`) |
|---|---|
| A | none beyond the common set |
| B (finance-style) | Year End, Creditor, ABN, Invoice Number, Insurance Claim, Account Number, Account Name, Invoice Amount (these come before `Notes`) |
| C (property-style) | Lot Number, Unit Number, Owners Name, Lot Address (these come before `Notes`) |

In archives B and C the extra columns sit between `Manager` and `Notes`.
`Invoice Amount` is declared as a string; oxifsshare sorts a column numerically
when every value in it parses as a number.

## Linking rows to files

Observed in the original viewer: the file name is `<Document ID>.<extension>`;
the viewer took the first file matching `<name>.*` and chose a handler from the
extension; if nothing matched it showed "document not found".

oxifsshare rules (Linux file systems are case-sensitive, unlike the original platform):

1. A file belongs to key `K` when its name is `K.<anything>`. The key is the part before the **first** dot.
2. Keys compare case-insensitively. All-digit keys compare numerically, so `000123` equals `123`.
3. Files without an extension, hidden files (leading dot), symbolic links and sub-folders are ignored and counted.
4. If several files share a key, the choice is deterministic: lowest extension rank (`pdf`, `tif`, `tiff`, `jpg`, `jpeg`, `msg`, then anything else), then lowest file name (byte order). Rows with more than one candidate are counted as ambiguous, and the files that lose are counted as shadowed.
5. The key column defaults to `Document ID`. If fewer than 50% of rows resolve, every column is tested and the best-matching one is used (ties go to the earlier column); the default is kept unless another column is strictly better.

Observed in the first archive: 548 of 555 key values and file stems are 6
digits and the rest are 3 or 4 digits; none had leading zeros. The lengths of
IDs and stems agree, which supports "file stem = Document ID".

## Open questions

- Exact behaviour of the original viewer when two files share a key (the rule above is our own).
- Whether any archive uses sub-folders, or a key other than Document ID.
- What the lone `.txt` file in each archive is (treated as an orphan).
- How the real `.msg` files in an archive differ from the public samples used to test the converter (see `docs/MSG.md`).
- Whether other `xs:` types or additional metadata tables occur in other exports.
