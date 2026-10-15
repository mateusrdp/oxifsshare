//! Folder indexing, key detection, matching, search, sort, open policy. Synthetic data only.

use std::fs;

use oxifsshare_core::{
    testgen::{synth_archive, Spec, Variant},
    Archive, ColumnKind, FileKind, FileState, Filter, OpenError, Sort,
};

fn write(dir: &std::path::Path, name: &str) {
    fs::write(dir.join(name), b"x").unwrap();
}

fn index_xml(ids: &[&str], extra_col: Option<(&str, &[&str])>) -> String {
    let mut s = String::from("<NewDataSet>");
    for (i, id) in ids.iter().enumerate() {
        s.push_str("<documents>");
        if let Some((name, vals)) = extra_col {
            s.push_str(&format!("<{name}>{}</{name}>", vals[i]));
        }
        s.push_str(&format!(
            "<Document_x0020_ID>{id}</Document_x0020_ID></documents>"
        ));
    }
    s.push_str("</NewDataSet>");
    s
}

fn tmp(tag: &str) -> oxifsshare_core::testgen::TempDir {
    oxifsshare_core::testgen::TempDir::new(tag).unwrap()
}

#[test]
fn generated_archive_matches_expectations() {
    let spec = Spec {
        rows: 200,
        missing_every: 10,
        orphans: 3,
        txt_file: true,
        duplicates: 5,
        ..Spec::default()
    };
    let fx = synth_archive(&spec).unwrap();
    let a = Archive::open(&fx.root).unwrap();
    let st = a.stats();
    assert_eq!(st.entries, 200);
    assert_eq!(st.matched, fx.expected.with_file);
    assert_eq!(st.missing, fx.expected.missing);
    assert_eq!(st.orphans, fx.expected.orphans);
    assert_eq!(st.ambiguous, fx.expected.ambiguous);
    assert_eq!(st.shadowed, 5, "the losing duplicate of each ambiguous row");
    assert_eq!(a.table.columns[a.key.chosen.unwrap()].name, "Document ID");
    assert!(
        a.key.candidates.is_empty(),
        "default key was good enough, nothing else tested"
    );
}

#[test]
fn both_root_forms_work() {
    for pass_parent in [false, true] {
        let fx = synth_archive(&Spec {
            rows: 10,
            pass_parent,
            ..Spec::default()
        })
        .unwrap();
        let a = Archive::open(&fx.root).unwrap();
        assert_eq!(a.stats().matched, 10);
        assert_eq!(a.location.via_subfolder, pass_parent);
    }
    // Passing the XML file itself also works.
    let fx = synth_archive(&Spec {
        rows: 4,
        ..Spec::default()
    })
    .unwrap();
    assert_eq!(Archive::open(&fx.index_path).unwrap().stats().matched, 4);
}

#[test]
fn subfolder_and_index_names_are_case_insensitive() {
    let d = tmp("case");
    let sub = d.path().join("FsShare");
    fs::create_dir_all(&sub).unwrap();
    fs::write(sub.join("FS-Share.XML"), index_xml(&["1"], None)).unwrap();
    write(&sub, "1.pdf");
    let a = Archive::open(d.path()).unwrap();
    assert_eq!(a.stats().matched, 1);
}

#[test]
fn case_insensitive_and_zero_padded_matching_both_ways() {
    let d = tmp("pad");
    // IDs unpadded, files padded and upper-case; and the reverse.
    fs::write(
        d.path().join("fs-share.xml"),
        index_xml(&["123", "000456", "789"], None),
    )
    .unwrap();
    write(d.path(), "000123.PDF");
    write(d.path(), "456.Tif");
    write(d.path(), "0789.msg");
    let a = Archive::open(d.path()).unwrap();
    assert_eq!(a.stats().matched, 3);
    assert_eq!(a.row_kind(0), FileKind::Pdf);
    assert_eq!(a.row_kind(1), FileKind::Tiff);
    assert_eq!(a.row_kind(2), FileKind::Msg);
    assert_eq!(a.stats().orphans, 0);
}

