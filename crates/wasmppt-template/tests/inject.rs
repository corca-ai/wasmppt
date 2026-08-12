use std::{
    collections::BTreeMap,
    io::{self, Write},
};
use wasmppt_opc::WriteSink;
use wasmppt_opc::{
    CompressionMethod, Conformance, EntryOptions, PackageGraph, VecSink, ZipArchive, ZipWriter,
};
use wasmppt_template::{
    ChartData, ChartSeriesData, GenerateErrorCode, ImageCrop, ImageData, InjectionData,
    PreparedTemplate, RichTextRunData, SemanticShapeData, TableOverflowPolicy, TablePolicyData,
    TemplateCompiler,
};

const ADVANCED_FIXTURE: &[u8] = include_bytes!("../../../fixtures/render/basic.pptx");

fn template(main_type: &str, with_macro: bool) -> Vec<u8> {
    let options = EntryOptions::deterministic(CompressionMethod::Deflate);
    let mut writer = ZipWriter::new(VecSink::new());
    let macro_override = if with_macro {
        r#"<Override PartName="/ppt/vbaProject.bin" ContentType="application/vnd.ms-office.vbaProject"/><Override PartName="/ppt/vbaData.xml" ContentType="application/vnd.ms-powerpoint.vbaData+xml"/>"#
    } else {
        ""
    };
    let content_types = format!(
        r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/><Default Extension="bin" ContentType="application/octet-stream"/><Override PartName="/ppt/presentation.xml" ContentType="{main_type}"/>{macro_override}</Types>"#,
    );
    let macro_relationship = if with_macro {
        r#"<Relationship Id="vba" Type="http://schemas.microsoft.com/office/2006/relationships/vbaProject" Target="vbaProject.bin"/><Relationship Id="vbaData" Type="http://schemas.microsoft.com/office/2006/relationships/vbaData" Target="vbaData.xml"/>"#
    } else {
        ""
    };
    let presentation_relationships = format!(
        r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide" Target="slides/slide1.xml"/>{macro_relationship}</Relationships>"#,
    );
    let entries = [
        ("[Content_Types].xml", content_types.as_str()),
        (
            "_rels/.rels",
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="ppt/presentation.xml"/></Relationships>"#,
        ),
        (
            "ppt/_rels/presentation.xml.rels",
            presentation_relationships.as_str(),
        ),
        (
            "ppt/presentation.xml",
            r#"<p:presentation xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><p:sldIdLst><p:sldId id="256" r:id="rId1"/></p:sldIdLst></p:presentation>"#,
        ),
        (
            "ppt/slides/slide1.xml",
            r#"<p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><p:cSld><p:spTree><p:sp><p:nvSpPr><p:cNvPr id="2" name="Greeting"/></p:nvSpPr><p:txBody><a:p><a:r><a:t>Hello {{na</a:t></a:r><a:r><a:t>me}}</a:t></a:r></a:p></p:txBody></p:sp><p:sp><p:nvSpPr><p:cNvPr id="3" name="Macro"><a:hlinkClick action="ppaction://macro?name=Run"/></p:cNvPr></p:nvSpPr></p:sp></p:spTree></p:cSld></p:sld>"#,
        ),
        ("ppt/notesSlides/notesSlide1.xml", "<notes future=\"yes\"/>"),
        ("ppt/opaque.bin", "opaque future payload"),
    ];
    for (name, value) in entries {
        writer
            .write_entry(name, value.as_bytes(), &options)
            .unwrap();
    }
    if with_macro {
        writer
            .write_entry("ppt/vbaProject.bin", b"VBA", &options)
            .unwrap();
        writer
            .write_entry("ppt/vbaData.xml", b"<vbaData/>", &options)
            .unwrap();
    }
    writer.finish().unwrap().0.into_inner()
}

