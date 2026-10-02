use wasmppt_xml::{TokenKind, XmlDocument, XmlErrorCode, XmlLimits};

#[test]
fn resolves_namespaces_and_retains_exact_source_ranges() {
    let source = br#"<?xml version="1.0"?><p:sld xmlns:p="urn:p" xmlns:a="urn:a" xmlns:mc="urn:mc"><mc:AlternateContent><a:t xml:space="preserve">A &amp; B</a:t></mc:AlternateContent><p:extLst><p:ext uri="future"/></p:extLst></p:sld>"#;
    let document = XmlDocument::parse(source.as_slice()).unwrap();
    assert_eq!(document.source(), source);
    let starts = document
        .tokens()
        .iter()
        .filter_map(|token| match &token.kind {
            TokenKind::Start {
                name, attributes, ..
            } => Some((token, name, attributes)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(starts[0].1.local, "sld");
    assert_eq!(document.namespace(starts[0].1.namespace.unwrap()), "urn:p");
    assert_eq!(starts[2].1.local, "t");
    assert_eq!(document.namespace(starts[2].1.namespace.unwrap()), "urn:a");
    assert_eq!(
        document.source_range(starts[3].0.range.clone()),
        b"<p:extLst>"
    );
}

#[test]
fn rejects_dtd_and_mismatched_markup_with_stable_codes() {
    let dtd = XmlDocument::parse(b"<!DOCTYPE x><x/>".as_slice()).unwrap_err();
    assert_eq!(dtd.code(), XmlErrorCode::DtdForbidden);
    let mismatch = XmlDocument::parse(b"<x><y></x>".as_slice()).unwrap_err();
    assert_eq!(mismatch.code(), XmlErrorCode::MismatchedEndTag);
}

#[test]
fn security_limits_fail_with_one_stable_code() {
    let base = XmlLimits::default();
    for (source, limits) in [
        (
            b"<root/>".as_slice(),
            XmlLimits {
                max_source_bytes: 3,
                ..base
            },
        ),
        (
            b"<a><b><c/></b></a>".as_slice(),
            XmlLimits {
                max_depth: 2,
                ..base
            },
        ),
        (
            b"<a x=\"1\" y=\"2\"/>".as_slice(),
            XmlLimits {
                max_attributes_per_element: 1,
                ..base
            },
        ),
        (
            b"<a><b/></a>".as_slice(),
            XmlLimits {
                max_tokens: 1,
                ..base
            },
        ),
    ] {
        assert_eq!(
            XmlDocument::parse_with_limits(source, limits)
                .unwrap_err()
                .code(),
            XmlErrorCode::LimitExceeded
        );
    }
}

#[test]
fn nested_and_empty_namespace_declarations_restore_parent_bindings() {
    let source = br#"<r xmlns="urn:root" xmlns:p="urn:outer"><p:a><b xmlns="urn:inner" xmlns:p="urn:shadow" p:id="1" id="2"><p:c/></b><p:d/><e xmlns:p="urn:empty"><p:f/></e><p:g xmlns:p="urn:leaf"/><p:h/></p:a></r>"#;
    let document = XmlDocument::parse(source.as_slice()).unwrap();
    let elements = document
        .tokens()
        .iter()
        .filter_map(|token| {
            let name = match &token.kind {
                TokenKind::Start { name, .. } | TokenKind::End { name } => name,
                _ => return None,
            };
            Some((
                name.local.as_str(),
                name.namespace.map(|symbol| document.namespace(symbol)),
            ))
        })
        .collect::<Vec<_>>();
    assert_eq!(
        elements,
        [
            ("r", Some("urn:root")),
            ("a", Some("urn:outer")),
            ("b", Some("urn:inner")),
            ("c", Some("urn:shadow")),
            ("b", Some("urn:inner")),
            ("d", Some("urn:outer")),
            ("e", Some("urn:root")),
            ("f", Some("urn:empty")),
            ("e", Some("urn:root")),
            ("g", Some("urn:leaf")),
            ("h", Some("urn:outer")),
            ("a", Some("urn:outer")),
            ("r", Some("urn:root")),
        ]
    );
    let attributes = document
        .tokens()
        .iter()
        .find_map(|token| match &token.kind {
            TokenKind::Start {
                name, attributes, ..
            } if name.local == "b" => Some(attributes),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        document
            .attribute(attributes, Some("urn:shadow"), "id")
            .unwrap()
            .value,
        "1"
    );
    assert_eq!(
        document.attribute(attributes, None, "id").unwrap().value,
        "2"
    );
    assert_eq!(document.source(), source);
}

#[test]
fn child_namespace_declarations_do_not_leak_to_siblings() {
    for source in [
        br#"<r><q:child xmlns:q="urn:child"/><q:sibling/></r>"#.as_slice(),
        br#"<r><q:child xmlns:q="urn:child"></q:child><q:sibling/></r>"#.as_slice(),
    ] {
        let error = XmlDocument::parse(source).unwrap_err();
        assert_eq!(error.code(), XmlErrorCode::UndeclaredPrefix);
    }
}
