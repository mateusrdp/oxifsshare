//! `.msg` to `.eml` conversion tests on synthetic messages (all content invented).

use oxifsshare_core::{
    msg::{convert_bytes, MsgError},
    testmsg::{build_msg, SynthAttachment, SynthMsg, SynthRecipient},
};

fn eml(m: &SynthMsg) -> (String, oxifsshare_core::msg::Info) {
    let c = convert_bytes(build_msg(m)).expect("convert");
    assert!(c.eml.is_ascii(), "output must be 7-bit");
    let s = String::from_utf8(c.eml).unwrap();
    assert!(
        !s.contains("\n") || s.replace("\r\n", "").find('\n').is_none(),
        "bare LF"
    );
    assert!(s.split("\r\n").all(|l| l.len() <= 998));
    (s, c.info)
}

fn rcpt(kind: u32, name: &str, smtp: Option<&str>) -> SynthRecipient {
    SynthRecipient {
        kind,
        name: name.into(),
        smtp: smtp.map(Into::into),
        addr_type_and_addr: None,
    }
}

fn base() -> SynthMsg {
    SynthMsg {
        subject: "Quarterly minutes".into(),
        body: Some("Hello team.\r\nSecond line.".into()),
        sender: Some(("Ann Example".into(), "ann@example.org".into())),
        recipients: vec![
            rcpt(1, "Bob Sample", Some("bob@example.org")),
            rcpt(2, "Cy Fake", Some("cy@example.org")),
            rcpt(3, "Di Made-up", Some("di@example.org")),
        ],
        filetime: Some(133_000_000_000_000_000),
        ..SynthMsg::default()
    }
}

#[test]
fn plain_message_headers() {
    let (s, info) = eml(&base());
    assert!(s.contains("Subject: Quarterly minutes\r\n"));
    assert!(s.contains("From: Ann Example <ann@example.org>\r\n"));
    assert!(s.contains("To: Bob Sample <bob@example.org>\r\n"));
    assert!(s.contains("Cc: Cy Fake <cy@example.org>\r\n"));
    assert!(s.contains("Bcc: Di Made-up <di@example.org>\r\n"));
    assert!(s.contains("Date: "));
    assert!(s.contains("Hello team."));
    assert_eq!(info.recipients, 3);
    assert!(info.text_body && !info.html_body && !info.no_date);
}

#[test]
fn unicode_subject_and_body_are_encoded() {
    let mut m = base();
    m.subject = "Grüße — 日本語".into();
    m.body = Some("Zażółć gęślą jaźń €".into());
    let (s, _) = eml(&m);
    assert!(s.contains("Subject: =?UTF-8?B?"));
    assert!(s.contains("charset=\"utf-8\"") || s.contains("charset=utf-8"));
    assert!(s.contains("Content-Transfer-Encoding: quoted-printable") || s.contains("base64"));
}

#[test]
fn ansi_strings_are_decoded() {
    let mut m = base();
    m.ansi = true;
    m.subject = "Café €".into();
    let (s, info) = eml(&m);
    assert!(info.ansi_strings);
    assert!(s.contains("Subject: =?UTF-8?B?"));
}

#[test]
fn header_injection_is_neutralised() {
    let mut m = base();
    m.subject = "Hi\r\nBcc: attacker@example.org".into();
    m.recipients.clear();
    m.sender = Some((
        "Eve\r\nX-Evil: 1".into(),
        "eve@example.org>\r\nBcc: z@z.org".into(),
    ));
    let (s, _) = eml(&m);
    let head = s.split("\r\n\r\n").next().unwrap();
    assert!(!head
        .lines()
        .any(|l| l.starts_with("Bcc:") || l.starts_with("X-Evil")));
}

#[test]
fn html_and_text_make_alternative() {
    let mut m = base();
    m.html = Some("<html><body><p>Hi</p></body></html>".into());
    let (s, info) = eml(&m);
    assert!(s.contains("multipart/alternative"));
    assert!(s.contains("text/html"));
    assert!(info.html_body && info.text_body);
}