fn strict_template() -> Vec<u8> {
    let source = template(
        "application/vnd.openxmlformats-officedocument.presentationml.template.main+xml",
        false,
    );
    let archive = ZipArchive::from_bytes(source).unwrap();
    let options = EntryOptions::deterministic(CompressionMethod::Deflate);
    let mut writer = ZipWriter::new(VecSink::new());
    for entry in archive.entries() {
        let mut bytes = archive.read_entry(entry).unwrap();
        if entry.name.ends_with(".xml") || entry.name.ends_with(".rels") {
            let value = String::from_utf8(bytes).unwrap();
            bytes = value
                .replace(
                    "http://schemas.openxmlformats.org/presentationml/2006/main",
                    "http://purl.oclc.org/ooxml/presentationml/main",
                )
                .replace(
                    "http://schemas.openxmlformats.org/drawingml/2006/main",
                    "http://purl.oclc.org/ooxml/drawingml/main",
                )
                .replace(
                    "http://schemas.openxmlformats.org/officeDocument/2006/relationships",
                    "http://purl.oclc.org/ooxml/officeDocument/relationships",
                )
                .replace(
                    "http://schemas.openxmlformats.org/package/2006/relationships",
                    "http://purl.oclc.org/ooxml/package/relationships",
                )
                .replace(
                    "http://schemas.openxmlformats.org/package/2006/content-types",
                    "http://purl.oclc.org/ooxml/package/content-types",
                )
                .into_bytes();
        }
        writer.write_entry(&entry.name, &bytes, &options).unwrap();
    }
    writer.finish().unwrap().0.into_inner()
}

fn named_chart_template() -> Vec<u8> {
    let archive = ZipArchive::from_bytes(ADVANCED_FIXTURE.to_vec()).unwrap();
    let options = EntryOptions::deterministic(CompressionMethod::Deflate);
    let mut writer = ZipWriter::new(VecSink::new());
    for entry in archive.entries() {
        let mut bytes = archive.read_entry(entry).unwrap();
        if entry.name == "ppt/slides/slide2.xml" {
            bytes = String::from_utf8(bytes)
                .unwrap()
                .replace("Quarterly sales chart", "wasmppt:chart:sales")
                .into_bytes();
        }
        writer.write_entry(&entry.name, &bytes, &options).unwrap();
    }
    writer.finish().unwrap().0.into_inner()
}

fn image_template() -> Vec<u8> {
    let options = EntryOptions::deterministic(CompressionMethod::Deflate);
    let mut writer = ZipWriter::new(VecSink::new());
    let entries: [(&str, &[u8]); 7] = [
        (
            "[Content_Types].xml",
            br#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/><Default Extension="png" ContentType="image/png"/><Override PartName="/ppt/presentation.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.template.main+xml"/></Types>"#,
        ),
        (
            "_rels/.rels",
            br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="ppt/presentation.xml"/></Relationships>"#,
        ),
        (
            "ppt/_rels/presentation.xml.rels",
            br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide" Target="slides/slide1.xml"/></Relationships>"#,
        ),
        (
            "ppt/presentation.xml",
            br#"<p:presentation xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><p:sldIdLst><p:sldId id="256" r:id="rId1"/></p:sldIdLst></p:presentation>"#,
        ),
        (
            "ppt/slides/slide1.xml",
            br#"<p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><p:cSld><p:spTree><p:pic><p:nvPicPr><p:cNvPr id="4" name="Hero" descr="wasmppt:image:hero"><a:hlinkClick r:id="rLink"/></p:cNvPr></p:nvPicPr><p:blipFill><a:blip r:embed="rImg"/><a:srcRect l="0" t="0" r="0" b="0"/></p:blipFill></p:pic></p:spTree></p:cSld></p:sld>"#,
        ),
        (
            "ppt/slides/_rels/slide1.xml.rels",
            br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rImg" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="../media/original.png"/><Relationship Id="rLink" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink" Target="https://example.com" TargetMode="External"/></Relationships>"#,
        ),
        ("ppt/media/original.png", b"old image"),
    ];
    for (name, bytes) in entries {
        writer.write_entry(name, bytes, &options).unwrap();
    }
    writer.finish().unwrap().0.into_inner()
}

fn table_template() -> Vec<u8> {
    let options = EntryOptions::deterministic(CompressionMethod::Deflate);
    let mut writer = ZipWriter::new(VecSink::new());
    let entries: [(&str, &[u8]); 5] = [
        (
            "[Content_Types].xml",
            br#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/><Override PartName="/ppt/presentation.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.template.main+xml"/></Types>"#,
        ),
        (
            "_rels/.rels",
            br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="ppt/presentation.xml"/></Relationships>"#,
        ),
        (
            "ppt/_rels/presentation.xml.rels",
            br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide" Target="slides/slide1.xml"/></Relationships>"#,
        ),
        (
            "ppt/presentation.xml",
            br#"<p:presentation xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><p:sldIdLst><p:sldId id="256" r:id="rId1"/></p:sldIdLst></p:presentation>"#,
        ),
        (
            "ppt/slides/slide1.xml",
            br#"<p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><p:cSld><p:spTree><p:sp><p:nvSpPr><p:cNvPr id="8" name="Table"/></p:nvSpPr><p:txBody><a:tbl><a:tr h="1"><a:tc><a:p><a:r><a:t>{{items.name}}</a:t></a:r></a:p></a:tc><a:tc><a:p><a:r><a:t>{{items.amount}}</a:t></a:r></a:p></a:tc></a:tr></a:tbl></p:txBody></p:sp></p:spTree></p:cSld></p:sld>"#,
        ),
    ];
    for (name, bytes) in entries {
        writer.write_entry(name, bytes, &options).unwrap();
    }
    writer.finish().unwrap().0.into_inner()
}

