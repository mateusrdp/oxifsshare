//! Synthetic Outlook `.msg` builder for tests. **All content is invented.**
//! Writes a real OLE compound file with the `cfb` crate following the layout the
//! converter reads, so tests need no real or third-party files.

use std::io::{Cursor, Write};

/// One recipient of a synthetic message.
#[derive(Clone, Debug, Default)]
pub struct SynthRecipient {
    /// 1 = To, 2 = Cc, 3 = Bcc.
    pub kind: u32,
    /// Display name.
    pub name: String,
    /// SMTP address (property 39FE), if any.
    pub smtp: Option<String>,
    /// Address type (3002) and address (3003), as an Exchange recipient has.
    pub addr_type_and_addr: Option<(String, String)>,
}

/// One attachment of a synthetic message.
#[derive(Clone, Debug, Default)]
pub struct SynthAttachment {
    /// Long file name (3707).
    pub name: String,
    /// Bytes.
    pub data: Vec<u8>,
    /// Content-ID for inline images (3712).
    pub cid: Option<String>,
    /// MIME type (370E).
    pub mime: Option<String>,
    /// An embedded message (attach method 5); `data` is then ignored.
    pub embedded: Option<Box<SynthMsg>>,
}

/// A synthetic message.
#[derive(Clone, Debug, Default)]
pub struct SynthMsg {
    /// Subject (0037).
    pub subject: String,
    /// Plain-text body (1000).
    pub body: Option<String>,
    /// HTML body (1013), written as binary.
    pub html: Option<String>,
    /// Write a (fake, not decodable) compressed-RTF body only.
    pub rtf_only: bool,
    /// Store strings as 8-bit text in code page 1252 instead of UTF-16.
    pub ansi: bool,
    /// Sender display name and SMTP address.
    pub sender: Option<(String, String)>,
    /// Recipients.
    pub recipients: Vec<SynthRecipient>,
    /// Attachments.
    pub attachments: Vec<SynthAttachment>,
    /// Original internet headers (007D).
    pub transport_headers: Option<String>,
    /// Delivery time as a Windows FILETIME (0039).
    pub filetime: Option<u64>,
    /// Message-ID (1035).
    pub message_id: Option<String>,
}

