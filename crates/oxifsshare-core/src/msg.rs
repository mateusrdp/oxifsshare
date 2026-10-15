//! Outlook `.msg` to `.eml` conversion, in pure Rust (the `cfb` crate reads the
//! OLE compound file; everything else is implemented here).
//!
//! Format notes (MS-OXMSG / MS-OXPROPS as understood; checked against public sample
//! files and an independent parser, see `docs/MSG.md`):
//! * A message is a compound file. String and binary properties are streams named
//!   `__substg1.0_<id:4 hex><type:4 hex>`: type `001F` is UTF-16LE, `001E` is 8-bit text
//!   in the message code page, `0102` is binary, `000D` is an embedded storage.
//! * Fixed-size properties live in `__properties_version1.0` as 16-byte entries
//!   (tag, flags, 8-byte value) after a header of 32 bytes (top-level message), 24
//!   bytes (embedded message) or 8 bytes (recipient or attachment).
//! * Recipients are storages `__recip_version1.0_#NNNNNNNN`, attachments
//!   `__attach_version1.0_#NNNNNNNN`.
//!
//! Known gaps: an RTF-only body is not converted (the message gets a short notice
//! instead), named properties are ignored, and OLE-object and by-reference
//! attachments are skipped and counted.
//!
//! Privacy: errors and [`Info`] contain only categories and counts, never message
//! content, names or addresses.

use std::{
    collections::HashMap,
    fmt, fs,
    io::{self, Cursor, Read},
    path::Path,
};

use crate::{
    date::civil_from_days,
    mime::{self, ascii_only, clean_header},
};

const MAX_FILE: u64 = 512 * 1024 * 1024;
const MAX_STREAM: u64 = 256 * 1024 * 1024;
const MAX_OUTPUT: usize = 1024 * 1024 * 1024;
const MAX_CHILDREN: usize = 5_000;
const MAX_PROPS: usize = 100_000;
const MAX_DEPTH: u32 = 4;

/// Why a `.msg` could not be converted. Messages never contain file content.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MsgError {
    /// The file could not be read.
    Io(io::ErrorKind),
    /// Not an OLE compound file (or it is damaged beyond reading).
    NotCompoundFile,
    /// A compound file, but without the streams every message has.
    NotAMessage,
    /// A stream or directory entry could not be read.
    Corrupt,
    /// The file, a stream or the result exceeds the safety limits.
    TooLarge,
}

impl MsgError {
    /// Stable short name used in the `--check` report.
    pub fn category(&self) -> &'static str {
        match self {
            MsgError::Io(_) => "unreadable",
            MsgError::NotCompoundFile => "not-a-compound-file",
            MsgError::NotAMessage => "not-a-message",
            MsgError::Corrupt => "corrupt",
            MsgError::TooLarge => "too-large",
        }
    }
}

impl fmt::Display for MsgError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MsgError::Io(k) => write!(f, "the .msg file could not be read ({k})"),
            MsgError::NotCompoundFile => write!(
                f,
                "the file is not an Outlook message (not an OLE compound file)"
            ),
            MsgError::NotAMessage => write!(
                f,
                "the file is a compound file but does not look like an Outlook message"
            ),
            MsgError::Corrupt => write!(f, "the message is damaged and could not be fully read"),
            MsgError::TooLarge => write!(f, "the message is larger than the safety limits allow"),
        }
    }
}

impl std::error::Error for MsgError {}

/// Counts describing what a conversion found. No content.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Info {
    /// A plain-text body was present.
    pub text_body: bool,
    /// An HTML body was present.
    pub html_body: bool,
    /// Only a (compressed) RTF body existed; it was not converted.
    pub rtf_only: bool,
    /// Recipients written.
    pub recipients: u32,
    /// Recipients for which no SMTP address could be found.
    pub recipients_no_smtp: u32,
    /// Attachments written (embedded messages included).
    pub attachments: u32,
    /// Embedded messages among them.
    pub embedded_messages: u32,
    /// Attachments skipped (by reference, OLE objects, unreadable, too deep).
    pub skipped_attachments: u32,
    /// Strings were stored as 8-bit text rather than UTF-16.
    pub ansi_strings: bool,
    /// The message carried the original internet headers.
    pub transport_headers: bool,
    /// No date could be determined.
    pub no_date: bool,
}