fn clone_template() -> Vec<u8> {
    let options = EntryOptions::deterministic(CompressionMethod::Deflate);
    let mut writer = ZipWriter::new(VecSink::new());
    let entries: [(&str, &[u8]); 8] = [
        (
            "[Content_Types].xml",
            br#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/><Override PartName="/ppt/presentation.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.template.main+xml"/><Override PartName="/ppt/slides/slide1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slide+xml"/><Override PartName="/ppt/notesSlides/notesSlide1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.notesSlide+xml"/></Types>"#,
        ),
        (
            "_rels/.rels",
            br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="ppt/presentation.xml"/></Relationships>"#,
        ),
        (
            "ppt/_rels/presentation.xml.rels",
            br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide" Target="slides/slide1.xml"/></Relationships>"#,
        ),
        (
            "ppt/presentation.xml",
            br#"<p:presentation xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><p:sldIdLst><p:sldId id="256" r:id="rId1"/></p:sldIdLst></p:presentation>"#,
        ),
        (
            "ppt/slides/slide1.xml",
            br#"<p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><p:cSld><p:spTree><p:sp><p:nvSpPr><p:cNvPr id="2" name="Name"/></p:nvSpPr><p:txBody><a:p><a:r><a:t>{{name}}</a:t></a:r></a:p></p:txBody></p:sp></p:spTree></p:cSld></p:sld>"#,
        ),
        (
            "ppt/slides/_rels/slide1.xml.rels",
            br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rLink" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink" Target="https://example.com" TargetMode="External"/><Relationship Id="rNotes" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/notesSlide" Target="../notesSlides/notesSlide1.xml"/></Relationships>"#,
        ),
        (
            "ppt/notesSlides/notesSlide1.xml",
            br#"<p:notes xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><p:cSld><p:spTree><p:sp><p:txBody><a:p><a:r><a:t>Old notes</a:t></a:r></a:p></p:txBody></p:sp></p:spTree></p:cSld></p:notes>"#,
        ),
        ("ppt/opaque.bin", b"opaque"),
    ];
    for (name, bytes) in entries {
        writer.write_entry(name, bytes, &options).unwrap();
    }
    writer.finish().unwrap().0.into_inner()
}

#[test]
fn potm_generation_strips_macros_and_injects_escaped_unicode() {
    let bytes = template(
        "application/vnd.ms-powerpoint.template.macroEnabled.main+xml",
        true,
    );
    let archive = ZipArchive::from_bytes(bytes.clone()).unwrap();
    let plan = TemplateCompiler::new(Default::default())
        .compile(&archive)
        .unwrap()
        .plan;
    let prepared = PreparedTemplate::new(bytes, plan).unwrap();
    let data = InjectionData::new().with_text("name", "민수 & <팀>");
    let first = prepared.generate(&data).unwrap();
    let second = prepared.generate(&data).unwrap();
    assert_eq!(first.bytes, second.bytes);
    assert_eq!(first.zip_stats.inflated_entries, 0);
    assert!(first.zip_stats.raw_copied_entries > 0);
    assert_eq!(first.removed_entries, 2);

    let output = ZipArchive::from_bytes(first.bytes).unwrap();
    assert!(output.entry("ppt/vbaProject.bin").is_none());
    assert!(output.entry("ppt/vbaData.xml").is_none());
    let all_xml = output
        .entries()
        .iter()
        .filter(|entry| entry.name.ends_with(".xml") || entry.name.ends_with(".rels"))
        .flat_map(|entry| output.read_entry(entry).unwrap())
        .collect::<Vec<_>>();
    let text = String::from_utf8(all_xml).unwrap();
    assert!(!text.to_ascii_lowercase().contains("vba"));
    assert!(!text.to_ascii_lowercase().contains("macro?"));
    assert!(text.contains("민수 &amp; &lt;팀&gt;"));
    assert!(text.contains("presentation.main+xml"));
    let graph = PackageGraph::build(&output).unwrap();
    assert!(!graph.diagnostics().iter().any(|diagnostic| matches!(
        diagnostic.code,
        wasmppt_opc::DiagnosticCode::MissingRelationshipTarget
            | wasmppt_opc::DiagnosticCode::InvalidRelationshipsXml
            | wasmppt_opc::DiagnosticCode::InvalidContentTypesXml
    )));
}

