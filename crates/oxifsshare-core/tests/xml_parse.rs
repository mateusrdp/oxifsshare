//! Index parser tests. All data is synthetic.

use oxifsshare_core::{xml::parse_index, ColumnKind, Error};

fn wrap(rows: &str) -> String {
    format!("<?xml version=\"1.0\" standalone=\"yes\"?>\n<NewDataSet>\n{rows}</NewDataSet>")
}

#[test]
fn decodes_names_and_omits_missing_optionals() {
    let xml = wrap(
        "<documents><Doc_x0020_Type>A</Doc_x0020_Type><Document_x0020_ID>1</Document_x0020_ID></documents>\n\
         <documents><Document_x0020_ID>2</Document_x0020_ID><Notes>n</Notes></documents>",
    );
    let t = parse_index(xml.as_bytes()).unwrap();
    assert_eq!(t.rows.len(), 2);
    let names: Vec<&str> = t.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["Doc Type", "Document ID", "Notes"]);
    assert_eq!(t.rows[0].get(2), None, "omitted element is None");
    assert_eq!(t.rows[1].get(0), None);
    assert_eq!(t.rows[1].get(2), Some("n"));
}

#[test]
fn entities_are_decoded_everywhere_in_a_value() {
    let xml = wrap("<documents><Notes>a &amp; b &lt;c&gt; &quot;d&quot; &apos;e&apos; &#65;&#x42; end</Notes></documents>");
    let t = parse_index(xml.as_bytes()).unwrap();
    assert_eq!(t.rows[0].get(0), Some("a & b <c> \"d\" 'e' AB end"));
}

#[test]
fn entity_at_start_and_end_of_value() {
    let xml = wrap("<documents><Notes>&amp;x&amp;</Notes></documents>");
    assert_eq!(
        parse_index(xml.as_bytes()).unwrap().rows[0].get(0),
        Some("&x&")
    );
}

#[test]
fn unknown_entity_is_kept_visibly_and_counted() {
    let xml = wrap("<documents><Notes>a &nope; b</Notes></documents>");
    let t = parse_index(xml.as_bytes()).unwrap();
    assert_eq!(t.rows[0].get(0), Some("a &nope; b"));
    assert_eq!(t.stats.unknown_entities, 1);
}

#[test]
fn multiline_notes_normalise_line_endings_and_keep_leading_space() {
    // Mixed endings inside one value: CRLF, lone CR, lone LF.
    let xml = wrap("<documents><Notes> lead\r\nsecond\rthird\nfourth</Notes></documents>");
    let t = parse_index(xml.as_bytes()).unwrap();
    assert_eq!(t.rows[0].get(0), Some(" lead\nsecond\nthird\nfourth"));
    assert_eq!(t.stats.crlf, 1);
}

#[test]
fn empty_element_is_empty_string_not_missing() {
    let xml = wrap("<documents><Notes /><Address></Address></documents>");
    let t = parse_index(xml.as_bytes()).unwrap();
    assert_eq!(t.rows[0].get(0), Some(""));
    assert_eq!(t.rows[0].get(1), Some(""));
}

#[test]
fn utf8_bom_is_accepted() {
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(wrap("<documents><Notes>x</Notes></documents>").as_bytes());
    let t = parse_index(&bytes).unwrap();
    assert!(t.stats.bom);
    assert_eq!(t.rows.len(), 1);
}

#[test]
fn utf16_is_accepted() {
    let text = wrap("<documents><Notes>é</Notes></documents>");
    let mut bytes = vec![0xFF, 0xFE];
    for u in text.encode_utf16() {
        bytes.extend_from_slice(&u.to_le_bytes());
    }
    let t = parse_index(&bytes).unwrap();
    assert!(t.stats.utf16);
    assert_eq!(t.rows[0].get(0), Some("é"));
}

#[test]
fn cdata_and_comments() {
    let xml = wrap("<!-- c --><documents><Notes><![CDATA[a <b> & c]]></Notes></documents>");
    assert_eq!(
        parse_index(xml.as_bytes()).unwrap().rows[0].get(0),
        Some("a <b> & c")
    );
}