/// A converted message.
#[derive(Debug)]
pub struct Converted {
    /// The RFC 5322 message, 7-bit ASCII with CRLF line endings.
    pub eml: Vec<u8>,
    /// What was found.
    pub info: Info,
}

/// Reads and converts a `.msg` file.
pub fn convert_file(path: &Path) -> Result<Converted, MsgError> {
    let meta = fs::metadata(path).map_err(|e| MsgError::Io(e.kind()))?;
    if meta.len() > MAX_FILE {
        return Err(MsgError::TooLarge);
    }
    let bytes = fs::read(path).map_err(|e| MsgError::Io(e.kind()))?;
    convert_bytes(bytes)
}

/// Converts the bytes of a `.msg` file. Never panics on bad input.
pub fn convert_bytes(bytes: Vec<u8>) -> Result<Converted, MsgError> {
    if bytes.len() as u64 > MAX_FILE {
        return Err(MsgError::TooLarge);
    }
    let cf = cfb::CompoundFile::open(Cursor::new(bytes)).map_err(|_| MsgError::NotCompoundFile)?;
    let mut doc = Doc {
        cf,
        written: 0,
        saw_ansi: false,
    };
    let mut ctx = Ctx {
        counter: 0,
        info: Info::default(),
    };
    let eml = doc.message("", 32, 0, &mut ctx)?;
    Ok(Converted {
        eml,
        info: ctx.info,
    })
}

// ---------------------------------------------------------------- text and code pages

#[derive(Clone, Debug)]
enum Text {
    Uni(String),
    Ansi(Vec<u8>, u32),
}

const CP1252_HIGH: [char; 32] = [
    '\u{20AC}', '\u{81}', '\u{201A}', '\u{192}', '\u{201E}', '\u{2026}', '\u{2020}', '\u{2021}',
    '\u{2C6}', '\u{2030}', '\u{160}', '\u{2039}', '\u{152}', '\u{8D}', '\u{17D}', '\u{8F}',
    '\u{90}', '\u{2018}', '\u{2019}', '\u{201C}', '\u{201D}', '\u{2022}', '\u{2013}', '\u{2014}',
    '\u{2DC}', '\u{2122}', '\u{161}', '\u{203A}', '\u{153}', '\u{9D}', '\u{17E}', '\u{178}',
];

/// IANA charset name for a Windows code page (unknown values fall back to windows-1252).
fn charset_for(cp: u32) -> &'static str {
    match cp {
        65001 => "utf-8",
        20127 => "us-ascii",
        1250 => "windows-1250",
        1251 => "windows-1251",
        1253 => "windows-1253",
        1254 => "windows-1254",
        1255 => "windows-1255",
        1256 => "windows-1256",
        1257 => "windows-1257",
        1258 => "windows-1258",
        28591 => "iso-8859-1",
        28592 => "iso-8859-2",
        28593 => "iso-8859-3",
        28594 => "iso-8859-4",
        28595 => "iso-8859-5",
        28596 => "iso-8859-6",
        28597 => "iso-8859-7",
        28598 => "iso-8859-8",
        28599 => "iso-8859-9",
        28605 => "iso-8859-15",
        20866 => "koi8-r",
        21866 => "koi8-u",
        932 => "shift_jis",
        936 => "gb2312",
        949 => "euc-kr",
        950 => "big5",
        51932 => "euc-jp",
        50220..=50222 => "iso-2022-jp",
        10000 => "macintosh",
        437 => "ibm437",
        850 => "ibm850",
        _ => "windows-1252",
    }
}

/// ANSI code page for a Windows locale id (primary language ids that matter in practice).
fn lcid_codepage(lcid: u32) -> Option<u32> {
    match lcid & 0x3FF {
        0x19 | 0x22 | 0x23 | 0x02 | 0x2F | 0x3F | 0x44 | 0x28 | 0x85 | 0x50 => Some(1251), // ru uk be bg mk kk tt tg sah mn
        0x05 | 0x15 | 0x0E | 0x1B | 0x24 | 0x18 | 0x1A | 0x41 | 0x1C => Some(1250), // cs pl hu sk sl ro hr/sr/bs sq (latin)
        0x08 => Some(1253),
        0x1F => Some(1254),
        0x0D => Some(1255),
        0x01 | 0x20 | 0x29 => Some(1256),
        0x25..=0x27 => Some(1257),
        0x2A => Some(1258),
        0x11 => Some(932),
        0x04 => Some(if lcid == 0x0404 || lcid == 0x0C04 || lcid == 0x1404 {
            950
        } else {
            936
        }),
        0x12 => Some(949),
        0x00 => None,
        _ => Some(1252),
    }
}