#[test]
fn generation_v2_repeats_conditions_and_rich_text_transactionally() {
    let bytes = template(
        "application/vnd.openxmlformats-officedocument.presentationml.template.main+xml",
        false,
    );
    let archive = ZipArchive::from_bytes(bytes.clone()).unwrap();
    let plan = TemplateCompiler::new(Default::default())
        .compile(&archive)
        .unwrap()
        .plan;
    let prepared = PreparedTemplate::new(bytes, plan).unwrap();
    let data = InjectionData::new().with_semantic_shape(
        "name",
        SemanticShapeData {
            copies: Some(2),
            rich_text: Some(vec![RichTextRunData {
                text: "중요 & safe".to_owned(),
                bold: Some(true),
                color: Some("FF0000".to_owned()),
                ..RichTextRunData::default()
            }]),
            ..SemanticShapeData::default()
        },
    );
    let buffered = prepared.generate(&data).unwrap();
    let mut cursor = prepared.generate_cursor(&data).unwrap();
    let mut streamed = Vec::new();
    while !cursor.is_done() {
        streamed.extend(cursor.pull(7).unwrap());
    }
    assert_eq!(streamed, buffered.bytes);
    let output = ZipArchive::from_bytes(buffered.bytes).unwrap();
    let slide = String::from_utf8(
        output
            .read_entry(output.entry("ppt/slides/slide1.xml").unwrap())
            .unwrap(),
    )
    .unwrap();
    assert_eq!(slide.matches("중요 &amp; safe").count(), 2);
    assert!(slide.contains("id=\"2\""));
    assert!(slide.contains("id=\"4\""));
    assert_eq!(slide.matches("val=\"FF0000\"").count(), 2);

    let hidden = InjectionData::new().with_semantic_shape(
        "name",
        SemanticShapeData {
            visible: Some(false),
            ..SemanticShapeData::default()
        },
    );
    let hidden = prepared.generate(&hidden).unwrap();
    let output = ZipArchive::from_bytes(hidden.bytes).unwrap();
    let slide = String::from_utf8(
        output
            .read_entry(output.entry("ppt/slides/slide1.xml").unwrap())
            .unwrap(),
    )
    .unwrap();
    assert!(!slide.contains("Greeting"));
}

#[test]
fn generation_v2_writes_notes_by_slide_part() {
    let bytes = clone_template();
    let archive = ZipArchive::from_bytes(bytes.clone()).unwrap();
    let plan = TemplateCompiler::new(Default::default())
        .compile(&archive)
        .unwrap()
        .plan;
    let data = InjectionData::new()
        .with_text("name", "Ada")
        .with_notes("ppt/slides/slide1.xml", "Private & clear");
    let output = PreparedTemplate::new(bytes, plan)
        .unwrap()
        .generate(&data)
        .unwrap();
    let archive = ZipArchive::from_bytes(output.bytes).unwrap();
    let notes = String::from_utf8(
        archive
            .read_entry(archive.entry("ppt/notesSlides/notesSlide1.xml").unwrap())
            .unwrap(),
    )
    .unwrap();
    assert!(notes.contains("Private &amp; clear"));
}

#[test]
fn potx_conversion_preserves_notes_and_unknown_payloads() {
    let bytes = template(
        "application/vnd.openxmlformats-officedocument.presentationml.template.main+xml",
        false,
    );
    let archive = ZipArchive::from_bytes(bytes.clone()).unwrap();
    let opaque = archive
        .read_compressed(archive.entry("ppt/opaque.bin").unwrap())
        .unwrap();
    let plan = TemplateCompiler::new(Default::default())
        .compile(&archive)
        .unwrap()
        .plan;
    let output = PreparedTemplate::new(bytes, plan)
        .unwrap()
        .generate(&InjectionData::new().with_text("name", "Ada"))
        .unwrap();
    let archive = ZipArchive::from_bytes(output.bytes).unwrap();
    assert_eq!(
        archive
            .read_compressed(archive.entry("ppt/opaque.bin").unwrap())
            .unwrap(),
        opaque
    );
    assert_eq!(
        archive
            .read_entry(archive.entry("ppt/notesSlides/notesSlide1.xml").unwrap())
            .unwrap(),
        b"<notes future=\"yes\"/>"
    );
}

