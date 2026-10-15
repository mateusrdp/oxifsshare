//! The --check report must never leak values or file names. Synthetic data only.

use oxifsshare_core::{
    check,
    testgen::{synth_archive, Spec, Variant},
};

const CANARY: &str = "ZQXCANARYVALUE";

#[test]
fn report_contains_no_values_or_file_names() {
    for (variant, parent) in [
        (Variant::Plain, false),
        (Variant::Financial, true),
        (Variant::Property, false),
    ] {
        let spec = Spec {
            rows: 120,
            variant,
            canary: CANARY.into(),
            id_start: 7_340_000,
            missing_every: 9,
            orphans: 4,
            txt_file: true,
            duplicates: 3,
            pass_parent: parent,
            ..Spec::default()
        };
        let fx = synth_archive(&spec).unwrap();
        for show_ids in [false, true] {
            let report = check::run(&fx.root, show_ids).unwrap();
            assert!(!report.contains(CANARY), "canary leaked:\n{report}");
            assert!(!report.contains("Example Street"), "address text leaked");
            assert!(!report.contains("Person "), "name text leaked");
            assert!(!report.contains("notes-synthetic"), "file name leaked");
            assert!(!report.contains("fs-share.xml"), "file name leaked");
            assert!(
                !report.contains(fx.dir.path().to_string_lossy().as_ref()),
                "path leaked"
            );
            assert!(!report.contains("2012-"), "date value leaked");
            let has_ids = report.contains("734");
            assert_eq!(has_ids, show_ids, "ids appear only with --show-ids");
            assert!(report.contains("rows: 120"));
            assert!(report.contains("rows without a file:"));
        }
    }
}

#[test]
fn show_ids_lists_exactly_the_unmatched_ids() {
    let spec = Spec {
        rows: 30,
        missing_every: 10,
        id_start: 5_000_000,
        ..Spec::default()
    };
    let fx = synth_archive(&spec).unwrap();
    let report = check::run(&fx.root, true).unwrap();
    let line = report
        .lines()
        .find(|l| l.contains("rows without a file:") && l.contains("500"))
        .unwrap();
    for id in &fx.expected.missing_ids {
        assert!(line.contains(&id.to_string()), "{id} missing from {line}");
    }
}

#[test]
fn report_for_auto_detected_key_lists_every_column_rate() {
    let d = oxifsshare_core::testgen::TempDir::new("chk").unwrap();
    std::fs::write(
        d.path().join("fs-share.xml"),
        "<NewDataSet><documents><Ref>77</Ref><Document_x0020_ID>1</Document_x0020_ID></documents></NewDataSet>",
    )
    .unwrap();
    std::fs::write(d.path().join("77.pdf"), b"x").unwrap();
    let r = check::run(d.path(), false).unwrap();
    assert!(r.contains("auto-detected"));
    assert!(r.contains("Ref: 100.0%"));
}

#[test]
fn errors_do_not_contain_values() {
    let d = oxifsshare_core::testgen::TempDir::new("chk2").unwrap();
    std::fs::write(
        d.path().join("fs-share.xml"),
        format!("<NewDataSet><documents><A>{CANARY}</B></documents>"),
    )
    .unwrap();
    let e = check::run(d.path(), false).unwrap_err().to_string();
    assert!(!e.contains(CANARY), "{e}");
    assert!(e.contains("line 1"));
}

#[test]
fn messages_section_reports_counts_only() {
    let spec = Spec {
        rows: 100,
        variant: Variant::Financial,
        id_start: 410_000,
        ..Spec::default()
    };
    let fx = synth_archive(&spec).unwrap();
    // Break one message so a failure category shows up.
    let broken = std::fs::read_dir(&fx.root)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .find(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("msg")))
        .expect("fixture has a .msg");
    std::fs::write(&broken, b"garbage").unwrap();
    let report = check::run(&fx.root, false).unwrap();
    assert!(report.contains("[Messages]"), "{report}");
    assert!(report.contains("not-a-compound-file 1"), "{report}");
    assert!(report.contains("with plain-text body: "));
    for leaked in [
        "Synthetic message",
        "Synthetic body",
        "example.org",
        "Sender Example",
    ] {
        assert!(!report.contains(leaked), "message content leaked: {leaked}");
    }
}