#[test]
fn zero_is_not_confused_with_empty() {
    let d = tmp("zero");
    fs::write(d.path().join("fs-share.xml"), index_xml(&["0", ""], None)).unwrap();
    write(d.path(), "000.pdf");
    let a = Archive::open(d.path()).unwrap();
    assert_eq!(a.stats().matched, 1);
    assert_eq!(a.stats().empty_key, 1);
}

#[test]
fn duplicates_choose_deterministically_by_extension_then_name() {
    let d = tmp("dup");
    fs::write(d.path().join("fs-share.xml"), index_xml(&["5", "6"], None)).unwrap();
    // 5: tif vs pdf -> pdf wins whatever the creation order.
    write(d.path(), "5.tif");
    write(d.path(), "5.pdf");
    // 6: same extension rank, differing case and padding -> lowest name wins.
    write(d.path(), "06.pdf");
    write(d.path(), "006.pdf");
    let a = Archive::open(d.path()).unwrap();
    assert_eq!(a.row_file(0).unwrap().name, "5.pdf");
    assert_eq!(a.row_file(1).unwrap().name, "006.pdf");
    assert_eq!(a.stats().ambiguous, 2);
    assert_eq!(a.resolution(0).candidates, 2);
    assert_eq!(
        a.file_state(a.resolution(0).file.unwrap()),
        FileState::Matched
    );
    assert_eq!(a.stats().shadowed, 2);
}

#[test]
fn orphans_subfolders_hidden_and_extensionless_files() {
    let d = tmp("orph");
    fs::write(d.path().join("fs-share.xml"), index_xml(&["1"], None)).unwrap();
    write(d.path(), "1.pdf");
    write(d.path(), "2.pdf");
    write(d.path(), "readme.txt");
    write(d.path(), "1"); // no extension: cannot match
    write(d.path(), ".hidden.pdf");
    fs::create_dir(d.path().join("sub")).unwrap();
    write(&d.path().join("sub"), "1.pdf");
    let a = Archive::open(d.path()).unwrap();
    assert_eq!(a.stats().matched, 1);
    assert_eq!(a.stats().orphans, 2);
    assert_eq!(a.folder.subdirs, 1);
    assert_eq!(a.folder.no_extension, 1);
    assert_eq!(a.folder.hidden, 1);
    assert_eq!(a.folder.files.len(), 3, "the index file is not a document");
}

#[test]
fn key_detection_falls_back_to_the_best_column() {
    let d = tmp("key");
    // "Document ID" values do not match any file; "Plan Number" does.
    let xml = "<NewDataSet>".to_string()
        + &(0..10)
            .map(|i| format!("<documents><Plan_x0020_Number>{}</Plan_x0020_Number><Document_x0020_ID>9{i}</Document_x0020_ID></documents>", 500 + i))
            .collect::<String>()
        + "</NewDataSet>";
    fs::write(d.path().join("fs-share.xml"), xml).unwrap();
    for i in 0..10 {
        write(d.path(), &format!("{}.pdf", 500 + i));
    }
    let a = Archive::open(d.path()).unwrap();
    assert_eq!(a.key.default_rate, 0.0);
    assert_eq!(a.table.columns[a.key.chosen.unwrap()].name, "Plan Number");
    assert_eq!(a.key.chosen_rate, 1.0);
    assert_eq!(a.key.candidates.len(), 2, "every column tested");
    assert_eq!(a.stats().matched, 10);
}

#[test]
fn good_default_key_is_kept_even_if_another_column_is_better() {
    let d = tmp("key2");
    let ids: Vec<String> = (0..10).map(|i| format!("{}", 100 + i)).collect();
    let refs: Vec<&str> = ids.iter().map(String::as_str).collect();
    fs::write(
        d.path().join("fs-share.xml"),
        index_xml(&refs, Some(("Other", &refs))),
    )
    .unwrap();
    for id in &ids[..6] {
        write(d.path(), &format!("{id}.pdf"));
    }
    let a = Archive::open(d.path()).unwrap();
    assert_eq!(a.key.default_rate, 0.6);
    assert_eq!(a.table.columns[a.key.chosen.unwrap()].name, "Document ID");
}

#[test]
fn missing_index_and_missing_folder_are_clear_errors() {
    let d = tmp("none");
    assert!(matches!(
        Archive::open(d.path()),
        Err(oxifsshare_core::Error::IndexNotFound { .. })
    ));
    assert!(matches!(
        Archive::open(&d.path().join("nope")),
        Err(oxifsshare_core::Error::Io { .. })
    ));
}