#[test]
fn strict_template_targeted_edit_preserves_conformance_and_unknown_bytes() {
    let bytes = strict_template();
    let source = ZipArchive::from_bytes(bytes.clone()).unwrap();
    assert_eq!(
        PackageGraph::build(&source).unwrap().conformance(),
        Conformance::Strict
    );
    let opaque = source
        .read_compressed(source.entry("ppt/opaque.bin").unwrap())
        .unwrap();
    let plan = TemplateCompiler::new(Default::default())
        .compile(&source)
        .unwrap()
        .plan;
    let generated = PreparedTemplate::new(bytes, plan)
        .unwrap()
        .generate(&InjectionData::new().with_text("name", "엄격 & 안전"))
        .unwrap();
    assert_eq!(generated.zip_stats.inflated_entries, 0);
    let output = ZipArchive::from_bytes(generated.bytes).unwrap();
    let graph = PackageGraph::build(&output).unwrap();
    assert_eq!(graph.conformance(), Conformance::Strict);
    assert_eq!(
        output
            .read_compressed(output.entry("ppt/opaque.bin").unwrap())
            .unwrap(),
        opaque,
    );
    let slide = String::from_utf8(
        output
            .read_entry(output.entry("ppt/slides/slide1.xml").unwrap())
            .unwrap(),
    )
    .unwrap();
    assert!(slide.contains("엄격 &amp; 안전"));
    assert!(slide.contains("http://purl.oclc.org/ooxml/presentationml/main"));
}

#[test]
fn malformed_or_missing_binding_data_fails_with_a_stable_code() {
    let bytes = template(
        "application/vnd.openxmlformats-officedocument.presentationml.template.main+xml",
        false,
    );
    let archive = ZipArchive::from_bytes(bytes.clone()).unwrap();
    let plan = TemplateCompiler::new(Default::default())
        .compile(&archive)
        .unwrap()
        .plan;
    let error = PreparedTemplate::new(bytes, plan)
        .unwrap()
        .generate(&InjectionData::new())
        .unwrap_err();
    assert_eq!(error.code(), GenerateErrorCode::MissingValue);
}

#[test]
fn image_replacement_updates_media_relationship_crop_and_content_type() {
    let bytes = image_template();
    let archive = ZipArchive::from_bytes(bytes.clone()).unwrap();
    let plan = TemplateCompiler::new(Default::default())
        .compile(&archive)
        .unwrap()
        .plan;
    assert_eq!(plan.bindings.len(), 1);
    let data = InjectionData::new().with_image(
        "hero",
        ImageData {
            bytes: b"new jpeg bytes".to_vec(),
            extension: "jpg".to_owned(),
            content_type: "image/jpeg".to_owned(),
            crop: Some(ImageCrop {
                left: 100,
                top: 200,
                right: 300,
                bottom: 400,
            }),
            fit: Default::default(),
        },
    );
    let generated = PreparedTemplate::new(bytes, plan)
        .unwrap()
        .generate(&data)
        .unwrap();
    let output = ZipArchive::from_bytes(generated.bytes).unwrap();
    assert!(output.entry("ppt/media/original.png").is_none());
    assert_eq!(
        output
            .read_entry(output.entry("ppt/media/wasmppt-hero.jpg").unwrap())
            .unwrap(),
        b"new jpeg bytes"
    );
    let relationships = String::from_utf8(
        output
            .read_entry(output.entry("ppt/slides/_rels/slide1.xml.rels").unwrap())
            .unwrap(),
    )
    .unwrap();
    assert!(relationships.contains("../media/wasmppt-hero.jpg"));
    assert!(relationships.contains("https://example.com"));
    let slide = String::from_utf8(
        output
            .read_entry(output.entry("ppt/slides/slide1.xml").unwrap())
            .unwrap(),
    )
    .unwrap();
    assert!(slide.contains("l=\"100\" t=\"200\" r=\"300\" b=\"400\""));
    assert!(slide.contains("r:id=\"rLink\""));
    let content_types = String::from_utf8(
        output
            .read_entry(output.entry("[Content_Types].xml").unwrap())
            .unwrap(),
    )
    .unwrap();
    assert!(content_types.contains("Extension=\"jpg\" ContentType=\"image/jpeg\""));
    let graph = PackageGraph::build(&output).unwrap();
    assert!(!graph.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == wasmppt_opc::DiagnosticCode::MissingRelationshipTarget
    }));
}