#[test]
fn attachments_and_inline_images() {
    let mut m = base();
    m.html = Some("<img src=\"cid:pic1\">".into());
    m.attachments = vec![
        SynthAttachment {
            name: "report.pdf".into(),
            data: vec![1, 2, 3, 4, 5],
            ..SynthAttachment::default()
        },
        SynthAttachment {
            name: "pic.png".into(),
            data: vec![9; 10],
            cid: Some("pic1".into()),
            mime: Some("image/png".into()),
            ..SynthAttachment::default()
        },
        SynthAttachment {
            name: "../../etc/pass\"wd\r\n.txt".into(),
            data: b"x".to_vec(),
            ..SynthAttachment::default()
        },
    ];
    let (s, info) = eml(&m);
    assert_eq!(info.attachments, 3);
    assert!(s.contains("multipart/mixed"));
    assert!(s.contains("multipart/related"));
    assert!(s.contains("Content-ID: <pic1>"));
    assert!(s.contains("filename=\"report.pdf\""));
    assert!(!s.contains("../"), "path components must be removed");
    assert!(s.contains("AQIDBAU="), "base64 of 01 02 03 04 05");
}

#[test]
fn embedded_message_becomes_rfc822() {
    let mut inner = base();
    inner.subject = "Inner note".into();
    let mut m = base();
    m.attachments = vec![SynthAttachment {
        name: "forwarded.msg".into(),
        embedded: Some(Box::new(inner)),
        ..SynthAttachment::default()
    }];
    let (s, info) = eml(&m);
    assert_eq!(info.embedded_messages, 1);
    assert!(s.contains("message/rfc822"));
    assert!(s.contains("Subject: Inner note"));
}

#[test]
fn recipient_without_smtp_uses_placeholder_and_is_counted() {
    let mut m = base();
    m.recipients = vec![SynthRecipient {
        kind: 1,
        name: "Internal Person".into(),
        smtp: None,
        addr_type_and_addr: Some(("EX".into(), "/O=FAKE/OU=X/CN=RECIPIENTS/CN=PERSON".into())),
    }];
    let (s, info) = eml(&m);
    assert_eq!(info.recipients_no_smtp, 1);
    assert!(s.contains("<unknown@invalid>"));
}

#[test]
fn smtp_typed_address_is_used() {
    let mut m = base();
    m.recipients = vec![SynthRecipient {
        kind: 1,
        name: "Zed".into(),
        smtp: None,
        addr_type_and_addr: Some(("SMTP".into(), "zed@example.org".into())),
    }];
    let (s, info) = eml(&m);
    assert_eq!(info.recipients_no_smtp, 0);
    assert!(s.contains("zed@example.org"));
}

#[test]
fn rtf_only_body_gets_a_notice() {
    let mut m = base();
    m.body = None;
    m.rtf_only = true;
    let (s, info) = eml(&m);
    assert!(info.rtf_only);
    assert!(s.contains("text/plain"));
}

#[test]
fn transport_headers_supply_message_id_and_flag() {
    let mut m = base();
    m.transport_headers = Some("Received: by x\r\nMessage-ID: <abc@example.org>\r\n".into());
    let (s, info) = eml(&m);
    assert!(info.transport_headers);
    assert!(
        s.contains("Message-ID: <abc@example.org>") || s.contains("Message-Id: <abc@example.org>")
    );
}

#[test]
fn missing_date_is_reported() {
    let mut m = base();
    m.filetime = None;
    let (_, info) = eml(&m);
    assert!(info.no_date);
}

#[test]
fn bad_input_gives_typed_errors_not_panics() {
    assert_eq!(
        convert_bytes(Vec::new()).unwrap_err(),
        MsgError::NotCompoundFile
    );
    assert_eq!(
        convert_bytes(b"hello".to_vec()).unwrap_err(),
        MsgError::NotCompoundFile
    );
    // A valid compound file that is not a message.
    let mut cf = cfb::CompoundFile::create(std::io::Cursor::new(Vec::new())).unwrap();
    cf.create_stream("/other").unwrap();
    let bytes = cf.into_inner().into_inner();
    assert_eq!(convert_bytes(bytes).unwrap_err(), MsgError::NotAMessage);
}

#[test]
fn truncated_and_corrupted_files_never_panic() {
    let mut m = base();
    m.html = Some("<p>x</p>".into());
    m.attachments = vec![SynthAttachment {
        name: "a.bin".into(),
        data: vec![7; 5000],
        ..SynthAttachment::default()
    }];
    let good = build_msg(&m);
    for cut in (0..good.len()).step_by(97) {
        let _ = convert_bytes(good[..cut].to_vec());
    }
    let mut seed = 12345u64;
    for _ in 0..300 {
        let mut b = good.clone();
        for _ in 0..8 {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let i = (seed >> 33) as usize % b.len();
            b[i] ^= (seed >> 20) as u8;
        }
        let _ = convert_bytes(b);
    }
}

#[test]
fn error_text_contains_no_content() {
    let e = MsgError::Corrupt.to_string();
    assert!(!e.is_empty() && e.is_ascii());
}
