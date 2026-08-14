use std::{borrow::Cow, sync::Arc};

use gif::{Encoder, Frame};
use sha2::{Digest, Sha256};
use wasmppt_deck::{
    ChartContent, ChartKind, ChartSeries, Continuation, DeckDiagnosticCode, DeckLimits, DeckPlan,
    DeckResource, DeckSpec, DeckTemplatePlan, EmuRect, EmuSize, FragmentSlice, HyperlinkKind,
    ImageContent, ListContent, ListItem, LogicalSlide, LogicalSlideKind, MediaPlacement,
    PhysicalPage, PixelSize, PlaceholderIdentity, PlannedFragment, PlannedRegion, RegionRole,
    ResourceKind, RichText, RichTextRun, SafeHyperlink, SemanticContent, SemanticNode,
    SemanticRole, SourceRange, SplitPolicy, StableId, SvgContent, TableCell, TableColumn,
    TableContent, TableRow, TemplateLayout, TemplateLayoutCapability, TemplateRegion,
    TemplateTextColor, TemplateTextLevel, TemplateTheme, TextMargins, TextMarks, TypeChoice,
    validate_deck_plan,
};
use wasmppt_deck_compose::{ComposeErrorCode, ComposeLimits, DeckComposer};
use wasmppt_display::{DisplayCommand, DisplayList};
use wasmppt_layout::{
    ChartKind as ResolvedChartKind, ElementKind, PresentationDocument, SourceLevel,
};
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
                "<Types xmlns=\"{CT}\"><Default Extension=\"xml\" ContentType=\"application/xml\"/><Default Extension=\"bin\" ContentType=\"application/octet-stream\"/><Override PartName=\"/ppt/presentation.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.template.main+xml\"/><Override PartName=\"/ppt/slideLayouts/content.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml\"/><Override PartName=\"/ppt/slides/slide1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.slide+xml\"/><Override PartName=\"/ppt/slides/slide2.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.slide+xml\"/><Override PartName=\"/ppt/notesSlides/notesSlide1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.notesSlide+xml\"/><Override PartName=\"/ppt/notesSlides/notesSlide2.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.notesSlide+xml\"/></Types>"
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
                "<p:presentation xmlns:p=\"{PML}\" xmlns:r=\"{OFFICE_REL}\"><p:sldIdLst><p:sldId id=\"256\" r:id=\"rId1\"/><p:sldId id=\"257\" r:id=\"rId2\"/></p:sldIdLst><p:sldSz cx=\"10000000\" cy=\"5625000\"/></p:presentation>"
            ),
        ),
        (
            "ppt/_rels/presentation.xml.rels",
            format!(
                "<Relationships xmlns=\"{REL}\"><Relationship Id=\"rId1\" Type=\"{OFFICE_REL}/slide\" Target=\"slides/slide1.xml\"/><Relationship Id=\"rId2\" Type=\"{OFFICE_REL}/slide\" Target=\"slides/slide2.xml\"/></Relationships>"
            ),
        ),
        (
            "ppt/slideLayouts/content.xml",
            format!(
                "<p:sldLayout xmlns:p=\"{PML}\" xmlns:a=\"{DRAWING}\"><p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/></p:spTree></p:cSld></p:sldLayout>"
            ),
        ),
        (
            "ppt/slides/slide1.xml",
            format!(
                "<p:sld xmlns:p=\"{PML}\"><p:cSld><p:spTree><p:nvGrpSpPr/><p:grpSpPr/></p:spTree></p:cSld></p:sld>"
            ),
        ),
        (
            "ppt/slides/_rels/slide1.xml.rels",
            format!(
                "<Relationships xmlns=\"{REL}\"><Relationship Id=\"rId1\" Type=\"{OFFICE_REL}/slideLayout\" Target=\"../slideLayouts/content.xml\"/><Relationship Id=\"rId2\" Type=\"{OFFICE_REL}/notesSlide\" Target=\"../notesSlides/notesSlide1.xml\"/></Relationships>"
            ),
        ),
        (
            "ppt/notesSlides/notesSlide1.xml",
            format!(
                "<p:notes xmlns:p=\"{PML}\"><p:cSld><p:spTree><p:nvGrpSpPr/><p:grpSpPr/></p:spTree></p:cSld></p:notes>"
            ),
        ),
        (
            "ppt/notesSlides/_rels/notesSlide1.xml.rels",
            format!(
                "<Relationships xmlns=\"{REL}\"><Relationship Id=\"rId1\" Type=\"{OFFICE_REL}/slide\" Target=\"../slides/slide1.xml\"/></Relationships>"
            ),
        ),
        (
            "ppt/slides/slide2.xml",
            format!(
                "<p:sld xmlns:p=\"{PML}\"><p:cSld><p:spTree><p:nvGrpSpPr/><p:grpSpPr/></p:spTree></p:cSld></p:sld>"
            ),
        ),
        (
            "ppt/slides/_rels/slide2.xml.rels",
            format!(
                "<Relationships xmlns=\"{REL}\"><Relationship Id=\"rId1\" Type=\"{OFFICE_REL}/slideLayout\" Target=\"../slideLayouts/content.xml\"/><Relationship Id=\"rId2\" Type=\"{OFFICE_REL}/notesSlide\" Target=\"../notesSlides/notesSlide2.xml\"/></Relationships>"
            ),
        ),
        (
            "ppt/notesSlides/notesSlide2.xml",
            format!(
                "<p:notes xmlns:p=\"{PML}\"><p:cSld><p:spTree><p:nvGrpSpPr/><p:grpSpPr/></p:spTree></p:cSld></p:notes>"
            ),
        ),
        (
            "ppt/notesSlides/_rels/notesSlide2.xml.rels",
            format!(
                "<Relationships xmlns=\"{REL}\"><Relationship Id=\"rId1\" Type=\"{OFFICE_REL}/slide\" Target=\"../slides/slide2.xml\"/></Relationships>"
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
        logical_slides: vec![LogicalSlide { id: id(2), source: range(0, 100), kind: LogicalSlideKind::Content, hidden: false, nodes: vec![text, list, image, svg], media_text_relations: Vec::new() }],
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
            capability: TemplateLayoutCapability::ContentEnvelope,
            matching_name: "wasmppt:content-envelope-v3".to_owned(),
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
            bleed_frame: None,
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
            x: 6_950_000,
            y: 300_000,
            width: 1_500_000,
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
        .map(|((source_node_id, slice), frame)| {
            let media = if source_node_id == id(20) {
                MediaPlacement::cover(frame, PixelSize { width: 2, height: 1 })
            } else if source_node_id == id(21) {
                MediaPlacement::contain(
                    EmuRect {
                        x: 6_700_000,
                        y: 300_000,
                        width: 2_000_000,
                        height: 1_500_000,
                    },
                    PixelSize { width: 10, height: 10 },
                )
            } else {
                None
            };
            PlannedFragment {
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
            },
            media,
            repeat_table_header_rows: 0,
        }})
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
            topology: wasmppt_deck::TopologyChoice::stack(),
            hidden: false,
            continuation: Continuation {
                ordinal: 1,
                total: 1,
                repeated_heading_node_id: None,
                label: None,
            },
            regions: vec![PlannedRegion {
                template_region_id: region_id,
                placement: wasmppt_deck::RegionPlacement::Slot(0),
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
    let covered_gif = slide
        .split("<p:pic>")
        .nth(1)
        .and_then(|tail| tail.split("</p:pic>").next())
        .expect("covered GIF picture");
    assert!(covered_gif.contains("<a:srcRect l=\"16667\" t=\"0\" r=\"16667\" b=\"0\"/>"));
    let contained_svg = slide
        .rsplit("<p:pic>")
        .next()
        .and_then(|tail| tail.split("</p:pic>").next())
        .expect("contained SVG picture");
    assert!(
        contained_svg.contains("<a:off x=\"6950000\" y=\"300000\"/>")
            && contained_svg.contains("<a:ext cx=\"1500000\" cy=\"1500000\"/>")
            && contained_svg.contains("<a:srcRect/>")
    );
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
    assert!(
        overlay
            .part_names()
            .iter()
            .all(|name| !name.starts_with("ppt/notesSlides/"))
    );
    let content_types =
        String::from_utf8(overlay.read_part("[Content_Types].xml").unwrap()).unwrap();
    assert!(!content_types.contains("notesSlide"));
    assert!(overlay.stats().reused_source_bytes > 0);

    let direct = PresentationDocument::open_source(Arc::new(overlay.clone())).unwrap();
    assert_eq!(direct.slide_count(), 1);
    assert_fragment_geometry(&direct, &plan);
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
    let crops = direct_slide
        .slide
        .elements
        .iter()
        .filter_map(|element| match element.kind {
            ElementKind::Image { crop, .. } => Some(crop),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(crops[0].left, 16_667);
    assert_eq!(crops[0].right, 16_667);
    assert_eq!(crops[1], wasmppt_layout::ImageCrop::default());
    let display = DisplayList::from_slide(&direct_slide.slide);
    let display_crops = display
        .commands
        .iter()
        .filter_map(|command| match command {
            DisplayCommand::DrawImage { crop, .. } => Some(*crop),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(display_crops, crops);
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
fn validation_rejects_drifted_resolved_media_geometry() {
    let (_, spec, template, plan) = fixture();

    let mut contain_drift = plan.clone();
    contain_drift.pages[0].regions[0].fragments[3]
        .media
        .as_mut()
        .unwrap()
        .visible_frame
        .width += 1;
    let contain_report =
        validate_deck_plan(&spec, &template, &contain_drift, &DeckLimits::default());
    assert!(
        contain_report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == DeckDiagnosticCode::PLAN_INVALID_GEOMETRY)
    );

    let mut cover_drift = plan;
    cover_drift.pages[0].regions[0].fragments[2]
        .media
        .as_mut()
        .unwrap()
        .crop
        .as_mut()
        .unwrap()
        .left += 1;
    let cover_report = validate_deck_plan(&spec, &template, &cover_drift, &DeckLimits::default());
    assert!(
        cover_report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == DeckDiagnosticCode::PLAN_INVALID_GEOMETRY)
    );
}

#[test]
fn composes_an_empty_list_item_as_an_editable_bullet_paragraph() {
    let (bytes, mut spec, template, mut plan) = fixture();
    let SemanticContent::List(list) = &mut spec.logical_slides[0].nodes[1].content else {
        unreachable!()
    };
    list.items.push(ListItem {
        id: id(16),
        source: range(39, 40),
        blocks: vec![],
        children: vec![],
    });
    let fragment = &mut plan.pages[0].regions[0].fragments[1];
    fragment.slice = FragmentSlice::ListItems { start: 0, end: 2 };
    fragment.id = PlannedFragment::expected_id(fragment.source_node_id, fragment.slice);

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

    assert!(slide.contains("<a:buAutoNum type=\"arabicPeriod\" startAt=\"4\"/>"));
    assert!(slide.contains("startAt=\"4\"/></a:pPr><a:endParaRPr/></a:p>"));
    assert_eq!(slide.matches("name=\"List\"").count(), 1);
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

#[test]
fn composes_split_editable_table_and_chart_with_live_export_parity() {
    let (bytes, mut spec, mut template, mut plan) = fixture();
    let cell = |value: u8, text: &str| TableCell {
        id: id(value),
        source: range(u32::from(value), u32::from(value) + 1),
        content: rich(text),
    };
    let table_id = id(70);
    let table = SemanticNode {
        id: table_id,
        source: range(100, 160),
        role: SemanticRole::Table,
        split: SplitPolicy::TableRows,
        content: SemanticContent::Table(TableContent {
            columns: vec![
                TableColumn {
                    id: id(71),
                    source: range(101, 102),
                    alignment: wasmppt_deck::TableColumnAlignment::Start,
                },
                TableColumn {
                    id: id(72),
                    source: range(102, 103),
                    alignment: wasmppt_deck::TableColumnAlignment::End,
                },
            ],
            header_rows: 1,
            rows: vec![
                TableRow {
                    id: id(73),
                    source: range(110, 120),
                    cells: vec![cell(74, "Quarter"), cell(75, "Revenue")],
                },
                TableRow {
                    id: id(76),
                    source: range(121, 130),
                    cells: vec![cell(77, "Q1"), cell(78, "12.5")],
                },
                TableRow {
                    id: id(79),
                    source: range(131, 140),
                    cells: vec![cell(80, "Q2"), cell(81, "24.0")],
                },
            ],
        }),
    };
    let chart_id = id(82);
    let chart = SemanticNode {
        id: chart_id,
        source: range(161, 180),
        role: SemanticRole::Chart,
        split: SplitPolicy::Never,
        content: SemanticContent::Chart(ChartContent {
            kind: ChartKind::Column,
            categories: vec!["Q1".to_owned(), "Q2".to_owned()],
            series: vec![ChartSeries {
                name: "Revenue".to_owned(),
                values: vec![12.5, 24.0],
            }],
        }),
    };
    spec.logical_slides[0].nodes = vec![table, chart];
    spec.logical_slides[0].source = range(0, 200);
    template.regions[0].accepts = vec![SemanticRole::Table, SemanticRole::Chart];
    template.theme.colors = vec![
        wasmppt_deck::ThemeColor {
            slot: "accent1".to_owned(),
            rgb: 0x12_3456,
        },
        wasmppt_deck::ThemeColor {
            slot: "lt1".to_owned(),
            rgb: 0xff_ffff,
        },
        wasmppt_deck::ThemeColor {
            slot: "lt2".to_owned(),
            rgb: 0xee_eeee,
        },
        wasmppt_deck::ThemeColor {
            slot: "dk1".to_owned(),
            rgb: 0x22_2222,
        },
    ];
    let table_first = PlannedFragment {
        id: PlannedFragment::expected_id(table_id, FragmentSlice::TableRows { start: 0, end: 2 }),
        source_node_id: table_id,
        slice: FragmentSlice::TableRows { start: 0, end: 2 },
        frame: EmuRect {
            x: 400_000,
            y: 400_000,
            width: 9_000_000,
            height: 2_000_000,
        },
        type_choice: TypeChoice { font_size: 1_600 },
        media: None,
        repeat_table_header_rows: 0,
    };
    let table_second = PlannedFragment {
        id: PlannedFragment::expected_id(table_id, FragmentSlice::TableRows { start: 2, end: 3 }),
        source_node_id: table_id,
        slice: FragmentSlice::TableRows { start: 2, end: 3 },
        frame: EmuRect {
            x: 400_000,
            y: 400_000,
            width: 9_000_000,
            height: 1_500_000,
        },
        type_choice: TypeChoice { font_size: 1_600 },
        media: None,
        repeat_table_header_rows: 1,
    };
    let chart_fragment = PlannedFragment {
        id: PlannedFragment::expected_id(chart_id, FragmentSlice::Whole),
        source_node_id: chart_id,
        slice: FragmentSlice::Whole,
        frame: EmuRect {
            x: 400_000,
            y: 2_100_000,
            width: 9_000_000,
            height: 3_000_000,
        },
        type_choice: TypeChoice { font_size: 0 },
        media: None,
        repeat_table_header_rows: 0,
    };
    let slide_id = spec.logical_slides[0].id;
    let layout_id = template.layouts[0].id;
    let region_id = template.regions[0].id;
    plan.pages = vec![
        PhysicalPage {
            id: slide_id.derive(b"physical-page", 1),
            logical_slide_id: slide_id,
            template_layout_id: layout_id,
            topology: wasmppt_deck::TopologyChoice::stack(),
            hidden: false,
            continuation: Continuation {
                ordinal: 1,
                total: 2,
                repeated_heading_node_id: None,
                label: Some("1/2".to_owned()),
            },
            regions: vec![PlannedRegion {
                template_region_id: region_id,
                placement: wasmppt_deck::RegionPlacement::Slot(0),
                frame: template.regions[0].frame,
                fragments: vec![table_first],
            }],
        },
        PhysicalPage {
            id: slide_id.derive(b"physical-page", 2),
            logical_slide_id: slide_id,
            template_layout_id: layout_id,
            topology: wasmppt_deck::TopologyChoice::stack(),
            hidden: false,
            continuation: Continuation {
                ordinal: 2,
                total: 2,
                repeated_heading_node_id: None,
                label: Some("2/2".to_owned()),
            },
            regions: vec![PlannedRegion {
                template_region_id: region_id,
                placement: wasmppt_deck::RegionPlacement::Slot(0),
                frame: template.regions[0].frame,
                fragments: vec![table_second, chart_fragment],
            }],
        },
    ];
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
    let slide_two = String::from_utf8(overlay.read_part("ppt/slides/slide2.xml").unwrap()).unwrap();
    assert!(!slide_two.contains("Continuation marker"));
    assert!(!slide_two.contains("<a:t>2/2</a:t>"));
    assert_eq!(slide_two.matches("<a:t>Quarter</a:t>").count(), 1);
    assert_eq!(slide_two.matches("<a:t>Q2</a:t>").count(), 1);
    assert_eq!(slide_two.matches("<a:tbl>").count(), 1);
    assert!(slide_two.contains("algn=\"l\"") && slide_two.contains("algn=\"r\""));
    let grid_widths = slide_two
        .match_indices("<a:gridCol w=\"")
        .filter_map(|(offset, marker)| {
            let value = &slide_two[offset + marker.len()..];
            value.split_once('\"')?.0.parse::<i64>().ok()
        })
        .collect::<Vec<_>>();
    assert_eq!(grid_widths.len(), 2);
    assert_ne!(grid_widths[0], grid_widths[1]);
    assert!(slide_two.contains("val=\"123456\"") && slide_two.contains("<c:chart"));

    let direct = PresentationDocument::open_source(Arc::new(overlay.clone())).unwrap();
    assert_fragment_geometry(&direct, &plan);
    let first_slide = direct.resolve_slide(0).unwrap();
    let first_table = first_slide
        .slide
        .elements
        .iter()
        .find_map(|element| match &element.kind {
            ElementKind::Table { table } => Some(table),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        first_table
            .rows
            .iter()
            .map(|row| row.cells[0].text.as_str())
            .collect::<Vec<_>>(),
        ["Quarter", "Q1"]
    );
    let direct_slide = direct.resolve_slide(1).unwrap();
    let resolved_table = direct_slide
        .slide
        .elements
        .iter()
        .find_map(|element| match &element.kind {
            ElementKind::Table { table } => Some(table),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        resolved_table
            .rows
            .iter()
            .map(|row| row.cells[0].text.as_str())
            .collect::<Vec<_>>(),
        ["Quarter", "Q2"]
    );
    assert!(resolved_table.rows[0].cells[0].text_frame.is_some());
    let resolved_chart = direct_slide
        .slide
        .elements
        .iter()
        .find_map(|element| match &element.kind {
            ElementKind::Chart { chart } => Some(chart),
            _ => None,
        })
        .unwrap();
    assert_eq!(resolved_chart.kind, ResolvedChartKind::Column);
    assert_eq!(resolved_chart.series[0].categories, ["Q1", "Q2"]);
    assert_eq!(resolved_chart.series[0].values, [12.5, 24.0]);
    assert!(
        resolved_chart
            .embedded_workbook
            .as_deref()
            .is_some_and(|name| name.ends_with(".xlsx"))
    );

    let mut cursor = overlay.generation_cursor();
    let mut exported = Vec::new();
    while !cursor.is_done() {
        exported.extend(cursor.pull(31).unwrap());
    }
    let reopened = PresentationDocument::open(exported.clone()).unwrap();
    assert_eq!(direct_slide.slide, reopened.resolve_slide(1).unwrap().slide);
    let package = ZipArchive::from_bytes(exported).unwrap();
    let workbook_name = package
        .part_names()
        .into_iter()
        .find(|name| name.starts_with("ppt/embeddings/deck-"))
        .unwrap();
    let workbook = ZipArchive::from_bytes(package.read_part(&workbook_name).unwrap()).unwrap();
    let sheet = String::from_utf8(workbook.read_part("xl/worksheets/sheet1.xml").unwrap()).unwrap();
    assert!(sheet.contains("Q2") && sheet.contains(">24<"));
}

fn assert_fragment_geometry(document: &PresentationDocument, plan: &DeckPlan) {
    for (slide_index, page) in plan.pages.iter().enumerate() {
        let resolved = document.resolve_slide(slide_index).unwrap();
        let expected = page
            .regions
            .iter()
            .flat_map(|region| {
                region.fragments.iter().map(|fragment| {
                    let frame = fragment.frame;
                    (frame.x, frame.y, frame.width, frame.height)
                })
            })
            .collect::<Vec<_>>();
        let skip = usize::from(page.continuation.repeated_heading_node_id.is_some());
        let actual = resolved
            .slide
            .elements
            .iter()
            .filter(|element| element.source == SourceLevel::Slide)
            .skip(skip)
            .take(expected.len())
            .map(|element| {
                let frame = element.transform.bounds;
                (
                    frame.origin.x,
                    frame.origin.y,
                    frame.size.width,
                    frame.size.height,
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(actual, expected, "slide {slide_index} geometry drifted");
    }
}
