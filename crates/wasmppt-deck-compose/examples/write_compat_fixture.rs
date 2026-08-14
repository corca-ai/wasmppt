use std::{env, fs, io::Write, path::PathBuf, sync::Arc};

#[path = "../../wasmppt-native/examples/support/dogfood_package.rs"]
mod dogfood_package;

use sha2::{Digest, Sha256};
use wasmppt_deck::{
    ChartContent, ChartKind, ChartSeries, ContentFit, Continuation, DeckLimits, DeckPlan, DeckSpec,
    DeckTemplatePlan, EmuRect, EmuSize, FragmentSlice, HyperlinkKind, LogicalSlide,
    LogicalSlideKind, PhysicalPage, PlaceholderIdentity, PlannedFragment, PlannedRegion,
    RegionRole, RichText, RichTextRun, SafeHyperlink, SemanticContent, SemanticNode, SemanticRole,
    SourceRange, SplitPolicy, StableId, TableCell, TableColumn, TableContent, TableRow,
    TemplateLayout, TemplateLayoutRole, TemplateRegion, TemplateTextColor, TemplateTextLevel,
    TemplateTheme, TextMargins, TextMarks, TypeChoice,
};
use wasmppt_deck_compose::{ComposeLimits, DeckComposer};

const EMPTY_SLIDE: &[u8] = br#"<p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/><a:chOff x="0" y="0"/><a:chExt cx="0" cy="0"/></a:xfrm></p:grpSpPr></p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sld>"#;