#[test]
fn malformed_index_reports_position_through_archive_open() {
    let d = tmp("bad");
    fs::write(
        d.path().join("fs-share.xml"),
        "<NewDataSet>\n<documents>\n<A>x</B>",
    )
    .unwrap();
    match Archive::open(d.path()) {
        Err(oxifsshare_core::Error::Xml { line, .. }) => assert_eq!(line, 3),
        other => panic!("{other:?}"),
    }
}

#[test]
fn three_schema_variants_load_and_sort_sensibly() {
    for variant in [Variant::Plain, Variant::Financial, Variant::Property] {
        let fx = synth_archive(&Spec {
            rows: 60,
            variant,
            seed: 7,
            ..Spec::default()
        })
        .unwrap();
        let a = Archive::open(&fx.root).unwrap();
        assert_eq!(a.stats().matched, 60);
        let date = a.table.find_column("Date").unwrap();
        assert_eq!(a.table.columns[date].kind, ColumnKind::DateTime);
        let mut rows = a.query(
            &Filter::default(),
            Some(Sort {
                column: date,
                descending: false,
            }),
        );
        assert_eq!(rows.len(), 60);
        let times: Vec<i64> = rows
            .iter()
            .filter_map(|&r| {
                a.table.rows[r]
                    .get(date)
                    .and_then(oxifsshare_core::date::parse_datetime)
            })
            .collect();
        assert!(times.windows(2).all(|w| w[0] <= w[1]), "chronological");
        rows.reverse();
        if variant == Variant::Financial {
            let amt = a.table.find_column("Invoice Amount").unwrap();
            assert_eq!(a.table.columns[amt].kind, ColumnKind::Number);
        }
    }
}

#[test]
fn document_id_sorts_numerically_not_alphabetically() {
    let d = tmp("sort");
    fs::write(
        d.path().join("fs-share.xml"),
        index_xml(&["9", "100", "20"], None),
    )
    .unwrap();
    let a = Archive::open(d.path()).unwrap();
    let col = a.key.chosen.unwrap();
    assert_eq!(
        a.table.columns[col].kind,
        ColumnKind::Number,
        "inferred without a schema"
    );
    let asc = a.query(
        &Filter::default(),
        Some(Sort {
            column: col,
            descending: false,
        }),
    );
    assert_eq!(asc, [0, 2, 1]);
    let desc = a.query(
        &Filter::default(),
        Some(Sort {
            column: col,
            descending: true,
        }),
    );
    assert_eq!(desc, [1, 2, 0]);
}

#[test]
fn search_is_case_insensitive_and_terms_are_anded() {
    let d = tmp("search");
    let xml = "<NewDataSet>\
        <documents><Address>12 Alpha Street</Address><Manager>Smith</Manager><Document_x0020_ID>1</Document_x0020_ID></documents>\
        <documents><Address>7 Beta Road</Address><Manager>smith</Manager><Document_x0020_ID>2</Document_x0020_ID></documents>\
        <documents><Address>9 Alpha Road</Address><Manager>Jones</Manager><Document_x0020_ID>3</Document_x0020_ID></documents>\
        </NewDataSet>";
    fs::write(d.path().join("fs-share.xml"), xml).unwrap();
    let a = Archive::open(d.path()).unwrap();
    let q = |t: &str| {
        a.query(
            &Filter {
                text: t.into(),
                ..Filter::default()
            },
            None,
        )
    };
    assert_eq!(q("ALPHA"), [0, 2]);
    assert_eq!(q("alpha smith"), [0]);
    assert_eq!(q("smith   road"), [1]);
    assert_eq!(q("alpha beta"), Vec::<usize>::new());
    assert_eq!(q("  "), [0, 1, 2]);
}

