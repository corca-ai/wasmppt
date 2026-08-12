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