#[test]
fn table_row_repetition_preserves_row_markup_and_escapes_values() {
    let bytes = table_template();
    let archive = ZipArchive::from_bytes(bytes.clone()).unwrap();
    let plan = TemplateCompiler::new(Default::default())
        .compile(&archive)
        .unwrap()
        .plan;
    let rows = [
        (&[("name", "Alpha"), ("amount", "10")][..]),
        (&[("name", "B & <C>"), ("amount", "20")][..]),
    ]
    .into_iter()
    .map(|fields| {
        fields
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect::<BTreeMap<_, _>>()
    })
    .collect();
    let output = PreparedTemplate::new(bytes, plan)
        .unwrap()
        .generate(&InjectionData::new().with_table_rows("items", rows))
        .unwrap();
    let archive = ZipArchive::from_bytes(output.bytes).unwrap();
    let slide = String::from_utf8(
        archive
            .read_entry(archive.entry("ppt/slides/slide1.xml").unwrap())
            .unwrap(),
    )
    .unwrap();
    assert_eq!(slide.matches("<a:tr h=\"1\">").count(), 2);
    assert!(slide.contains("Alpha"));
    assert!(slide.contains("B &amp; &lt;C&gt;"));
    assert!(!slide.contains("{{items."));
}

#[test]
fn table_overflow_policies_fail_clip_or_shrink_before_emission() {
    let bytes = table_template();
    let archive = ZipArchive::from_bytes(bytes.clone()).unwrap();
    let plan = TemplateCompiler::new(Default::default())
        .compile(&archive)
        .unwrap()
        .plan;
    let rows = (0..3)
        .map(|index| {
            BTreeMap::from([
                ("name".to_owned(), format!("Row {index}")),
                ("amount".to_owned(), index.to_string()),
            ])
        })
        .collect::<Vec<_>>();
    let prepared = PreparedTemplate::new(bytes, plan).unwrap();
    let failed = prepared
        .generate(
            &InjectionData::new()
                .with_table_rows("items", rows.clone())
                .with_table_policy(
                    "items",
                    TablePolicyData {
                        maximum_rows: 2,
                        overflow: TableOverflowPolicy::Fail,
                    },
                ),
        )
        .unwrap_err();
    assert_eq!(failed.code(), GenerateErrorCode::InvalidTable);

    for (policy, expected_rows, expected_height) in [
        (TableOverflowPolicy::Clip, 2, "h=\"1\""),
        (TableOverflowPolicy::Shrink, 3, "h=\"1\""),
    ] {
        let generated = prepared
            .generate(
                &InjectionData::new()
                    .with_table_rows("items", rows.clone())
                    .with_table_policy(
                        "items",
                        TablePolicyData {
                            maximum_rows: 2,
                            overflow: policy,
                        },
                    ),
            )
            .unwrap();
        let package = ZipArchive::from_bytes(generated.bytes).unwrap();
        let slide = String::from_utf8(
            package
                .read_entry(package.entry("ppt/slides/slide1.xml").unwrap())
                .unwrap(),
        )
        .unwrap();
        assert_eq!(slide.matches("<a:tr").count(), expected_rows);
        assert!(slide.contains(expected_height));
    }
}