#[test]
fn filters_for_type_kind_and_date_range() {
    let fx = synth_archive(&Spec {
        rows: 300,
        missing_every: 7,
        seed: 3,
        ..Spec::default()
    })
    .unwrap();
    let a = Archive::open(&fx.root).unwrap();
    let all = a.query(&Filter::default(), None).len();
    assert_eq!(all, 300);

    let by_type = a.query(
        &Filter {
            doc_type: Some("invoice".into()),
            ..Filter::default()
        },
        None,
    );
    assert!(!by_type.is_empty() && by_type.len() < all);
    let tcol = a.table.find_column("Doc Type").unwrap();
    assert!(by_type
        .iter()
        .all(|&r| a.table.rows[r].get(tcol) == Some("Invoice")));

    let missing = a.query(
        &Filter {
            kinds: vec![FileKind::Missing],
            ..Filter::default()
        },
        None,
    );
    assert_eq!(missing.len(), fx.expected.missing);
    let docs = a.query(
        &Filter {
            kinds: vec![FileKind::Pdf, FileKind::Tiff],
            ..Filter::default()
        },
        None,
    );
    assert!(docs
        .iter()
        .all(|&r| matches!(a.row_kind(r), FileKind::Pdf | FileKind::Tiff)));

    let from = oxifsshare_core::date::parse_day("2012-01-01").unwrap();
    let to = oxifsshare_core::date::parse_day("2012-12-31").unwrap();
    let ranged = a.query(
        &Filter {
            date_from: Some(from),
            date_to: Some(to),
            ..Filter::default()
        },
        None,
    );
    assert!(!ranged.is_empty() && ranged.len() < all);
    let dcol = a.table.find_column("Date").unwrap();
    assert!(ranged
        .iter()
        .all(|&r| a.table.rows[r].get(dcol).unwrap().starts_with("2012-")));
}

#[test]
fn open_policy_allows_documents_and_blocks_escapes_and_odd_types() {
    let d = tmp("open");
    let outside = tmp("outside");
    fs::write(outside.path().join("secret.pdf"), b"x").unwrap();
    fs::write(
        d.path().join("fs-share.xml"),
        index_xml(&["1", "2", "3", "4"], None),
    )
    .unwrap();
    write(d.path(), "1.pdf");
    write(d.path(), "2.sh");
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(outside.path().join("secret.pdf"), d.path().join("3.pdf"))
            .unwrap();
    }
    let a = Archive::open(d.path()).unwrap();
    assert!(a.open_target(0, false).is_ok());
    assert!(
        matches!(a.open_target(1, false), Err(OpenError::NeedsConfirmation(ref e)) if e == "sh")
    );
    assert!(a.open_target(1, true).is_ok());
    // Symlinks are not indexed at all, so a symlinked "document" is simply not found.
    assert!(matches!(a.open_target(2, true), Err(OpenError::NoFile)));
    assert!(matches!(a.open_target(3, true), Err(OpenError::NoFile)));
    // Direct policy check on an escaping path.
    #[cfg(unix)]
    assert!(matches!(
        oxifsshare_core::checked_path(d.path(), &d.path().join("3.pdf"), true),
        Err(OpenError::OutsideArchive)
    ));
}

/// Run with: cargo test --release -p oxifsshare-core -- --ignored --nocapture
#[test]
#[ignore = "slow in debug builds; run in release mode"]
fn hundred_thousand_rows_timing() {
    use std::time::Instant;
    let spec = Spec {
        rows: 100_000,
        variant: Variant::Financial,
        ..Spec::default()
    };
    let t0 = Instant::now();
    let fx = synth_archive(&spec).unwrap();
    let t1 = Instant::now();
    let a = Archive::open(&fx.root).unwrap();
    let t2 = Instant::now();
    assert_eq!(a.table.rows.len(), 100_000);
    assert_eq!(a.stats().matched, 100_000);
    let col = a.table.find_column("Date").unwrap();
    let f = Filter {
        text: "invoice person".into(),
        ..Filter::default()
    };
    let rows = a.query(
        &f,
        Some(Sort {
            column: col,
            descending: true,
        }),
    );
    let t3 = Instant::now();
    assert!(!rows.is_empty());
    let rows2 = a.query(&Filter::default(), None);
    let t4 = Instant::now();
    assert_eq!(rows2.len(), 100_000);
    eprintln!(
        "generate {:?} | open (parse+index+match) {:?} | search+sort {:?} | unfiltered query {:?}",
        t1 - t0,
        t2 - t1,
        t3 - t2,
        t4 - t3
    );
}
