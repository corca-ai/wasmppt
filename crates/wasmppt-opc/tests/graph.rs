use wasmppt_opc::{
    CompressionMethod, Conformance, DiagnosticCode, EntryOptions, PackageGraph, RelationshipTarget,
    RewriteMode, VecSink, ZipArchive, ZipWriter, rewrite_archive_to_vec,
};

const CT_TRANSITIONAL: &str = "http://schemas.openxmlformats.org/package/2006/content-types";
const REL_TRANSITIONAL: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const OFFICE_REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

fn package(entries: &[(&str, &str)]) -> Vec<u8> {
    let options = EntryOptions::deterministic(CompressionMethod::Deflate);
    let mut writer = ZipWriter::new(VecSink::new());
    for (name, value) in entries {
        writer
            .write_entry(name, value.as_bytes(), &options)
            .unwrap();
    }
    writer.finish().unwrap().0.into_inner()
}

fn transitional_fixture() -> Vec<u8> {
    package(&[
        (
            "[Content_Types].xml",
            &format!(
                r#"<Types xmlns="{CT_TRANSITIONAL}"><Default Extension="xml" ContentType="application/xml"/><Default Extension="bin" ContentType="application/octet-stream"/><Override PartName="/ppt/presentation.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml"/></Types>"#,
            ),
        ),
        (
            "_rels/.rels",
            &format!(
                r#"<Relationships xmlns="{REL_TRANSITIONAL}"><Relationship Id="rId1" Type="{OFFICE_REL}/officeDocument" Target="ppt/presentation.xml"/><Relationship Id="ext" Type="urn:external" Target="https://example.com" TargetMode="External"/></Relationships>"#,
            ),
        ),
        (
            "ppt/_rels/presentation.xml.rels",
            &format!(
                r#"<Relationships xmlns="{REL_TRANSITIONAL}"><Relationship Id="rId1" Type="{OFFICE_REL}/slide" Target="slides/slide1.xml"/></Relationships>"#,
            ),
        ),
        (
            "ppt/slides/_rels/slide1.xml.rels",
            &format!(
                r#"<Relationships xmlns="{REL_TRANSITIONAL}"><Relationship Id="back" Type="urn:cycle" Target="../presentation.xml"/></Relationships>"#,
            ),
        ),
        (
            "ppt/presentation.xml",
            r#"<p:presentation xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006"><mc:AlternateContent><mc:Choice Requires="p14"><p:extLst/></mc:Choice></mc:AlternateContent></p:presentation>"#,
        ),
        (
            "ppt/slides/slide1.xml",
            r#"<p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"/>"#,
        ),
        ("ppt/opaque.bin", "future opaque bytes"),
    ])
}

#[test]
fn builds_cycle_safe_graph_and_reports_orphans() {
    let archive = ZipArchive::from_bytes(transitional_fixture()).unwrap();
    let graph = PackageGraph::build(&archive).unwrap();
    assert_eq!(graph.conformance(), Conformance::Transitional);
    assert!(graph.interned_namespace_count() >= 3);

    let presentation = graph.part_by_name("ppt/presentation.xml").unwrap();
    let slide = graph.part_by_name("ppt/slides/slide1.xml").unwrap();
    let opaque = graph.part_by_name("ppt/opaque.bin").unwrap();
    assert!(!presentation.orphaned);
    assert!(!slide.orphaned);
    assert!(opaque.orphaned);
    assert!(
        graph
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code == DiagnosticCode::RelationshipCycle)
    );
    assert!(
        graph
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code == DiagnosticCode::OrphanedPart)
    );
    assert!(graph.walk_from(presentation.id, 1).is_err());
    assert_eq!(graph.walk_from(presentation.id, 8).unwrap().len(), 2);
    assert!(matches!(
        graph.package_relationships()[1].target,
        RelationshipTarget::External(_)
    ));
}

#[test]
fn graph_inspection_and_no_op_rewrite_preserve_extension_and_opaque_bytes() {
    let bytes = transitional_fixture();
    let archive = ZipArchive::from_bytes(bytes).unwrap();
    let original_xml = archive
        .read_compressed(archive.entry("ppt/presentation.xml").unwrap())
        .unwrap();
    let original_opaque = archive
        .read_compressed(archive.entry("ppt/opaque.bin").unwrap())
        .unwrap();
    PackageGraph::build(&archive).unwrap();

    let rewritten = rewrite_archive_to_vec(&archive, RewriteMode::Preserve)
        .unwrap()
        .0;
    let rewritten = ZipArchive::from_bytes(rewritten).unwrap();
    assert_eq!(
        rewritten
            .read_compressed(rewritten.entry("ppt/presentation.xml").unwrap())
            .unwrap(),
        original_xml
    );
    assert_eq!(
        rewritten
            .read_compressed(rewritten.entry("ppt/opaque.bin").unwrap())
            .unwrap(),
        original_opaque
    );
}

#[test]
fn detects_strict_packages_explicitly() {
    let bytes = package(&[
        (
            "[Content_Types].xml",
            r#"<Types xmlns="http://purl.oclc.org/ooxml/package/content-types"><Default Extension="xml" ContentType="application/xml"/></Types>"#,
        ),
        (
            "_rels/.rels",
            r#"<Relationships xmlns="http://purl.oclc.org/ooxml/package/relationships"><Relationship Id="rId1" Type="http://purl.oclc.org/ooxml/officeDocument/relationships/officeDocument" Target="ppt/presentation.xml"/></Relationships>"#,
        ),
        (
            "ppt/presentation.xml",
            r#"<p:presentation xmlns:p="http://purl.oclc.org/ooxml/presentationml/main"/>"#,
        ),
    ]);
    let archive = ZipArchive::from_bytes(bytes).unwrap();
    assert_eq!(
        PackageGraph::build(&archive).unwrap().conformance(),
        Conformance::Strict
    );
}

#[test]
fn malformed_relationships_produce_machine_readable_diagnostics() {
    let bytes = package(&[
        (
            "[Content_Types].xml",
            &format!(
                r#"<Types xmlns="{CT_TRANSITIONAL}"><Default Extension="xml" ContentType="application/xml"/></Types>"#
            ),
        ),
        (
            "_rels/.rels",
            &format!(
                r#"<Relationships xmlns="{REL_TRANSITIONAL}"><Relationship Id="same" Type="urn:x" Target="missing.xml"/><Relationship Id="same" Type="urn:y" Target="also-missing.xml"/></Relationships>"#
            ),
        ),
        ("ppt/presentation.xml", "<broken>"),
    ]);
    let archive = ZipArchive::from_bytes(bytes).unwrap();
    let graph = PackageGraph::build(&archive).unwrap();
    let codes = graph
        .diagnostics()
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect::<Vec<_>>();
    assert!(codes.contains(&DiagnosticCode::MissingRelationshipTarget));
    assert!(codes.contains(&DiagnosticCode::DuplicateRelationshipId));
}
