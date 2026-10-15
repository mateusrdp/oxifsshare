//! Small, dependency-free MIME helpers used by the `.msg` to `.eml` converter:
//! base64, quoted-printable, RFC 2047 encoded words, header folding and
//! address formatting. Everything produced is 7-bit ASCII with CRLF line ends.

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Plain base64 without line breaks.
pub fn base64(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(B64[(n >> 18) as usize & 63] as char);
        out.push(B64[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            B64[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            B64[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

/// Base64 wrapped at 76 columns with CRLF line ends (for a MIME body).
pub fn base64_wrapped(data: &[u8]) -> String {
    let flat = base64(data);
    let mut out = String::with_capacity(flat.len() + flat.len() / 38 + 2);
    for line in flat.as_bytes().chunks(76) {
        // base64 output is ASCII, so this cannot fail.
        out.push_str(std::str::from_utf8(line).unwrap_or_default());
        out.push_str("\r\n");
    }
    out
}

/// Quoted-printable encoding of UTF-8 text. Any of `\r\n`, `\n`, `\r` in the
/// input becomes a hard CRLF line break. Lines are at most 76 characters.
pub fn quoted_printable(text: &str) -> String {
    let normalised = text.replace("\r\n", "\n").replace('\r', "\n");
    let mut out = String::with_capacity(normalised.len() + normalised.len() / 8);
    let lines: Vec<&str> = normalised.split('\n').collect();
    for (li, line) in lines.iter().enumerate() {
        let bytes = line.as_bytes();
        let mut col = 0usize;
        for (i, &b) in bytes.iter().enumerate() {
            let last = i + 1 == bytes.len();
            let literal =
                (b == b' ' || b == b'\t') && !last || (33..=126).contains(&b) && b != b'=';
            let token = if literal {
                (b as char).to_string()
            } else {
                format!("={b:02X}")
            };
            // Leave room for the soft-break "=" unless this is the last byte on the line.
            let limit = if last { 76 } else { 75 };
            if col + token.len() > limit {
                out.push_str("=\r\n");
                col = 0;
            }
            out.push_str(&token);
            col += token.len();
        }
        if li + 1 < lines.len() {
            out.push_str("\r\n");
        }
    }
    if !out.ends_with("\r\n") {
        out.push_str("\r\n");
    }
    out
}

/// Replaces control characters (including CR and LF, which would allow header
/// injection) with spaces and trims the result.
pub fn clean_header(s: &str) -> String {
    let mapped: String = s
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    mapped.trim().to_string()
}

/// Keeps only printable ASCII (other characters become `?`). For values that
/// must stay unencoded, such as addresses copied from raw transport headers.
pub fn ascii_only(s: &str) -> String {
    clean_header(s)
        .chars()
        .map(|c| if c.is_ascii() { c } else { '?' })
        .collect()
}

fn is_plain(s: &str) -> bool {
    s.bytes().all(|b| (0x20..0x7f).contains(&b)) && !s.contains("=?")
}

/// RFC 2047 encoding of `bytes` that are text in `charset`. `boundary_ok`
/// splits the input only where it is safe (each piece must be valid on its own).
fn encoded_words(pieces: &[&[u8]], charset: &str) -> String {
    let words: Vec<String> = pieces
        .iter()
        .map(|p| format!("=?{charset}?B?{}?=", base64(p)))
        .collect();
    words.join(" ")
}

/// Encodes header text. Plain ASCII is returned unchanged (after cleaning);
/// anything else becomes UTF-8 encoded words of at most 75 characters each,
/// separated by spaces (a folding point).
pub fn encode_text(s: &str) -> String {
    let s = clean_header(s);
    if is_plain(&s) {
        return s;
    }
    let mut pieces: Vec<&[u8]> = Vec::new();
    let mut start = 0usize;
    let mut end = 0usize;
    for (i, c) in s.char_indices() {
        let next = i + c.len_utf8();
        if next - start > 45 {
            pieces.push(&s.as_bytes()[start..end]);
            start = i;
        }
        end = next;
    }
    pieces.push(&s.as_bytes()[start..end]);
    encoded_words(&pieces, "UTF-8")
}

/// Like [`encode_text`] but for text that is not Unicode: raw bytes in a named charset.
pub fn encode_raw_text(bytes: &[u8], charset: &str) -> String {
    let cleaned: Vec<u8> = bytes
        .iter()
        .map(|&b| if b < 0x20 || b == 0x7f { b' ' } else { b })
        .collect();
    if cleaned.iter().all(|b| (0x20..0x7f).contains(b)) && !cleaned.windows(2).any(|w| w == b"=?") {
        return String::from_utf8_lossy(&cleaned).trim().to_string();
    }
    // Raw bytes may be multi-byte in the charset, so only split at safe sizes for single-byte
    // charsets; for others keep one word per 45 bytes (acceptable for the short strings seen here).
    let pieces: Vec<&[u8]> = cleaned.chunks(45).collect();
    encoded_words(&pieces, charset)
}

/// `Name <address>` with the name quoted or encoded as needed.
pub fn format_address(name_encoded: Option<&str>, name_is_plain: bool, addr: &str) -> String {
    let addr = clean_addr(addr);
    match name_encoded.filter(|n| !n.is_empty()) {
        None => addr,
        Some(n) if !name_is_plain => format!("{n} <{addr}>"),
        Some(n) => {
            let needs_quote = n.chars().any(|c| "()<>[]:;@\\,.\"".contains(c));
            if needs_quote {
                let escaped = n.replace('\\', "\\\\").replace('"', "\\\"");
                format!("\"{escaped}\" <{addr}>")
            } else {
                format!("{n} <{addr}>")
            }
        }
    }
}

/// Strips everything that cannot be in an address and requires an `@`.
/// Unusable input becomes `unknown@invalid` (`.invalid` is reserved by RFC 6761).
pub fn clean_addr(addr: &str) -> String {
    let a: String = addr
        .chars()
        .filter(|c| c.is_ascii_graphic() && !"<>\"\\(),;:[]".contains(*c))
        .collect();
    if a.contains('@') && !a.starts_with('@') && !a.ends_with('@') {
        a
    } else {
        "unknown@invalid".to_string()
    }
}

/// `Name: value` folded at spaces so no line exceeds 76 characters, ending with CRLF.
pub fn fold_header(name: &str, value: &str) -> String {
    let mut out = String::new();
    let mut line = format!("{name}:");
    for word in value.split(' ').filter(|w| !w.is_empty()) {
        if line.len() + 1 + word.len() > 76 && line.len() > name.len() + 1 {
            out.push_str(&line);
            out.push_str("\r\n");
            line = String::from(" ");
            line.push_str(word);
        } else {
            line.push(' ');
            line.push_str(word);
        }
    }
    out.push_str(&line);
    out.push_str("\r\n");
    out
}

/// Percent-encodes for RFC 2231 `filename*=UTF-8''...` (unreserved characters stay).
pub fn percent_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Parameter text for `name` and `filename`: `"plain.txt"` or an ASCII fallback
/// plus an RFC 2231 extended form for anything else.
pub fn file_param(key: &str, filename: &str) -> String {
    let safe: String = filename
        .chars()
        .map(|c| {
            if c.is_ascii_graphic() || c == ' ' {
                c
            } else {
                '_'
            }
        })
        .map(|c| if c == '"' || c == '\\' { '_' } else { c })
        .collect();
    if safe == filename {
        format!("{key}=\"{safe}\"")
    } else {
        format!(
            "{key}=\"{safe}\"; {key}*=UTF-8''{}",
            percent_encode(filename)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_known_vectors() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn base64_wraps_at_76() {
        let w = base64_wrapped(&[0u8; 200]);
        assert!(w.lines().all(|l| l.len() <= 76));
        assert!(w.ends_with("\r\n"));
    }

    #[test]
    fn qp_basics() {
        assert_eq!(quoted_printable("abc"), "abc\r\n");
        assert_eq!(quoted_printable("a=b"), "a=3Db\r\n");
        assert_eq!(quoted_printable("é"), "=C3=A9\r\n");
        assert_eq!(quoted_printable("a\nb\r\nc"), "a\r\nb\r\nc\r\n");
        assert_eq!(quoted_printable("end "), "end=20\r\n");
        assert_eq!(quoted_printable("a b"), "a b\r\n");
    }

    #[test]
    fn qp_lines_stay_short() {
        let long = "x".repeat(500) + &"é".repeat(100);
        let q = quoted_printable(&long);
        assert!(q.split("\r\n").all(|l| l.len() <= 76), "{q}");
    }

    #[test]
    fn header_text_encoding() {
        assert_eq!(encode_text("Hello world"), "Hello world");
        assert_eq!(encode_text("a\r\nBcc: x"), "a  Bcc: x");
        let e = encode_text("Grüße");
        assert!(e.starts_with("=?UTF-8?B?") && e.is_ascii());
        let long = encode_text(&"ü".repeat(100));
        assert!(long.split(' ').all(|w| w.len() <= 75), "{long}");
        assert!(!is_plain("=?x?"));
    }

    #[test]
    fn address_formatting() {
        assert_eq!(
            format_address(Some("Ann Lee"), true, "ann@x.org"),
            "Ann Lee <ann@x.org>"
        );
        assert_eq!(
            format_address(Some("Lee, Ann"), true, "ann@x.org"),
            "\"Lee, Ann\" <ann@x.org>"
        );
        assert_eq!(format_address(None, true, "ann@x.org"), "ann@x.org");
        assert_eq!(
            format_address(Some("x"), true, "no address"),
            "x <unknown@invalid>"
        );
        assert_eq!(
            clean_addr("a@b>\r\nBcc: z@z"),
            "a@bBcc:z@z".replace(':', "")
        );
    }

    #[test]
    fn folding_respects_limit() {
        let v = (0..40)
            .map(|i| format!("person{i}@example.org"))
            .collect::<Vec<_>>()
            .join(", ");
        let f = fold_header("To", &v);
        assert!(f.split("\r\n").all(|l| l.len() <= 76));
        assert!(f.lines().skip(1).all(|l| l.starts_with(' ')));
        let unfolded: String = f.replace("\r\n ", " ");
        assert!(unfolded.starts_with("To: person0@example.org,"));
    }

    #[test]
    fn filename_params() {
        assert_eq!(file_param("filename", "a b.pdf"), "filename=\"a b.pdf\"");
        let p = file_param("filename", "bär.pdf");
        assert!(p.contains("filename*=UTF-8''b%C3%A4r.pdf") && p.is_ascii());
        assert_eq!(
            file_param("filename", "a\"b"),
            "filename=\"a_b\"; filename*=UTF-8''a%22b"
        );
    }
}