#[test]
fn schema_gives_order_and_types_and_is_not_a_column() {
    let xml = "<NewDataSet><xs:schema xmlns:xs=\"http://www.w3.org/2001/XMLSchema\" xmlns:msdata=\"urn:schemas-microsoft-com:xml-msdata\">\
        <xs:element name=\"NewDataSet\" msdata:MainDataTable=\"documents\"><xs:complexType><xs:choice><xs:element name=\"documents\"><xs:complexType><xs:sequence>\
        <xs:element name=\"Zeta\" minOccurs=\"0\"><xs:simpleType><xs:restriction base=\"xs:string\"/></xs:simpleType></xs:element>\
        <xs:element name=\"Archive_x0020_Date\" type=\"xs:dateTime\"/><xs:element name=\"Document_x0020_ID\" type=\"xs:int\"/>\
        </xs:sequence></xs:complexType></xs:element></xs:choice></xs:complexType></xs:element></xs:schema>\
        <documents><Document_x0020_ID>5</Document_x0020_ID><Extra>e</Extra><Zeta>z</Zeta></documents></NewDataSet>";
    let t = parse_index(xml.as_bytes()).unwrap();
    let names: Vec<&str> = t.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(
        names,
        ["Zeta", "Archive Date", "Document ID", "Extra"],
        "schema order, extras last"
    );
    assert_eq!(t.columns[1].kind, ColumnKind::DateTime);
    assert_eq!(t.columns[2].kind, ColumnKind::Int);
    assert!(t.stats.schema_found);
    assert_eq!(t.row_element, "documents");
}

#[test]
fn file_without_schema_works_and_infers_kinds() {
    let xml = wrap(
        "<documents><Date>2015-06-06T09:39:48.1234567+10:00</Date><Amount>$1,200.50</Amount><Code>A1</Code></documents>\n\
         <documents><Date>2016-01-01T00:00:00+10:00</Date><Amount>3</Amount><Code>B2</Code></documents>",
    );
    let t = parse_index(xml.as_bytes()).unwrap();
    assert_eq!(t.columns[0].kind, ColumnKind::DateTime);
    assert_eq!(t.columns[1].kind, ColumnKind::Number);
    assert_eq!(t.columns[2].kind, ColumnKind::Text);
}

#[test]
fn malformed_mismatched_tag_reports_line_and_column() {
    let xml = "<NewDataSet>\n<documents>\n  <Notes>x</Wrong>\n</documents>\n</NewDataSet>";
    match parse_index(xml.as_bytes()) {
        Err(Error::Xml { line, column, .. }) => {
            assert_eq!(line, 3);
            assert!(column >= 1);
        }
        other => panic!("expected Xml error, got {other:?}"),
    }
}

#[test]
fn truncated_file_is_an_error_not_a_panic() {
    let full = wrap("<documents><Archive_x0020_Date>2015-06-06T09:39:48.1234567+10:00</Archive_x0020_Date></documents>");
    // Cut in the middle of the timestamp, like a partial copy.
    let cut = full.find("39:48").unwrap() + 6;
    for len in [0, 5, cut, full.len() - 3] {
        let r = parse_index(&full.as_bytes()[..len]);
        assert!(r.is_err(), "len {len} should fail");
    }
    match parse_index(&full.as_bytes()[..cut]) {
        Err(Error::Xml { line, .. }) => assert!(line >= 1),
        other => panic!("{other:?}"),
    }
}

#[test]
fn invalid_utf8_reports_position() {
    let mut bytes = b"<NewDataSet>\n<documents><Notes>".to_vec();
    bytes.push(0xFF);
    bytes.extend_from_slice(b"</Notes></documents></NewDataSet>");
    match parse_index(&bytes) {
        Err(Error::Encoding { line, .. }) => assert_eq!(line, 2),
        other => panic!("{other:?}"),
    }
}

#[test]
fn empty_and_garbage_input_do_not_panic() {
    for input in [
        &b""[..],
        b"   ",
        b"not xml at all",
        b"<",
        b"<a",
        b"\xEF\xBB\xBF",
        b"\xFF\xFE\x00",
    ] {
        let _ = parse_index(input);
    }
}

#[test]
fn empty_dataset_is_ok() {
    let t = parse_index(b"<NewDataSet/>").unwrap();
    assert_eq!(t.rows.len(), 0);
}

#[test]
fn duplicate_fields_and_other_root_children_are_counted() {
    let xml =
        wrap("<documents><Notes>a</Notes><Notes>b</Notes></documents><other><x>1</x></other>");
    let t = parse_index(xml.as_bytes()).unwrap();
    assert_eq!(t.rows.len(), 1);
    assert_eq!(t.rows[0].get(0), Some("b"));
    assert_eq!(t.stats.duplicate_fields, 1);
    assert_eq!(t.stats.other_root_children, 1);
}
