//! Synthetic fixture generator. **Everything here is fake**: invented words,
//! random numbers and tiny placeholder files. It reproduces the *shape* of the
//! index and folder (column sets, omitted elements, entities, padding, case,
//! duplicates, orphans) without containing any real data. Used by the tests
//! and by benchmarks; safe to publish.

use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};

use crate::date::civil_from_days;

/// A temporary directory that is removed on drop.
#[derive(Debug)]
pub struct TempDir(PathBuf);

impl TempDir {
    /// Creates a fresh, empty directory under the system temp dir.
    pub fn new(tag: &str) -> io::Result<TempDir> {
        static N: AtomicUsize = AtomicUsize::new(0);
        let p = std::env::temp_dir().join(format!(
            "oxifsshare-{tag}-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p)?;
        Ok(TempDir(p))
    }

    /// The directory path.
    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Small deterministic xorshift generator (no `rand` dependency).
#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    /// Seeded generator.
    pub fn new(seed: u64) -> Self {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    /// Next 64 random bits.
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    /// Uniform value below `n` (`n` > 0).
    pub fn below(&mut self, n: u64) -> u64 {
        self.next_u64() % n.max(1)
    }
}

/// Which synthetic column set to emit (mirrors the *kinds* of schemas seen).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Variant {
    /// Core columns only.
    Plain,
    /// Core plus invented finance-like columns.
    Financial,
    /// Core plus invented property-like columns.
    Property,
}

/// What to generate.
#[derive(Clone, Debug)]
pub struct Spec {
    /// Number of index rows.
    pub rows: usize,
    /// Random seed.
    pub seed: u64,
    /// Column set.
    pub variant: Variant,
    /// First Document ID.
    pub id_start: u32,
    /// Zero-pad file stems to this width (0 = no padding).
    pub stem_width: usize,
    /// Every Nth row gets no file (0 = none missing).
    pub missing_every: usize,
    /// Extra files that no row refers to.
    pub orphans: usize,
    /// Also create an unrelated `.txt` file.
    pub txt_file: bool,
    /// Rows (from the start) that get a second file with the same key.
    pub duplicates: usize,
    /// Use CRLF line endings in the index.
    pub crlf: bool,
    /// Prefix the index with a UTF-8 byte order mark.
    pub bom: bool,
    /// Upper-case the extension on odd rows.
    pub upper_ext: bool,
    /// Point at the parent folder (true) or at the `fsshare` folder (false).
    pub pass_parent: bool,
    /// Text inserted into every text value, to prove the check report leaks nothing.
    pub canary: String,
    /// Skip writing the document files (index only).
    pub no_files: bool,
}

impl Default for Spec {
    fn default() -> Self {
        Spec {
            rows: 50,
            seed: 1,
            variant: Variant::Plain,
            id_start: 100_000,
            stem_width: 0,
            missing_every: 0,
            orphans: 0,
            txt_file: false,
            duplicates: 0,
            crlf: false,
            bom: false,
            upper_ext: false,
            pass_parent: false,
            canary: String::new(),
            no_files: false,
        }
    }
}

/// Facts about what was generated, for assertions.
#[derive(Clone, Debug, Default)]
pub struct Expected {
    /// Rows in the index.
    pub rows: usize,
    /// Rows that have a file.
    pub with_file: usize,
    /// Rows without a file.
    pub missing: usize,
    /// Files with no row (including the `.txt` if requested).
    pub orphans: usize,
    /// Rows with two candidate files.
    pub ambiguous: usize,
    /// Ids of rows without a file.
    pub missing_ids: Vec<u32>,
}

/// A generated archive on disk.
#[derive(Debug)]
pub struct Fixture {
    /// Owns the directory.
    pub dir: TempDir,
    /// Path to pass to [`crate::Archive::open`].
    pub root: PathBuf,
    /// Path of the index XML.
    pub index_path: PathBuf,
    /// What was generated.
    pub expected: Expected,
}

const DOC_TYPES: [&str; 5] = ["Invoice", "Minutes", "Notice", "Report", "Correspondence"];

fn enc(name: &str) -> String {
    name.replace(' ', "_x0020_")
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn columns(variant: Variant) -> Vec<(&'static str, &'static str)> {
    let mut c = vec![
        ("Date", "xs:dateTime"),
        ("Doc Type", ""),
        ("Plan Number", ""),
        ("Address", ""),
        ("Manager", ""),
    ];
    match variant {
        Variant::Plain => {}
        Variant::Financial => c.extend([
            ("Year End", ""),
            ("Creditor", ""),
            ("Account Number", ""),
            ("Invoice Number", ""),
            ("Invoice Amount", ""),
        ]),
        Variant::Property => c.extend([
            ("Lot Number", ""),
            ("Unit Number", ""),
            ("Owners Name", ""),
            ("Lot Address", ""),
        ]),
    }
    c.extend([
        ("Notes", ""),
        ("Show on Portals", ""),
        ("Portal Description", ""),
        ("Archive Date", "xs:dateTime"),
        ("User Name", ""),
        ("WorkItemId", "xs:string"),
        ("Document ID", "xs:int"),
    ]);
    c
}

fn schema(cols: &[(&str, &str)]) -> String {
    let mut s = String::from(
        "  <xs:schema id=\"NewDataSet\" xmlns=\"\" xmlns:xs=\"http://www.w3.org/2001/XMLSchema\" xmlns:msdata=\"urn:schemas-microsoft-com:xml-msdata\">\n\
    <xs:element name=\"NewDataSet\" msdata:IsDataSet=\"true\" msdata:MainDataTable=\"documents\" msdata:UseCurrentLocale=\"true\">\n\
      <xs:complexType>\n        <xs:choice minOccurs=\"0\" maxOccurs=\"unbounded\">\n          <xs:element name=\"documents\">\n            <xs:complexType>\n              <xs:sequence>\n",
    );
    for (name, ty) in cols {
        let n = enc(name);
        if ty.is_empty() {
            s.push_str(&format!(
                "                <xs:element name=\"{n}\" msdata:ReadOnly=\"true\" minOccurs=\"0\">\n                  <xs:simpleType>\n                    <xs:restriction base=\"xs:string\">\n                      <xs:maxLength value=\"2147483647\" />\n                    </xs:restriction>\n                  </xs:simpleType>\n                </xs:element>\n"
            ));
        } else {
            s.push_str(&format!(
                "                <xs:element name=\"{n}\" type=\"{ty}\" minOccurs=\"0\" />\n"
            ));
        }
    }
    s.push_str("              </xs:sequence>\n            </xs:complexType>\n          </xs:element>\n        </xs:choice>\n      </xs:complexType>\n    </xs:element>\n  </xs:schema>\n");
    s
}

fn stamp(rng: &mut Rng) -> String {
    let day = 14_000 + rng.below(4_000) as i64; // roughly 2008 to 2019
    let (y, m, d) = civil_from_days(day);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.{:07}+10:00",
        rng.below(24),
        rng.below(60),
        rng.below(60),
        rng.below(10_000_000)
    )
}

fn row_values(
    spec: &Spec,
    cols: &[(&str, &str)],
    i: usize,
    id: u32,
    rng: &mut Rng,
) -> Vec<Option<String>> {
    let canary = &spec.canary;
    cols.iter()
        .map(|(name, _)| match *name {
            "Date" | "Archive Date" => Some(stamp(rng)),
            "Doc Type" => Some(DOC_TYPES[rng.below(5) as usize].to_string()),
            "Plan Number" => Some(format!("SP{}", 1000 + rng.below(9000))),
            "Address" | "Lot Address" => {
                Some(format!("{} Example Street {canary}", 1 + rng.below(300)))
            }
            "Manager" | "Owners Name" | "Creditor" => {
                Some(format!("Person {} {canary}", rng.below(40)))
            }
            "Invoice Amount" => Some(format!("{}.{:02}", rng.below(5000), rng.below(100))),
            "Notes" => match rng.below(4) {
                0 => None,
                1 => Some(format!(" leading space & \"quotes\" <tag> {canary}")),
                2 => Some(format!("line one {canary}\nline two\n\nline four & more")),
                _ => Some(format!("short note {i} {canary}")),
            },
            "Show on Portals" => (rng.below(4) == 0).then(|| "True".to_string()),
            "Portal Description" => (rng.below(8) == 0).then(|| format!("portal text {canary}")),
            "User Name" => Some(format!("user{} {canary}", rng.below(9))),
            "WorkItemId" => (rng.below(2) == 0).then(|| {
                format!(
                    "{:08x}-0000-4000-8000-{:012x}",
                    rng.below(u64::from(u32::MAX)),
                    rng.below(1 << 40)
                )
            }),
            "Document ID" => Some(id.to_string()),
            other => Some(format!("{other}-{} {canary}", rng.below(1000))),
        })
        .collect()
}

/// Generates only the index text (no files). `Document ID` is `id_start + row`.
pub fn synth_xml(spec: &Spec) -> String {
    let cols = columns(spec.variant);
    let mut rng = Rng::new(spec.seed);
    let mut s = String::with_capacity(spec.rows * 400 + 4096);
    s.push_str("<?xml version=\"1.0\" standalone=\"yes\"?>\n<NewDataSet>\n");
    s.push_str(&schema(&cols));
    for i in 0..spec.rows {
        let id = spec.id_start + i as u32;
        s.push_str("  <documents>\n");
        for ((name, _), v) in cols.iter().zip(row_values(spec, &cols, i, id, &mut rng)) {
            if let Some(v) = v {
                let n = enc(name);
                s.push_str(&format!("    <{n}>{}</{n}>\n", escape(&v)));
            }
        }
        s.push_str("  </documents>\n");
    }
    s.push_str("</NewDataSet>");
    let mut out = if spec.crlf {
        s.replace('\n', "\r\n")
    } else {
        s
    };
    if spec.bom {
        out.insert(0, '\u{feff}');
    }
    out
}

fn stem(spec: &Spec, id: u32) -> String {
    if spec.stem_width == 0 {
        id.to_string()
    } else {
        format!("{id:0>w$}", w = spec.stem_width)
    }
}

/// Generates an archive folder on disk: `<tmp>/fsshare/fs-share.xml` plus fake documents.
pub fn synth_archive(spec: &Spec) -> io::Result<Fixture> {
    let dir = TempDir::new("fx")?;
    let (root, index_path, expected) = synth_archive_in(dir.path(), spec)?;
    Ok(Fixture {
        dir,
        root,
        index_path,
        expected,
    })
}

/// Like [`synth_archive`] but writes into `base/fsshare` and keeps the files.
/// Returns the path to open, the index path and the expectations.
pub fn synth_archive_in(base: &Path, spec: &Spec) -> io::Result<(PathBuf, PathBuf, Expected)> {
    let folder = base.join("fsshare");
    fs::create_dir_all(&folder)?;
    let index_path = folder.join("fs-share.xml");
    fs::write(&index_path, synth_xml(spec).as_bytes())?;

    let mut exp = Expected {
        rows: spec.rows,
        ..Expected::default()
    };
    // A real (tiny, invented) Outlook message, so converting a fixture .msg works.
    let msg_bytes = crate::testmsg::build_msg(&crate::testmsg::SynthMsg {
        subject: "Synthetic message".into(),
        body: Some("Synthetic body text.".into()),
        sender: Some(("Sender Example".into(), "sender@example.org".into())),
        recipients: vec![crate::testmsg::SynthRecipient {
            kind: 1,
            name: "Receiver Example".into(),
            smtp: Some("receiver@example.org".into()),
            addr_type_and_addr: None,
        }],
        filetime: Some(133_000_000_000_000_000),
        ..crate::testmsg::SynthMsg::default()
    });
    for i in 0..spec.rows {
        let id = spec.id_start + i as u32;
        if spec.missing_every > 0 && i % spec.missing_every == spec.missing_every - 1 {
            exp.missing += 1;
            exp.missing_ids.push(id);
            continue;
        }
        exp.with_file += 1;
        if spec.no_files {
            continue;
        }
        let (ext, body): (&str, &[u8]) = match i % 20 {
            0..=10 => ("pdf", b"%PDF-1.4\n% synthetic placeholder\n"),
            11..=14 => ("tif", b"II*\0synthetic placeholder"),
            _ => ("msg", &msg_bytes[..]),
        };
        let ext = if spec.upper_ext && i % 2 == 1 {
            ext.to_uppercase()
        } else {
            ext.to_string()
        };
        fs::write(folder.join(format!("{}.{ext}", stem(spec, id))), body)?;
        if i < spec.duplicates {
            let dup = if ext.eq_ignore_ascii_case("tif") {
                "pdf"
            } else {
                "tif"
            };
            fs::write(
                folder.join(format!("{}.{dup}", stem(spec, id))),
                b"synthetic duplicate",
            )?;
            exp.ambiguous += 1;
        }
    }
    if !spec.no_files {
        for k in 0..spec.orphans {
            let id = spec.id_start + spec.rows as u32 + 100 + k as u32;
            fs::write(
                folder.join(format!("{}.pdf", stem(spec, id))),
                b"%PDF-1.4\n% orphan\n",
            )?;
            exp.orphans += 1;
        }
        if spec.txt_file {
            fs::write(folder.join("notes-synthetic.txt"), b"synthetic text file")?;
            exp.orphans += 1;
        }
    }
    let root = if spec.pass_parent {
        base.to_path_buf()
    } else {
        folder
    };
    Ok((root, index_path, exp))
}