/// Decodes 8-bit text when the code page is one we can decode exactly.
fn decode_ansi(bytes: &[u8], cp: u32) -> Option<String> {
    match cp {
        65001 => String::from_utf8(bytes.to_vec()).ok(),
        20127 => bytes
            .is_ascii()
            .then(|| String::from_utf8_lossy(bytes).into_owned()),
        28591 => Some(bytes.iter().map(|&b| char::from(b)).collect()),
        1252 | 0 => Some(
            bytes
                .iter()
                .map(|&b| {
                    if (0x80..0xA0).contains(&b) {
                        CP1252_HIGH[usize::from(b - 0x80)]
                    } else {
                        char::from(b)
                    }
                })
                .collect(),
        ),
        _ => None,
    }
}

fn utf16le(bytes: &[u8]) -> String {
    let units: Vec<u16> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_le_bytes(*c))
        .collect();
    String::from_utf16_lossy(&units)
        .trim_end_matches('\0')
        .to_string()
}

impl Text {
    fn resolved(self) -> Text {
        match self {
            Text::Ansi(b, cp) => match decode_ansi(&b, cp) {
                Some(s) => Text::Uni(s.trim_end_matches('\0').to_string()),
                None => Text::Ansi(b, cp),
            },
            t => t,
        }
    }

    /// Header-safe text and whether it is plain ASCII.
    fn header(&self) -> (String, bool) {
        let e = match self {
            Text::Uni(s) => mime::encode_text(s),
            Text::Ansi(b, cp) => mime::encode_raw_text(b, charset_for(*cp)),
        };
        let plain = !e.contains("=?");
        (e, plain)
    }

    /// Best-effort Unicode, for file names and comparisons.
    fn lossy(&self) -> String {
        match self {
            Text::Uni(s) => s.clone(),
            Text::Ansi(b, _) => b.iter().map(|&c| char::from(c)).collect(),
        }
    }

    fn is_empty(&self) -> bool {
        match self {
            Text::Uni(s) => s.trim().is_empty(),
            Text::Ansi(b, _) => b.iter().all(u8::is_ascii_whitespace),
        }
    }
}

// ---------------------------------------------------------------- properties

struct Props(HashMap<u16, (u16, [u8; 8])>);

impl Props {
    fn parse(data: &[u8], header: usize) -> Props {
        let mut m = HashMap::new();
        for chunk in data
            .get(header..)
            .unwrap_or(&[])
            .as_chunks::<16>()
            .0
            .iter()
            .take(MAX_PROPS)
        {
            let tag = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
            let mut v = [0u8; 8];
            v.copy_from_slice(&chunk[8..16]);
            m.insert((tag >> 16) as u16, ((tag & 0xFFFF) as u16, v));
        }
        Props(m)
    }

    fn long(&self, id: u16) -> Option<u32> {
        match self.0.get(&id) {
            Some((0x0003, v)) => Some(u32::from_le_bytes([v[0], v[1], v[2], v[3]])),
            _ => None,
        }
    }

    fn time(&self, id: u16) -> Option<u64> {
        match self.0.get(&id) {
            Some((0x0040, v)) => Some(u64::from_le_bytes(*v)),
            _ => None,
        }
    }
}

/// FILETIME (100 ns ticks since 1601) to an RFC 5322 date in UTC.
fn rfc_date(filetime: u64) -> Option<String> {
    let secs = filetime / 10_000_000;
    let unix = secs.checked_sub(11_644_473_600)?;
    let days = i64::try_from(unix / 86_400).ok()?;
    let rem = unix % 86_400;
    if !(0..=2_932_896).contains(&days) {
        return None;
    }
    let (y, m, d) = civil_from_days(days);
    const WD: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
    const MO: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    Some(format!(
        "{}, {:02} {} {:04} {:02}:{:02}:{:02} +0000",
        WD[(days + 3).rem_euclid(7) as usize],
        d,
        MO[(m - 1).clamp(0, 11) as usize],
        y,
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    ))
}

