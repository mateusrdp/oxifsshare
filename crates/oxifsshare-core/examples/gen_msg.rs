//! Writes a few synthetic .msg files: `gen_msg OUT_DIR`. All content is invented.
use oxifsshare_core::testmsg::{build_msg, SynthAttachment, SynthMsg, SynthRecipient};
use std::{env, fs, path::Path};

fn main() {
    let Some(dir) = env::args().nth(1) else {
        eprintln!("usage: gen_msg OUT_DIR");
        std::process::exit(2);
    };
    let dir = Path::new(&dir);
    fs::create_dir_all(dir).expect("create dir");
    let r = |k, n: &str, s: &str| SynthRecipient {
        kind: k,
        name: n.into(),
        smtp: Some(s.into()),
        addr_type_and_addr: None,
    };
    let inner = SynthMsg {
        subject: "Inner note".into(),
        body: Some("Inner body".into()),
        sender: Some(("Inner Sender".into(), "inner@example.org".into())),
        recipients: vec![r(1, "Rae", "rae@example.org")],
        filetime: Some(133_100_000_000_000_000),
        ..SynthMsg::default()
    };
    let rich = SynthMsg {
        subject: "Grüße — 日本語 rich".into(),
        body: Some("Plain body ü\r\nline two".into()),
        html: Some("<html><body><p>Hi ü</p><img src=\"cid:p1\"></body></html>".into()),
        sender: Some(("Ann \"A\" Example, Jr.".into(), "ann@example.org".into())),
        recipients: vec![
            r(1, "Bob", "bob@example.org"),
            r(1, "Lee, Ann", "lee@example.org"),
            r(2, "Cy", "cy@example.org"),
        ],
        attachments: vec![
            SynthAttachment {
                name: "report ü.pdf".into(),
                data: (0..=255u8).cycle().take(3000).collect(),
                ..SynthAttachment::default()
            },
            SynthAttachment {
                name: "pic.png".into(),
                data: vec![1; 40],
                cid: Some("p1".into()),
                mime: Some("image/png".into()),
                ..SynthAttachment::default()
            },
            SynthAttachment {
                name: "fwd.msg".into(),
                embedded: Some(Box::new(inner)),
                ..SynthAttachment::default()
            },
        ],
        filetime: Some(133_000_000_000_000_000),
        message_id: Some("<synthetic-1@example.org>".into()),
        ..SynthMsg::default()
    };
    let ansi = SynthMsg {
        subject: "Café".into(),
        body: Some("Caf\u{e9} \u{20ac}5".into()),
        ansi: true,
        sender: Some(("Zoë".into(), "zoe@example.org".into())),
        recipients: vec![r(1, "Yan", "yan@example.org")],
        filetime: Some(133_000_000_000_000_000),
        ..SynthMsg::default()
    };
    for (n, m) in [("synth_rich", rich), ("synth_ansi", ansi)] {
        fs::write(dir.join(format!("{n}.msg")), build_msg(&m)).expect("write");
    }
}