fn utf16(s: &str) -> Vec<u8> {
    s.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

/// Encodes to cp1252 for the characters the tests use; others become `?`.
fn cp1252(s: &str) -> Vec<u8> {
    s.chars()
        .map(|c| match c {
            '\u{20AC}' => 0x80,
            c if (c as u32) < 0x100 => c as u32 as u8,
            _ => b'?',
        })
        .collect()
}

struct Props(Vec<u8>);

impl Props {
    fn new(header: usize) -> Props {
        Props(vec![0; header])
    }
    fn entry(&mut self, tag: u16, ty: u16, value: [u8; 8]) {
        self.0
            .extend_from_slice(&((u32::from(tag) << 16) | u32::from(ty)).to_le_bytes());
        self.0.extend_from_slice(&6u32.to_le_bytes()); // readable | writable
        self.0.extend_from_slice(&value);
    }
    fn long(&mut self, tag: u16, v: u32) {
        let mut b = [0u8; 8];
        b[..4].copy_from_slice(&v.to_le_bytes());
        self.entry(tag, 0x0003, b);
    }
    fn time(&mut self, tag: u16, v: u64) {
        self.entry(tag, 0x0040, v.to_le_bytes());
    }
    fn sized(&mut self, tag: u16, ty: u16, len: usize) {
        let mut b = [0u8; 8];
        b[..4].copy_from_slice(&(len as u32).to_le_bytes());
        self.entry(tag, ty, b);
    }
}

type Cf = cfb::CompoundFile<Cursor<Vec<u8>>>;

fn put(cf: &mut Cf, path: &str, data: &[u8]) {
    let mut s = cf.create_stream(path).expect("create stream");
    s.write_all(data).expect("write stream");
}

fn put_string(cf: &mut Cf, props: &mut Props, base: &str, id: u16, s: &str, ansi: bool) {
    if ansi {
        let mut b = cp1252(s);
        put(cf, &format!("{base}/__substg1.0_{id:04X}001E"), &b);
        b.push(0);
        props.sized(id, 0x001E, b.len());
    } else {
        let b = utf16(s);
        put(cf, &format!("{base}/__substg1.0_{id:04X}001F"), &b);
        props.sized(id, 0x001F, b.len() + 2);
    }
}

fn put_binary(cf: &mut Cf, props: &mut Props, base: &str, id: u16, data: &[u8]) {
    put(cf, &format!("{base}/__substg1.0_{id:04X}0102"), data);
    props.sized(id, 0x0102, data.len());
}

fn write_message(cf: &mut Cf, base: &str, header: usize, m: &SynthMsg) {
    let mut p = Props::new(header);
    let a = m.ansi;
    if a {
        p.long(0x3FFD, 1252);
    }
    put_string(cf, &mut p, base, 0x001A, "IPM.Note", a);
    put_string(cf, &mut p, base, 0x0037, &m.subject, a);
    if let Some(b) = &m.body {
        put_string(cf, &mut p, base, 0x1000, b, a);
    }
    if let Some(h) = &m.html {
        put_binary(cf, &mut p, base, 0x1013, h.as_bytes());
    }
    if m.rtf_only {
        put_binary(cf, &mut p, base, 0x1009, b"not really compressed rtf");
    }
    if let Some((n, e)) = &m.sender {
        put_string(cf, &mut p, base, 0x0C1A, n, a);
        put_string(cf, &mut p, base, 0x5D01, e, a);
    }
    if let Some(h) = &m.transport_headers {
        put_string(cf, &mut p, base, 0x007D, h, a);
    }
    if let Some(id) = &m.message_id {
        put_string(cf, &mut p, base, 0x1035, id, a);
    }
    if let Some(t) = m.filetime {
        p.time(0x0039, t);
    }
    for (i, r) in m.recipients.iter().enumerate() {
        let rb = format!("{base}/__recip_version1.0_#{i:08X}");
        cf.create_storage(&rb).expect("recipient storage");
        let mut rp = Props::new(8);
        rp.long(0x0C15, r.kind);
        put_string(cf, &mut rp, &rb, 0x3001, &r.name, a);
        if let Some(s) = &r.smtp {
            put_string(cf, &mut rp, &rb, 0x39FE, s, a);
        }
        if let Some((t, e)) = &r.addr_type_and_addr {
            put_string(cf, &mut rp, &rb, 0x3002, t, a);
            put_string(cf, &mut rp, &rb, 0x3003, e, a);
        }
        put(cf, &format!("{rb}/__properties_version1.0"), &rp.0);
    }
    for (i, at) in m.attachments.iter().enumerate() {
        let ab = format!("{base}/__attach_version1.0_#{i:08X}");
        cf.create_storage(&ab).expect("attachment storage");
        let mut ap = Props::new(8);
        put_string(cf, &mut ap, &ab, 0x3707, &at.name, a);
        if let Some(c) = &at.cid {
            put_string(cf, &mut ap, &ab, 0x3712, c, a);
        }
        if let Some(t) = &at.mime {
            put_string(cf, &mut ap, &ab, 0x370E, t, a);
        }
        if let Some(inner) = &at.embedded {
            ap.long(0x3705, 5);
            let eb = format!("{ab}/__substg1.0_3701000D");
            cf.create_storage(&eb).expect("embedded storage");
            write_message(cf, &eb, 24, inner);
        } else {
            ap.long(0x3705, 1);
            put_binary(cf, &mut ap, &ab, 0x3701, &at.data);
        }
        put(cf, &format!("{ab}/__properties_version1.0"), &ap.0);
    }
    // Header counts (top-level 32-byte and embedded 24-byte headers).
    let (r, t) = (m.recipients.len() as u32, m.attachments.len() as u32);
    p.0[8..12].copy_from_slice(&r.to_le_bytes());
    p.0[12..16].copy_from_slice(&t.to_le_bytes());
    p.0[16..20].copy_from_slice(&r.to_le_bytes());
    p.0[20..24].copy_from_slice(&t.to_le_bytes());
    put(cf, &format!("{base}/__properties_version1.0"), &p.0);
}

/// Builds the bytes of a `.msg` file for `m`.
pub fn build_msg(m: &SynthMsg) -> Vec<u8> {
    let mut cf = cfb::CompoundFile::create(Cursor::new(Vec::new())).expect("create compound file");
    // The named-property mapping storage every real message has (empty here).
    cf.create_storage("/__nameid_version1.0").expect("nameid");
    for n in ["00020102", "00030102", "00040102"] {
        put(
            &mut cf,
            &format!("/__nameid_version1.0/__substg1.0_{n}"),
            &[],
        );
    }
    write_message(&mut cf, "", 32, m);
    cf.flush().expect("flush");
    cf.into_inner().into_inner()
}
