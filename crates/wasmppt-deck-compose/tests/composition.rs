use std::{borrow::Cow, sync::Arc};

use gif::{Encoder, Frame};
use sha2::{Digest, Sha256};
use wasmppt_deck::{
    ContentFit, Continuation, DeckLimits, DeckPlan, DeckResource, DeckSpec, DeckTemplatePlan,
    EmuRect, EmuSize, FragmentSlice, HyperlinkKind, ImageContent, ListContent, ListItem,
    LogicalSlide, LogicalSlideKind, PhysicalPage, PixelSize, PlaceholderIdentity, PlannedFragment,
    PlannedRegion, RegionRole, ResourceKind, RichText, RichTextRun, SafeHyperlink, SemanticContent,
    SemanticNode, SemanticRole, SourceRange, SplitPolicy, StableId, SvgContent, TemplateLayout,
    TemplateLayoutRole, TemplateRegion, TemplateTextColor, TemplateTextLevel, TemplateTheme,
    TextMargins, TextMarks, TypeChoice, validate_deck_plan,
};
use wasmppt_deck_compose::{ComposeErrorCode, ComposeLimits, DeckComposer};
use wasmppt_layout::PresentationDocument;
use wasmppt_opc::{
    CompressionMethod, EntryOptions, PackageGraph, PackagePartSource, VecSink, ZipArchive,
    ZipWriter,
};

const CT: &str = "http://schemas.openxmlformats.org/package/2006/content-types";
const REL: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const OFFICE_REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const PML: &str = "http://schemas.openxmlformats.org/presentationml/2006/main";
const DRAWING: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";

fn id(value: u8) -> StableId {
    let mut bytes = [0; 16];
    bytes[15] = value;
    StableId::from_bytes(bytes)
}

fn range(start: u32, end: u32) -> SourceRange {
    SourceRange::new("deck.md", start, end)
}

fn package() -> Vec<u8> {
    let entries = [
        (
            "[Content_Types].xml",
            format!(
                "<Types xmlns=\"{CT}\"><Default Extension=\"xml\" ContentType=\"application/xml\"/><Default Extension=\"bin\" ContentType=\"application/octet-stream\"/><Override PartName=\"/ppt/presentation.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.template.main+xml\"/><Override PartName=\"/ppt/slideLayouts/content.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml\"/></Types>"
            ),
        ),
        (
            "_rels/.rels",
            format!(
                "<Relationships xmlns=\"{REL}\"><Relationship Id=\"rId1\" Type=\"{OFFICE_REL}/officeDocument\" Target=\"ppt/presentation.xml\"/></Relationships>"
            ),
        ),
        (
            "ppt/presentation.xml",
            format!(
                "<p:presentation xmlns:p=\"{PML}\"><p:sldSz cx=\"10000000\" cy=\"5625000\"/></p:presentation>"
            ),
        ),
        (
            "ppt/_rels/presentation.xml.rels",
            format!("<Relationships xmlns=\"{REL}\"></Relationships>"),
        ),
        (
            "ppt/slideLayouts/content.xml",
            format!(
                "<p:sldLayout xmlns:p=\"{PML}\" xmlns:a=\"{DRAWING}\"><p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/></p:spTree></p:cSld></p:sldLayout>"
            ),
        ),
        ("custom/opaque.bin", "unknown-template-data".to_owned()),
    ];
    let options = EntryOptions::deterministic(CompressionMethod::Deflate);
    let mut writer = ZipWriter::new(VecSink::new());
    for (name, value) in entries {
        writer
            .write_entry(name, value.as_bytes(), &options)
            .unwrap();
    }
    writer.finish().unwrap().0.into_inner()
}

fn gif() -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut encoder = Encoder::new(&mut bytes, 2, 1, &[255, 0, 0, 0, 0, 255]).unwrap();
        let frame = Frame {
            width: 2,
            height: 1,
            buffer: Cow::Owned(vec![0, 1]),
            ..Frame::default()
        };
        encoder.write_frame(&frame).unwrap();
    }
    bytes
}