#[test]
fn slide_cloning_allocates_deterministic_ids_and_preserves_hyperlinks() {
    let bytes = clone_template();
    let archive = ZipArchive::from_bytes(bytes.clone()).unwrap();
    let plan = TemplateCompiler::new(Default::default())
        .compile(&archive)
        .unwrap()
        .plan;
    let data = InjectionData::new()
        .with_text("name", "Cloned")
        .with_slide_copies("ppt/slides/slide1.xml", 3);
    let prepared = PreparedTemplate::new(bytes, plan).unwrap();
    let first = prepared.generate(&data).unwrap();
    let second = prepared.generate(&data).unwrap();
    assert_eq!(first.bytes, second.bytes);
    let output = ZipArchive::from_bytes(first.bytes).unwrap();
    for number in 1..=3 {
        let slide_name = format!("ppt/slides/slide{number}.xml");
        let slide = String::from_utf8(
            output
                .read_entry(output.entry(&slide_name).unwrap())
                .unwrap(),
        )
        .unwrap();
        assert!(slide.contains("Cloned"));
    }
    let presentation = String::from_utf8(
        output
            .read_entry(output.entry("ppt/presentation.xml").unwrap())
            .unwrap(),
    )
    .unwrap();
    assert!(presentation.contains("id=\"256\" r:id=\"rId1\""));
    assert!(presentation.contains("id=\"257\" r:id=\"rId2\""));
    assert!(presentation.contains("id=\"258\" r:id=\"rId3\""));
    for number in 2..=3 {
        let rels_name = format!("ppt/slides/_rels/slide{number}.xml.rels");
        let rels = String::from_utf8(
            output
                .read_entry(output.entry(&rels_name).unwrap())
                .unwrap(),
        )
        .unwrap();
        assert!(rels.contains("https://example.com"));
        assert!(!rels.contains("notesSlide"));
    }
    assert!(output.entry("ppt/notesSlides/notesSlide1.xml").is_some());
    assert!(
        !PackageGraph::build(&output)
            .unwrap()
            .diagnostics()
            .iter()
            .any(|diagnostic| {
                diagnostic.code == wasmppt_opc::DiagnosticCode::MissingRelationshipTarget
            })
    );
}

#[test]
fn zero_slide_copies_excludes_slide_and_its_relationship() {
    let bytes = clone_template();
    let archive = ZipArchive::from_bytes(bytes.clone()).unwrap();
    let plan = TemplateCompiler::new(Default::default())
        .compile(&archive)
        .unwrap()
        .plan;
    let output = PreparedTemplate::new(bytes, plan)
        .unwrap()
        .generate(&InjectionData::new().with_slide_copies("ppt/slides/slide1.xml", 0))
        .unwrap();
    let output = ZipArchive::from_bytes(output.bytes).unwrap();
    assert!(output.entry("ppt/slides/slide1.xml").is_none());
    assert!(output.entry("ppt/slides/_rels/slide1.xml.rels").is_none());
    let presentation = String::from_utf8(
        output
            .read_entry(output.entry("ppt/presentation.xml").unwrap())
            .unwrap(),
    )
    .unwrap();
    assert!(!presentation.contains("sldId id"));
    let rels = String::from_utf8(
        output
            .read_entry(output.entry("ppt/_rels/presentation.xml.rels").unwrap())
            .unwrap(),
    )
    .unwrap();
    assert!(!rels.contains("slides/slide1.xml"));
}

#[derive(Default)]
struct AppendOnly(Vec<u8>);