/// Unfolded headers from the message's original internet headers (names lower-cased).
fn parse_transport(s: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for line in s.lines().take(4000) {
        if line.starts_with([' ', '\t']) {
            if let Some(last) = out.last_mut() {
                last.1.push(' ');
                last.1.push_str(line.trim());
            }
        } else if let Some((n, v)) = line.split_once(':') {
            if !n.is_empty() && n.bytes().all(|b| b.is_ascii_graphic()) {
                out.push((n.to_ascii_lowercase(), v.trim().to_string()));
            }
        } else if line.is_empty() {
            break;
        }
    }
    out
}

fn transport<'a>(h: &'a [(String, String)], name: &str) -> Option<&'a str> {
    h.iter()
        .find(|(n, _)| n == name)
        .map(|(_, v)| v.as_str())
        .filter(|v| !v.is_empty())
}

fn mime_guess(name: &str) -> &'static str {
    let ext = name
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "pdf" => "application/pdf",
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        "tif" | "tiff" => "image/tiff",
        "txt" => "text/plain",
        "htm" | "html" => "text/html",
        "csv" => "text/csv",
        "xml" => "application/xml",
        "zip" => "application/zip",
        "doc" => "application/msword",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "xls" => "application/vnd.ms-excel",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "ppt" => "application/vnd.ms-powerpoint",
        "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        "msg" => "application/vnd.ms-outlook",
        "eml" => "message/rfc822",
        _ => "application/octet-stream",
    }
}

fn valid_mime_type(s: &str) -> bool {
    let Some((a, b)) = s.split_once('/') else {
        return false;
    };
    let ok = |p: &str| {
        !p.is_empty()
            && p.len() < 64
            && p.bytes()
                .all(|c| c.is_ascii_alphanumeric() || b".+-_".contains(&c))
    };
    ok(a) && ok(b)
}

fn safe_filename(raw: &str, fallback: String) -> String {
    let last = raw.rsplit(['/', '\\']).next().unwrap_or("");
    let cleaned: String = last.chars().filter(|c| !c.is_control()).collect();
    let cleaned = cleaned.trim().trim_start_matches('.').to_string();
    let cleaned: String = cleaned.chars().take(150).collect();
    if cleaned.is_empty() {
        fallback
    } else {
        cleaned
    }
}

// ---------------------------------------------------------------- MIME assembly

struct Part {
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Part {
    fn render(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.body.len() + 256);
        for (n, v) in &self.headers {
            out.extend_from_slice(mime::fold_header(n, v).as_bytes());
        }
        out.extend_from_slice(b"\r\n");
        out.extend_from_slice(&self.body);
        if !out.ends_with(b"\r\n") {
            out.extend_from_slice(b"\r\n");
        }
        out
    }
}

struct Ctx {
    counter: u32,
    info: Info,
}

impl Ctx {
    fn boundary(&mut self) -> String {
        self.counter += 1;
        format!(
            "=_oxifsshare_{:04}_{:08x}",
            self.counter,
            0x5eed_u32.wrapping_mul(self.counter + 7)
        )
    }

    fn multipart(&mut self, subtype: &str, parts: Vec<Part>) -> Part {
        let b = self.boundary();
        let mut body = Vec::new();
        for p in parts {
            body.extend_from_slice(format!("--{b}\r\n").as_bytes());
            body.extend_from_slice(&p.render());
        }
        body.extend_from_slice(format!("--{b}--\r\n").as_bytes());
        Part {
            headers: vec![(
                "Content-Type".into(),
                format!("multipart/{subtype}; boundary=\"{b}\""),
            )],
            body,
        }
    }
}

fn text_part(text: &Text, subtype: &str, cp: u32) -> Part {
    let (charset, encoding, body) = match text.clone().resolved() {
        Text::Uni(s) => (
            "utf-8".to_string(),
            "quoted-printable",
            mime::quoted_printable(&s).into_bytes(),
        ),
        Text::Ansi(b, _) => (
            charset_for(cp).to_string(),
            "base64",
            mime::base64_wrapped(&b).into_bytes(),
        ),
    };
    Part {
        headers: vec![
            (
                "Content-Type".into(),
                format!("text/{subtype}; charset={charset}"),
            ),
            ("Content-Transfer-Encoding".into(), encoding.into()),
        ],
        body,
    }
}