fn id(value: u8) -> StableId {
    let mut bytes = [0; 16];
    bytes[15] = value;
    StableId::from_bytes(bytes)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .expect("usage: write_compat_fixture OUTPUT.pptx");
    let template_path = output.with_extension("source.potx");
    dogfood_package::write_fixture(&template_path, EMPTY_SLIDE, EMPTY_SLIDE);
    let template_bytes: Arc<[u8]> = fs::read(&template_path)?.into();

    let slide_id = id(2);
    let node_id = id(3);
    let table_id = id(8);
    let chart_id = id(9);
    let layout_id = id(4);
    let region_id = id(5);
    let frame = EmuRect {
        x: 600_000,
        y: 500_000,
        width: 10_800_000,
        height: 5_800_000,
    };
    let plain = |value: u8, text: &str| TableCell {
        id: id(value),
        source: SourceRange::new("compat.md", u32::from(value) + 24, u32::from(value) + 25),
        content: RichText {
            runs: vec![RichTextRun {
                text: text.to_owned(),
                marks: TextMarks::default(),
                hyperlink: None,
            }],
        },
    };
    let spec = DeckSpec {
        id: id(1),
        logical_slides: vec![LogicalSlide {
            id: slide_id,
            source: SourceRange::new("compat.md", 0, 100),
            kind: LogicalSlideKind::Content,
            hidden: false,
            nodes: vec![
                SemanticNode {
                    id: node_id,
                    source: SourceRange::new("compat.md", 0, 32),
                    role: SemanticRole::Prose,
                    split: SplitPolicy::Never,
                    content: SemanticContent::Text(RichText {
                        runs: vec![RichTextRun {
                            text: "Editable wasmppt deck output".to_owned(),
                            marks: TextMarks {
                                bold: true,
                                italic: true,
                                ..TextMarks::default()
                            },
                            hyperlink: Some(SafeHyperlink {
                                kind: HyperlinkKind::Web,
                                target: "https://github.com/corca-ai/wasmppt".to_owned(),
                            }),
                        }],
                    }),
                },
                SemanticNode {
                    id: table_id,
                    source: SourceRange::new("compat.md", 33, 65),
                    role: SemanticRole::Table,
                    split: SplitPolicy::TableRows,
                    content: SemanticContent::Table(TableContent {
                        columns: vec![
                            TableColumn {
                                id: id(10),
                                source: SourceRange::new("compat.md", 34, 35),
                                alignment: wasmppt_deck::TableColumnAlignment::Start,
                            },
                            TableColumn {
                                id: id(11),
                                source: SourceRange::new("compat.md", 35, 36),
                                alignment: wasmppt_deck::TableColumnAlignment::End,
                            },
                        ],
                        header_rows: 1,
                        rows: vec![
                            TableRow {
                                id: id(12),
                                source: SourceRange::new("compat.md", 37, 39),
                                cells: vec![plain(13, "Quarter"), plain(14, "Revenue")],
                            },
                            TableRow {
                                id: id(15),
                                source: SourceRange::new("compat.md", 40, 55),
                                cells: vec![plain(16, "Q1"), plain(17, "42")],
                            },
                        ],
                    }),
                },
                SemanticNode {
                    id: chart_id,
                    source: SourceRange::new("compat.md", 66, 95),
                    role: SemanticRole::Chart,
                    split: SplitPolicy::Never,
                    content: SemanticContent::Chart(ChartContent {
                        kind: ChartKind::Column,
                        categories: vec!["Q1".to_owned(), "Q2".to_owned()],
                        series: vec![ChartSeries {
                            name: "Revenue".to_owned(),
                            values: vec![42.0, 64.0],
                        }],
                    }),
                },
            ],
        }],
        resources: vec![],
    };
    let template = DeckTemplatePlan {
        id: id(6),
        template_hash: Sha256::digest(&template_bytes).into(),
        cache_key: [0; 32],
        validator_version: 1,
        compiler_policy: "openxml-compatibility".to_owned(),
        page_size: EmuSize {
            width: 12_192_000,
            height: 6_858_000,
        },
        theme: TemplateTheme::default(),
        layouts: vec![TemplateLayout {
            id: layout_id,
            role: TemplateLayoutRole::Content,
            matching_name: "wasmppt:content-v1".to_owned(),
            source_part: "ppt/slideLayouts/slideLayout1.xml".to_owned(),
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
            frame,
            margins: TextMargins::default(),
            text_levels: vec![TemplateTextLevel {
                level: 0,
                font_size: Some(3_200),
                latin_typeface: Some("Aptos".to_owned()),
                color: Some(TemplateTextColor {
                    scheme: None,
                    rgb: 0x10_233d,
                }),
                ..TemplateTextLevel::default()
            }],
            accepts: vec![
                SemanticRole::Prose,
                SemanticRole::Table,
                SemanticRole::Chart,
            ],
            required: true,
        }],
        assets: vec![],
        diagnostics: vec![],
    };
    let slice = FragmentSlice::Whole;
    let plan = DeckPlan {
        id: id(7),
        spec_id: spec.id,
        template_id: template.id,
        page_size: template.page_size,
        pages: vec![PhysicalPage {
            id: slide_id.derive(b"physical-page", 1),
            logical_slide_id: slide_id,
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
                frame,
                fragments: vec![
                    PlannedFragment {
                        id: PlannedFragment::expected_id(node_id, slice),
                        source_node_id: node_id,
                        slice,
                        frame: EmuRect {
                            x: 600_000,
                            y: 500_000,
                            width: 10_800_000,
                            height: 800_000,
                        },
                        type_choice: TypeChoice {
                            font_size: 3_200,
                            fit: ContentFit::None,
                        },
                        repeat_table_header_rows: 0,
                    },
                    PlannedFragment {
                        id: PlannedFragment::expected_id(
                            table_id,
                            FragmentSlice::TableRows { start: 0, end: 2 },
                        ),
                        source_node_id: table_id,
                        slice: FragmentSlice::TableRows { start: 0, end: 2 },
                        frame: EmuRect {
                            x: 600_000,
                            y: 1_500_000,
                            width: 5_000_000,
                            height: 2_000_000,
                        },
                        type_choice: TypeChoice {
                            font_size: 1_600,
                            fit: ContentFit::None,
                        },
                        repeat_table_header_rows: 0,
                    },
                    PlannedFragment {
                        id: PlannedFragment::expected_id(chart_id, FragmentSlice::Whole),
                        source_node_id: chart_id,
                        slice: FragmentSlice::Whole,
                        frame: EmuRect {
                            x: 6_000_000,
                            y: 1_500_000,
                            width: 5_000_000,
                            height: 4_000_000,
                        },
                        type_choice: TypeChoice {
                            font_size: 0,
                            fit: ContentFit::Contain,
                        },
                        repeat_table_header_rows: 0,
                    },
                ],
            }],
        }],
        diagnostics: vec![],
    };
    let overlay = DeckComposer.compose(
        template_bytes,
        &spec,
        &template,
        &plan,
        &DeckLimits::default(),
        &ComposeLimits::default(),
    )?;
    let mut file = fs::File::create(output)?;
    let mut cursor = overlay.generation_cursor();
    while !cursor.is_done() {
        file.write_all(&cursor.pull(64 * 1024)?)?;
    }
    Ok(())
}