impl Write for AppendOnly {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn generation_streams_to_a_non_seekable_sink() {
    let bytes = template(
        "application/vnd.openxmlformats-officedocument.presentationml.template.main+xml",
        false,
    );
    let archive = ZipArchive::from_bytes(bytes.clone()).unwrap();
    let plan = TemplateCompiler::new(Default::default())
        .compile(&archive)
        .unwrap()
        .plan;
    let (sink, stats) = PreparedTemplate::new(bytes, plan)
        .unwrap()
        .generate_to(
            &InjectionData::new().with_text("name", "stream"),
            WriteSink::new(AppendOnly::default()),
        )
        .unwrap();
    assert!(stats.zip.raw_copied_entries > 0);
    assert_eq!(stats.zip.inflated_entries, 0);
    ZipArchive::from_bytes(sink.into_inner().0).unwrap();
}

#[test]
fn pull_generation_matches_buffered_generation_for_one_byte_chunks() {
    let bytes = template(
        "application/vnd.openxmlformats-officedocument.presentationml.template.main+xml",
        false,
    );
    let archive = ZipArchive::from_bytes(bytes.clone()).unwrap();
    let plan = TemplateCompiler::new(Default::default())
        .compile(&archive)
        .unwrap()
        .plan;
    let prepared = PreparedTemplate::new(bytes, plan).unwrap();
    let data = InjectionData::new().with_text("name", "스트리밍 & parity");
    let expected = prepared.generate(&data).unwrap();
    let mut cursor = prepared.generate_cursor(&data).unwrap();
    let mut actual = Vec::new();
    while !cursor.is_done() {
        let chunk = cursor.pull(1).unwrap();
        assert!(chunk.len() <= 1);
        actual.extend(chunk);
    }
    assert_eq!(actual, expected.bytes);
    assert_eq!(cursor.stats().unwrap().zip, expected.zip_stats);
}

#[test]
fn chart_injection_updates_cache_and_embedded_workbook_atomically() {
    let bytes = ADVANCED_FIXTURE.to_vec();
    let archive = ZipArchive::from_bytes(bytes.clone()).unwrap();
    let plan = TemplateCompiler::new(Default::default())
        .compile(&archive)
        .unwrap()
        .plan;
    let chart = ChartData {
        categories: vec!["북부".to_owned(), "남부 & 동부".to_owned()],
        series: vec![ChartSeriesData {
            name: "매출 <확정>".to_owned(),
            values: vec![101.5, 202.25],
        }],
    };
    let output = PreparedTemplate::new(bytes, plan)
        .unwrap()
        .generate(&InjectionData::new().with_chart("ppt/charts/chart1.xml", chart))
        .unwrap();
    assert_eq!(output.rewritten_entries, 2);
    let package = ZipArchive::from_bytes(output.bytes).unwrap();
    let chart_xml = String::from_utf8(
        package
            .read_entry(package.entry("ppt/charts/chart1.xml").unwrap())
            .unwrap(),
    )
    .unwrap();
    assert!(chart_xml.contains("매출 &lt;확정&gt;"));
    assert!(chart_xml.contains("남부 &amp; 동부"));
    assert!(chart_xml.contains("Sheet1!$A$2:$A$3"));
    assert!(chart_xml.contains("Sheet1!$B$2:$B$3"));
    assert!(!chart_xml.contains(">64<"));
    let workbook_bytes = package
        .read_entry(package.entry("ppt/embeddings/sales.xlsx").unwrap())
        .unwrap();
    let workbook = ZipArchive::from_bytes(workbook_bytes).unwrap();
    let sheet = String::from_utf8(
        workbook
            .read_entry(workbook.entry("xl/worksheets/sheet1.xml").unwrap())
            .unwrap(),
    )
    .unwrap();
    assert!(sheet.contains("매출 &lt;확정&gt;"));
    assert!(sheet.contains("남부 &amp; 동부"));
    assert!(sheet.contains(">202.25<"));
    assert!(!sheet.contains(">64<"));
}

#[test]
fn chart_injection_uses_stable_alt_text_metadata_instead_of_part_names() {
    let bytes = named_chart_template();
    let archive = ZipArchive::from_bytes(bytes.clone()).unwrap();
    let compiled = TemplateCompiler::new(Default::default())
        .compile(&archive)
        .unwrap();
    let binding = compiled
        .plan
        .bindings
        .iter()
        .find(|binding| binding.id == "sales")
        .expect("named chart binding");
    assert_eq!(binding.kind, wasmppt_template::BindingKind::Chart);
    let chart = ChartData {
        categories: vec!["Q1".to_owned(), "Q2".to_owned()],
        series: vec![ChartSeriesData {
            name: "Revenue".to_owned(),
            values: vec![12.0, 24.0],
        }],
    };
    let output = PreparedTemplate::new(bytes, compiled.plan)
        .unwrap()
        .generate(&InjectionData::new().with_chart("sales", chart))
        .unwrap();
    let package = ZipArchive::from_bytes(output.bytes).unwrap();
    let chart_xml = String::from_utf8(
        package
            .read_entry(package.entry("ppt/charts/chart1.xml").unwrap())
            .unwrap(),
    )
    .unwrap();
    assert!(chart_xml.contains("Revenue"));
    assert!(chart_xml.contains(">24<"));
}

#[test]
fn invalid_chart_data_fails_before_writing_partial_output() {
    let bytes = ADVANCED_FIXTURE.to_vec();
    let archive = ZipArchive::from_bytes(bytes.clone()).unwrap();
    let plan = TemplateCompiler::new(Default::default())
        .compile(&archive)
        .unwrap()
        .plan;
    let error = PreparedTemplate::new(bytes, plan)
        .unwrap()
        .generate(&InjectionData::new().with_chart(
            "ppt/charts/chart1.xml",
            ChartData {
                categories: vec!["Q1".to_owned(), "Q2".to_owned()],
                series: vec![ChartSeriesData {
                    name: "Sales".to_owned(),
                    values: vec![1.0],
                }],
            },
        ))
        .unwrap_err();
    assert_eq!(error.code(), GenerateErrorCode::InvalidChart);
}