fn html_part(bytes: &[u8], charset: &str) -> Part {
    Part {
        headers: vec![
            (
                "Content-Type".into(),
                format!("text/html; charset={charset}"),
            ),
            ("Content-Transfer-Encoding".into(), "base64".into()),
        ],
        body: mime::base64_wrapped(bytes).into_bytes(),
    }
}

// ---------------------------------------------------------------- the compound file

struct Doc {
    cf: cfb::CompoundFile<Cursor<Vec<u8>>>,
    written: usize,
    saw_ansi: bool,
}

impl Doc {
    fn stream(&mut self, path: &str) -> Result<Option<Vec<u8>>, MsgError> {
        let entry = match self.cf.entry(path) {
            Ok(e) => e,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(MsgError::Corrupt),
        };
        if !entry.is_stream() {
            return Ok(None);
        }
        if entry.len() > MAX_STREAM {
            return Err(MsgError::TooLarge);
        }
        let mut s = self.cf.open_stream(path).map_err(|_| MsgError::Corrupt)?;
        let mut v = Vec::with_capacity(entry.len().min(16 * 1024 * 1024) as usize);
        s.read_to_end(&mut v).map_err(|_| MsgError::Corrupt)?;
        Ok(Some(v))
    }

    fn exists_storage(&self, path: &str) -> bool {
        self.cf.is_storage(path)
    }

    fn children(&self, storage: &str, prefix: &str) -> Vec<String> {
        let path = if storage.is_empty() { "/" } else { storage };
        let Ok(rd) = self.cf.read_storage(path) else {
            return Vec::new();
        };
        let mut v: Vec<String> = rd
            .filter(|e| e.is_storage() && e.name().starts_with(prefix))
            .map(|e| e.name().to_string())
            .take(MAX_CHILDREN)
            .collect();
        v.sort();
        v
    }

    fn text(&mut self, base: &str, id: u16, cp: u32) -> Result<Option<Text>, MsgError> {
        if let Some(b) = self.stream(&format!("{base}/__substg1.0_{id:04X}001F"))? {
            return Ok(Some(Text::Uni(utf16le(&b))));
        }
        if let Some(b) = self.stream(&format!("{base}/__substg1.0_{id:04X}001E"))? {
            self.saw_ansi = true;
            return Ok(Some(Text::Ansi(b, cp).resolved()));
        }
        Ok(None)
    }

    fn binary(&mut self, base: &str, id: u16) -> Result<Option<Vec<u8>>, MsgError> {
        self.stream(&format!("{base}/__substg1.0_{id:04X}0102"))
    }

    fn props(&mut self, base: &str, header: usize) -> Result<Option<Props>, MsgError> {
        Ok(self
            .stream(&format!("{base}/__properties_version1.0"))?
            .map(|d| Props::parse(&d, header)))
    }

