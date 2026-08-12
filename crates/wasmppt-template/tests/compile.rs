use wasmppt_opc::{CompressionMethod, EntryOptions, VecSink, ZipArchive, ZipWriter};
use wasmppt_template::{
    BindingDiagnosticCode, BindingSource, CompressionProfile, ReuseDecision, TemplateCompiler,
    TemplatePlan,
};

fn package(extra_shapes: &str, manifest: Option<&str>) -> Vec<u8> {
    let options = EntryOptions::deterministic(CompressionMethod::Deflate);
    let mut writer = ZipWriter::new(VecSink::new());
    let entries = [
        (
            "[Content_Types].xml",
            r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/><Override PartName="/ppt/presentation.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml"/></Types>"#,
        ),
        (
            "_rels/.rels",
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="ppt/presentation.xml"/></Relationships>"#,
        ),
        (
            "ppt/_rels/presentation.xml.rels",
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide" Target="slides/slide1.xml"/></Relationships>"#,
        ),
        (
            "ppt/presentation.xml",
            r#"<p:presentation xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><p:sldIdLst><p:sldId id="256" r:id="rId1"/></p:sldIdLst></p:presentation>"#,
        ),
    ];
    for (name, bytes) in entries {
        writer
            .write_entry(name, bytes.as_bytes(), &options)
            .unwrap();
    }
    let slide = format!(
        r#"<p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><p:cSld><p:spTree><p:sp><p:nvSpPr><p:cNvPr id="1" name="Customer"/></p:nvSpPr><p:txBody><a:p><a:r><a:t>{{{{cus</a:t></a:r><a:r><a:t>tomer}}}}</a:t></a:r></a:p></p:txBody></p:sp><p:sp><p:nvSpPr><p:cNvPr id="2" name="Revenue" descr="wasmppt:text:revenue"/></p:nvSpPr><p:txBody><a:p><a:r><a:t>old</a:t></a:r></a:p></p:txBody></p:sp><p:sp><p:nvSpPr><p:cNvPr id="3" name="Headline"/></p:nvSpPr><p:txBody><a:p><a:r><a:t>headline</a:t></a:r></a:p></p:txBody></p:sp>{extra_shapes}</p:spTree></p:cSld></p:sld>"#,
    );
    writer
        .write_entry("ppt/slides/slide1.xml", slide.as_bytes(), &options)
        .unwrap();
    if let Some(manifest) = manifest {
        writer
            .write_entry("wasmppt/bindings.xml", manifest.as_bytes(), &options)
            .unwrap();
    }
    writer.finish().unwrap().0.into_inner()
}

#[test]
fn compiles_metadata_manifest_and_split_run_tokens_once_into_a_stable_plan() {
    let manifest = r#"<bindings xmlns="urn:wasmppt:bindings:v1"><bind id="headline" kind="text" part="/ppt/slides/slide1.xml" shapeName="Headline"/></bindings>"#;
    let archive = ZipArchive::from_bytes(package("", Some(manifest))).unwrap();
    let compiler = TemplateCompiler::new(Default::default());
    let first = compiler.compile(&archive).unwrap();
    let second = compiler.compile(&archive).unwrap();
    assert!(first.diagnostics.is_empty());
    assert_eq!(first.plan.bindings.len(), 3);
    assert_eq!(
        first
            .plan
            .bindings
            .iter()
            .find(|binding| binding.id == "customer")
            .unwrap()
            .text_spans
            .len(),
        2
    );
    assert_eq!(
        first
            .plan
            .bindings
            .iter()
            .find(|binding| binding.id == "revenue")
            .unwrap()
            .source,
        BindingSource::ShapeMetadata
    );
    assert_eq!(
        first.plan.structural_signature(),
        second.plan.structural_signature()
    );

    let encoded = first.plan.encode();
    assert_eq!(TemplatePlan::decode(&encoded).unwrap(), first.plan);
    assert_eq!(
        first.plan.reuse_decision(&first.plan.identity),
        ReuseDecision::Reuse
    );
}

#[test]
fn identity_mismatch_fails_closed_to_recompilation() {
    let archive = ZipArchive::from_bytes(package("", None)).unwrap();
    let plan = TemplateCompiler::new(Default::default())
        .compile(&archive)
        .unwrap()
        .plan;
    let mut expected = plan.identity.clone();
    expected.compression = CompressionProfile::StoreMedia;
    assert!(matches!(
        plan.reuse_decision(&expected),
        ReuseDecision::Recompile(_)
    ));
}

#[test]
fn reports_duplicate_missing_ambiguous_and_unsupported_bindings() {
    let duplicate_shape = r#"<p:sp><p:nvSpPr><p:cNvPr id="4" name="Revenue" descr="wasmppt:text:revenue"/></p:nvSpPr><p:txBody><a:p><a:r><a:t>x</a:t></a:r></a:p></p:txBody></p:sp>"#;
    let manifest = r#"<bindings xmlns="urn:wasmppt:bindings:v1"><bind id="missing" kind="text" part="ppt/slides/slide1.xml" shapeName="Nope"/><bind id="ambiguous" kind="text" part="ppt/slides/slide1.xml" shapeName="Revenue"/><bind id="chart" kind="chart" part="ppt/slides/slide1.xml" shapeId="2"/></bindings>"#;
    let archive = ZipArchive::from_bytes(package(duplicate_shape, Some(manifest))).unwrap();
    let output = TemplateCompiler::new(Default::default())
        .compile(&archive)
        .unwrap();
    let codes = output
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect::<Vec<_>>();
    assert!(codes.contains(&BindingDiagnosticCode::DuplicateId));
    assert!(codes.contains(&BindingDiagnosticCode::MissingTarget));
    assert!(codes.contains(&BindingDiagnosticCode::AmbiguousTarget));
    assert!(codes.contains(&BindingDiagnosticCode::UnsupportedKind));
}

#[test]
fn rejects_truncated_or_unknown_plan_serialization() {
    assert!(TemplatePlan::decode(b"bad").is_err());
}