fn rich(text: &str) -> RichText {
    RichText {
        runs: vec![RichTextRun {
            text: text.to_owned(),
            marks: TextMarks {
                bold: true,
                italic: true,
                strikethrough: true,
                inline_code: true,
            },
            hyperlink: Some(SafeHyperlink {
                kind: HyperlinkKind::Web,
                target: "https://example.com/deck".to_owned(),
            }),
        }],
    }
}

fn fixture() -> (Vec<u8>, DeckSpec, DeckTemplatePlan, DeckPlan) {
    let bytes = package();
    let text = SemanticNode {
        id: id(10),
        source: range(1, 10),
        role: SemanticRole::Prose,
        split: SplitPolicy::Never,
        content: SemanticContent::Text(rich("editable")),
    };
    let list_text = SemanticNode {
        id: id(12),
        source: range(22, 30),
        role: SemanticRole::Prose,
        split: SplitPolicy::Never,
        content: SemanticContent::Text(RichText {
            runs: vec![RichTextRun {
                text: "nested item".to_owned(),
                marks: Default::default(),
                hyperlink: None,
            }],
        }),
    };
    let nested_text = SemanticNode {
        id: id(14),
        source: range(31, 35),
        role: SemanticRole::Prose,
        split: SplitPolicy::Never,
        content: SemanticContent::Text(RichText {
            runs: vec![RichTextRun {
                text: "child".to_owned(),
                marks: Default::default(),
                hyperlink: None,
            }],
        }),
    };
    let list = SemanticNode {
        id: id(11),
        source: range(20, 40),
        role: SemanticRole::List,
        split: SplitPolicy::ListItems,
        content: SemanticContent::List(ListContent {
            ordered: true,
            start: 3,
            items: vec![ListItem {
                id: id(13),
                source: range(21, 39),
                blocks: vec![list_text],
                children: vec![ListContent {
                    ordered: false,
                    start: 1,
                    items: vec![ListItem {
                        id: id(15),
                        source: range(30, 38),
                        blocks: vec![nested_text],
                        children: vec![],
                    }],
                }],
            }],
        }),
    };
    let image = SemanticNode {
        id: id(20),
        source: range(41, 50),
        role: SemanticRole::Figure,
        split: SplitPolicy::Never,
        content: SemanticContent::Image(ImageContent {
            resource_id: id(40),
            alt_text: "Animated chart".to_owned(),
        }),
    };
    let svg = SemanticNode {
        id: id(21),
        source: range(51, 60),
        role: SemanticRole::Diagram,
        split: SplitPolicy::Never,
        content: SemanticContent::Svg(SvgContent {
            resource_id: id(41),
            source_text: Some("graph TD; A-->B".to_owned()),
        }),
    };
    let spec = DeckSpec {
        id: id(1),
        logical_slides: vec![LogicalSlide { id: id(2), source: range(0, 100), kind: LogicalSlideKind::Content, hidden: false, nodes: vec![text, list, image, svg] }],
        resources: vec![
            DeckResource { id: id(40), kind: ResourceKind::RasterImage, media_type: "image/gif".to_owned(), bytes: gif(), intrinsic_size: Some(PixelSize { width: 2, height: 1 }) },
            DeckResource { id: id(41), kind: ResourceKind::Svg, media_type: "image/svg+xml".to_owned(), bytes: br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><path d="M0 0L10 10"/></svg>"#.to_vec(), intrinsic_size: Some(PixelSize { width: 10, height: 10 }) },
        ],
    };
    let layout_id = id(50);
    let region_id = id(51);
    let template = DeckTemplatePlan {
        id: id(3),
        template_hash: Sha256::digest(&bytes).into(),
        cache_key: [7; 32],
        validator_version: 1,
        compiler_policy: "test".to_owned(),
        page_size: EmuSize {
            width: 10_000_000,
            height: 5_625_000,
        },
        theme: TemplateTheme::default(),
        layouts: vec![TemplateLayout {
            id: layout_id,
            role: TemplateLayoutRole::Content,
            matching_name: "wasmppt:content-v1".to_owned(),
            source_part: "ppt/slideLayouts/content.xml".to_owned(),
            master_part: "ppt/slideMasters/slideMaster1.xml".to_owned(),
            region_ids: vec![region_id],
            asset_ids: vec![],
            background: None,
        }],
        regions: vec![TemplateRegion {
            id: region_id,
            layout_id,
            role: RegionRole::Body,
            placeholder: PlaceholderIdentity {
                kind: "body".to_owned(),
                index: 1,
            },
            frame: EmuRect {
                x: 200_000,
                y: 200_000,
                width: 9_600_000,
                height: 5_200_000,
            },
            margins: TextMargins::default(),
            text_levels: vec![TemplateTextLevel {
                level: 0,
                font_size: Some(1_800),
                latin_typeface: Some("Aptos".to_owned()),
                color: Some(TemplateTextColor {
                    scheme: None,
                    rgb: 0x112233,
                }),
                ..Default::default()
            }],
            accepts: vec![
                SemanticRole::Prose,
                SemanticRole::List,
                SemanticRole::Figure,
                SemanticRole::Diagram,
            ],
            required: true,
        }],
        assets: vec![],
        diagnostics: vec![],
    };
    let frames = [
        EmuRect {
            x: 300_000,
            y: 300_000,
            width: 4_000_000,
            height: 600_000,
        },
        EmuRect {
            x: 300_000,
            y: 1_000_000,
            width: 4_000_000,
            height: 1_000_000,
        },
        EmuRect {
            x: 4_500_000,
            y: 300_000,
            width: 2_000_000,
            height: 1_500_000,
        },
        EmuRect {
            x: 6_700_000,
            y: 300_000,
            width: 2_000_000,
            height: 1_500_000,
        },
    ];
    let node_ids = [id(10), id(11), id(20), id(21)];
    let slices = [
        FragmentSlice::Whole,
        FragmentSlice::ListItems { start: 0, end: 1 },
        FragmentSlice::Whole,
        FragmentSlice::Whole,
    ];
    let fragments = node_ids
        .into_iter()
        .zip(slices)
        .zip(frames)
        .map(|((source_node_id, slice), frame)| PlannedFragment {
            id: PlannedFragment::expected_id(source_node_id, slice),
            source_node_id,
            slice,
            frame,
            type_choice: TypeChoice {
                font_size: if matches!(source_node_id, value if value == id(10) || value == id(11))
                {
                    1_800
                } else {
                    0
                },
                columns: 1,
                fit: if source_node_id == id(20) {
                    ContentFit::Cover
                } else {
                    ContentFit::Contain
                },
            },
            repeat_table_header_rows: 0,
        })
        .collect();
    let plan = DeckPlan {
        id: id(4),
        spec_id: spec.id,
        template_id: template.id,
        page_size: template.page_size,
        pages: vec![PhysicalPage {
            id: id(2).derive(b"physical-page", 1),
            logical_slide_id: id(2),
            template_layout_id: layout_id,
            hidden: false,
            continuation: Continuation {
                ordinal: 1,
                total: 1,
                repeated_heading_node_id: None,
                label: None,
            },
            regions: vec![PlannedRegion {
                template_region_id: region_id,
                frame: template.regions[0].frame,
                fragments,
            }],
        }],
        diagnostics: vec![],
    };
    (bytes, spec, template, plan)
}

#[test]
fn composes_editable_vector_and_first_frame_media_into_a_live_overlay() {
    let (bytes, spec, template, plan) = fixture();
    let report = validate_deck_plan(&spec, &template, &plan, &DeckLimits::default());
    assert!(report.is_valid(), "{:#?}", report.diagnostics);
    let overlay = DeckComposer
        .compose(
            Arc::<[u8]>::from(bytes),
            &spec,
            &template,
            &plan,
            &DeckLimits::default(),
            &ComposeLimits::default(),
        )
        .unwrap();
    let slide = String::from_utf8(overlay.read_part("ppt/slides/slide1.xml").unwrap()).unwrap();
    let rels = String::from_utf8(
        overlay
            .read_part("ppt/slides/_rels/slide1.xml.rels")
            .unwrap(),
    )
    .unwrap();
    assert!(slide.contains("<a:t>editable</a:t>"));
    assert!(slide.contains("b=\"1\"") && slide.contains("i=\"1\"") && slide.contains("sngStrike"));
    assert!(
        slide.contains("Courier New")
            && slide.contains("startAt=\"3\"")
            && slide.contains("lvl=\"1\"")
    );
    assert!(slide.contains("asvg:svgBlip") && slide.contains("descr=\"Animated chart\""));
    assert!(rels.contains("https://example.com/deck") && rels.contains("TargetMode=\"External\""));
    let gif_still_name = overlay
        .part_names()
        .into_iter()
        .find(|name| name.ends_with("-first-frame.png"))
        .unwrap();
    assert!(
        overlay
            .read_part(&gif_still_name)
            .unwrap()
            .starts_with(b"\x89PNG\r\n\x1a\n")
    );
    let svg_name = overlay
        .part_names()
        .into_iter()
        .find(|name| name.ends_with(".svg"))
        .unwrap();
    assert_eq!(
        overlay.read_part(&svg_name).unwrap(),
        spec.resources[1].bytes
    );
    assert_eq!(
        overlay.read_part("custom/opaque.bin").unwrap(),
        b"unknown-template-data"
    );
    assert!(overlay.stats().reused_source_bytes > 0);

    let direct = PresentationDocument::open_source(Arc::new(overlay.clone())).unwrap();
    assert_eq!(direct.slide_count(), 1);
    let mut cursor = overlay.generation_cursor();
    let mut exported = Vec::new();
    while !cursor.is_done() {
        exported.extend(cursor.pull(7).unwrap());
    }
    let reopened = PresentationDocument::open(exported.clone()).unwrap();
    assert_eq!(direct.slide_part_names(), reopened.slide_part_names());
    assert_eq!(reopened.slide_count(), 1);
    let direct_slide = direct.resolve_slide(0).unwrap();
    let reopened_slide = reopened.resolve_slide(0).unwrap();
    assert_eq!(direct_slide.slide, reopened_slide.slide);
    assert_eq!(direct_slide.diagnostics, reopened_slide.diagnostics);
    assert_eq!(
        direct.slide_dependency_fingerprint(0).unwrap(),
        reopened.slide_dependency_fingerprint(0).unwrap()
    );
    let archive = ZipArchive::from_bytes(exported).unwrap();
    let graph = PackageGraph::build(&archive).unwrap();
    assert!(!graph.diagnostics().iter().any(|diagnostic| matches!(
        diagnostic.code,
        wasmppt_opc::DiagnosticCode::MissingRelationshipTarget
            | wasmppt_opc::DiagnosticCode::InvalidRelationshipsXml
    )));
}

#[test]
fn composition_is_deterministic_and_rejects_template_drift_and_unsafe_links() {
    let (bytes, mut spec, template, plan) = fixture();
    let compose = |spec: &DeckSpec| {
        DeckComposer.compose(
            Arc::<[u8]>::from(bytes.clone()),
            spec,
            &template,
            &plan,
            &DeckLimits::default(),
            &ComposeLimits::default(),
        )
    };
    let first = compose(&spec).unwrap();
    let second = compose(&spec).unwrap();
    assert_eq!(first.revision(), second.revision());
    assert!(first.changed_parts_since(&second).is_empty());

    let SemanticContent::Text(text) = &mut spec.logical_slides[0].nodes[0].content else {
        unreachable!()
    };
    text.runs[0].hyperlink.as_mut().unwrap().target = "javascript:alert(1)".to_owned();
    assert_eq!(
        compose(&spec).unwrap_err().code(),
        ComposeErrorCode::InvalidContract
    );

    let (mut bytes, spec, template, plan) = fixture();
    bytes.push(0);
    let error = DeckComposer
        .compose(
            Arc::<[u8]>::from(bytes),
            &spec,
            &template,
            &plan,
            &DeckLimits::default(),
            &ComposeLimits::default(),
        )
        .unwrap_err();
    assert_eq!(error.code(), ComposeErrorCode::TemplateMismatch);
}