    /// Converts the message stored at `base` ("" for the file's root).
    fn message(
        &mut self,
        base: &str,
        header: usize,
        depth: u32,
        ctx: &mut Ctx,
    ) -> Result<Vec<u8>, MsgError> {
        let props = match self.props(base, header)? {
            Some(p) => p,
            None if depth == 0 => return Err(MsgError::NotAMessage),
            None => Props(HashMap::new()),
        };
        // 8-bit strings use the message code page (3FFD). Without it, derive one from the
        // message locale (3FF1); the internet code page (3FDE) describes bodies, not
        // other strings, so it is only a last resort and never UTF-8 for generic strings.
        let internet_cp = props.long(0x3FDE);
        let cp = props
            .long(0x3FFD)
            .or_else(|| props.long(0x3FF1).and_then(lcid_codepage))
            .or(internet_cp.filter(|&c| c != 65001))
            .unwrap_or(1252);
        let html_cp = internet_cp.or(props.long(0x3FFD)).unwrap_or(1252);

        let subject = self.text(base, 0x0037, cp)?;
        let body_text = self.text(base, 0x1000, cp)?.filter(|t| !t.is_empty());
        let html_bytes = match self.binary(base, 0x1013)? {
            Some(b) if !b.iter().all(u8::is_ascii_whitespace) => {
                Some((b, charset_for(html_cp).to_string()))
            }
            _ => match self.text(base, 0x1013, cp)? {
                Some(t) if !t.is_empty() => Some((
                    match t.resolved() {
                        Text::Uni(s) => s.into_bytes(),
                        Text::Ansi(b, _) => b,
                    },
                    "utf-8".to_string(),
                )),
                _ => None,
            },
        };
        let has_rtf = self.binary(base, 0x1009)?.is_some_and(|b| !b.is_empty());
        let transport_raw = self.text(base, 0x007D, cp)?;
        let th = transport_raw
            .as_ref()
            .map(|t| parse_transport(&t.lossy()))
            .unwrap_or_default();
        if !th.is_empty() {
            ctx.info.transport_headers = true;
        }
        ctx.info.ansi_strings |= self.saw_ansi;

        // ---- sender
        let sender_name = match self.text(base, 0x0C1A, cp)? {
            Some(t) if !t.is_empty() => Some(t),
            _ => self.text(base, 0x0042, cp)?.filter(|t| !t.is_empty()),
        };
        let mut sender_addr = self
            .text(base, 0x5D01, cp)?
            .map(|t| t.lossy())
            .filter(|a| a.contains('@'));
        if sender_addr.is_none() {
            let atype = self.text(base, 0x0C1E, cp)?.map(|t| t.lossy());
            let addr = self.text(base, 0x0C1F, cp)?.map(|t| t.lossy());
            if let Some(a) = addr {
                if atype
                    .as_deref()
                    .is_some_and(|t| t.eq_ignore_ascii_case("SMTP"))
                    || (a.contains('@') && !a.starts_with("/"))
                {
                    sender_addr = Some(a);
                }
            }
        }
        if sender_addr.is_none() {
            sender_addr = self
                .text(base, 0x5D02, cp)?
                .map(|t| t.lossy())
                .filter(|a| a.contains('@'));
        }

        // ---- recipients
        let mut to: Vec<String> = Vec::new();
        let mut cc: Vec<String> = Vec::new();
        let mut bcc: Vec<String> = Vec::new();
        let (mut to_missing, mut cc_missing) = (false, false);
        for name in self.children(base, "__recip_version1.0_#") {
            let rb = format!("{base}/{name}");
            let rprops = self.props(&rb, 8)?;
            let kind = rprops.as_ref().and_then(|p| p.long(0x0C15)).unwrap_or(1);
            let display = self.text(&rb, 0x3001, cp)?.filter(|t| !t.is_empty());
            let atype = self.text(&rb, 0x3002, cp)?.map(|t| t.lossy());
            let email = self.text(&rb, 0x3003, cp)?.map(|t| t.lossy());
            let smtp = self
                .text(&rb, 0x39FE, cp)?
                .map(|t| t.lossy())
                .filter(|a| a.contains('@'))
                .or_else(|| {
                    email.clone().filter(|a| {
                        atype
                            .as_deref()
                            .is_some_and(|t| t.eq_ignore_ascii_case("SMTP"))
                            || (a.contains('@') && !a.starts_with('/'))
                    })
                });
            ctx.info.recipients += 1;
            if smtp.is_none() {
                ctx.info.recipients_no_smtp += 1;
                match kind {
                    2 => cc_missing = true,
                    3 => {}
                    _ => to_missing = true,
                }
            }
            let (enc, plain) = display.as_ref().map_or((String::new(), true), Text::header);
            let formatted = mime::format_address(Some(&enc), plain, smtp.as_deref().unwrap_or(""));
            match kind {
                2 => cc.push(formatted),
                3 => bcc.push(formatted),
                _ => to.push(formatted),
            }
        }

        // ---- headers
        let mut headers: Vec<(String, String)> = Vec::new();
        let from_value = match (&sender_name, &sender_addr) {
            (Some(n), Some(a)) => {
                let (enc, plain) = n.header();
                Some(mime::format_address(Some(&enc), plain, a))
            }
            (None, Some(a)) => Some(mime::format_address(None, true, a)),
            _ => transport(&th, "from").map(ascii_only).or_else(|| {
                sender_name.as_ref().map(|n| {
                    let (enc, plain) = n.header();
                    mime::format_address(Some(&enc), plain, "")
                })
            }),
        };
        if let Some(v) = from_value {
            headers.push(("From".into(), v));
        }
        let pick = |list: &[String], missing: bool, key: &str| -> Option<String> {
            if missing {
                if let Some(t) = transport(&th, key) {
                    return Some(ascii_only(t));
                }
            }
            (!list.is_empty()).then(|| list.join(", "))
        };
        if let Some(v) = pick(&to, to_missing, "to") {
            headers.push(("To".into(), v));
        }
        if let Some(v) = pick(&cc, cc_missing, "cc") {
            headers.push(("Cc".into(), v));
        }
        if !bcc.is_empty() {
            headers.push(("Bcc".into(), bcc.join(", ")));
        }
        if let Some(r) = transport(&th, "reply-to") {
            headers.push(("Reply-To".into(), ascii_only(r)));
        }
        headers.push((
            "Subject".into(),
            subject.as_ref().map_or(String::new(), |s| s.header().0),
        ));
        let date = props
            .time(0x0039)
            .or_else(|| props.time(0x0E06))
            .and_then(rfc_date)
            .or_else(|| {
                transport(&th, "date")
                    .map(ascii_only)
                    .filter(|d| !d.is_empty())
            });
        match date {
            Some(d) => headers.push(("Date".into(), d)),
            None => ctx.info.no_date = true,
        }
        for (key, header) in [
            (0x1035u16, "Message-ID"),
            (0x1042, "In-Reply-To"),
            (0x1039, "References"),
        ] {
            let v = self
                .text(base, key, cp)?
                .map(|t| ascii_only(&t.lossy()))
                .filter(|v| !v.is_empty());
            let v = v.or_else(|| transport(&th, &header.to_ascii_lowercase()).map(ascii_only));
            if let Some(v) = v.filter(|v| !v.is_empty()) {
                headers.push((header.into(), v));
            }
        }
        headers.push(("MIME-Version".into(), "1.0".into()));

        // ---- attachments
        let mut inline: Vec<Part> = Vec::new();
        let mut regular: Vec<Part> = Vec::new();
        let html_lower = html_bytes
            .as_ref()
            .map(|(b, _)| String::from_utf8_lossy(b).to_ascii_lowercase());
        for (idx, name) in self
            .children(base, "__attach_version1.0_#")
            .into_iter()
            .enumerate()
        {
            let ab = format!("{base}/{name}");
            let aprops = self.props(&ab, 8)?;
            let method = aprops.as_ref().and_then(|p| p.long(0x3705)).unwrap_or(1);
            let long_name = self.text(&ab, 0x3707, cp)?.filter(|t| !t.is_empty());
            let short_name = self.text(&ab, 0x3704, cp)?.filter(|t| !t.is_empty());
            let display = self.text(&ab, 0x3001, cp)?.filter(|t| !t.is_empty());
            let raw_name = long_name
                .or(short_name)
                .or(display)
                .map(|t| t.lossy())
                .unwrap_or_default();
            let cid = self
                .text(&ab, 0x3712, cp)?
                .map(|t| clean_header(&t.lossy()))
                .filter(|c| !c.is_empty());
            let mime_tag = self
                .text(&ab, 0x370E, cp)?
                .map(|t| clean_header(&t.lossy()).to_ascii_lowercase());

            let embedded_path = format!("{ab}/__substg1.0_3701000D");
            let part = if method == 5 && self.exists_storage(&embedded_path) {
                if depth + 1 > MAX_DEPTH {
                    None
                } else {
                    match self.message(&embedded_path, 24, depth + 1, ctx) {
                        Ok(eml) => {
                            ctx.info.embedded_messages += 1;
                            let fname =
                                safe_filename(&raw_name, format!("message-{}.eml", idx + 1));
                            let fname = if fname.to_ascii_lowercase().ends_with(".eml") {
                                fname
                            } else {
                                format!("{fname}.eml")
                            };
                            Some(Part {
                                headers: vec![
                                    (
                                        "Content-Type".into(),
                                        format!(
                                            "message/rfc822; {}",
                                            mime::file_param("name", &fname)
                                        ),
                                    ),
                                    (
                                        "Content-Disposition".into(),
                                        format!(
                                            "attachment; {}",
                                            mime::file_param("filename", &fname)
                                        ),
                                    ),
                                    ("Content-Transfer-Encoding".into(), "7bit".into()),
                                ],
                                body: eml,
                            })
                        }
                        Err(MsgError::TooLarge) => return Err(MsgError::TooLarge),
                        Err(_) => None,
                    }
                }
            } else if method == 1 || method == 0 {
                match self.binary(&ab, 0x3701)? {
                    Some(data) => {
                        let fname = safe_filename(&raw_name, format!("attachment-{}", idx + 1));
                        let ctype = mime_tag
                            .filter(|t| valid_mime_type(t))
                            .unwrap_or_else(|| mime_guess(&fname).to_string());
                        let is_inline = cid.is_some();
                        let disposition = if is_inline { "inline" } else { "attachment" };
                        let mut headers = vec![
                            (
                                "Content-Type".into(),
                                format!("{ctype}; {}", mime::file_param("name", &fname)),
                            ),
                            ("Content-Transfer-Encoding".into(), "base64".into()),
                            (
                                "Content-Disposition".into(),
                                format!("{disposition}; {}", mime::file_param("filename", &fname)),
                            ),
                        ];
                        if let Some(c) = &cid {
                            headers.push((
                                "Content-ID".into(),
                                format!("<{}>", c.trim_matches(['<', '>'])),
                            ));
                        }
                        self.written = self.written.saturating_add(data.len() / 3 * 4);
                        if self.written > MAX_OUTPUT {
                            return Err(MsgError::TooLarge);
                        }
                        Some(Part {
                            headers,
                            body: mime::base64_wrapped(&data).into_bytes(),
                        })
                    }
                    None => None,
                }
            } else {
                None
            };
            match part {
                Some(p) => {
                    ctx.info.attachments += 1;
                    let referenced = cid.as_ref().is_some_and(|c| {
                        html_lower.as_ref().is_some_and(|h| {
                            h.contains(&format!(
                                "cid:{}",
                                c.trim_matches(['<', '>']).to_ascii_lowercase()
                            ))
                        })
                    });
                    if referenced {
                        inline.push(p);
                    } else {
                        regular.push(p);
                    }
                }
                None => ctx.info.skipped_attachments += 1,
            }
        }

        // ---- body structure
        ctx.info.text_body |= body_text.is_some();
        ctx.info.html_body |= html_bytes.is_some();
        let mut note: Option<Text> = None;
        if body_text.is_none() && html_bytes.is_none() && has_rtf {
            ctx.info.rtf_only = true;
            note = Some(Text::Uni(
                "[This message has only a rich-text (RTF) body, which oxifsshare cannot convert yet. \
                 Open the original .msg file to read it.]"
                    .into(),
            ));
        }
        let text_part_opt = body_text
            .as_ref()
            .or(note.as_ref())
            .map(|t| text_part(t, "plain", cp));
        let html_part_opt = html_bytes.as_ref().map(|(b, cs)| html_part(b, cs));
        let html_final = match html_part_opt {
            Some(h) if !inline.is_empty() => {
                let mut parts = vec![h];
                parts.extend(inline);
                Some(ctx.multipart("related", parts))
            }
            other => other,
        };
        let body = match (text_part_opt, html_final) {
            (Some(t), Some(h)) => ctx.multipart("alternative", vec![t, h]),
            (Some(t), None) => t,
            (None, Some(h)) => h,
            (None, None) => text_part(&Text::Uni(String::new()), "plain", cp),
        };
        let top = if regular.is_empty() {
            body
        } else {
            let mut parts = vec![body];
            parts.append(&mut regular);
            ctx.multipart("mixed", parts)
        };
        headers.extend(top.headers.clone());
        let mut eml = Vec::new();
        for (n, v) in &headers {
            eml.extend_from_slice(mime::fold_header(n, v).as_bytes());
        }
        eml.extend_from_slice(b"\r\n");
        eml.extend_from_slice(&top.body);
        self.written = self.written.saturating_add(eml.len());
        if self.written > MAX_OUTPUT {
            return Err(MsgError::TooLarge);
        }
        Ok(eml)
    }
}
