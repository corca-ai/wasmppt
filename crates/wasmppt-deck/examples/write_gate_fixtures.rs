use std::{collections::BTreeSet, env, fs, path::PathBuf};

use wasmppt_deck::{
    ChartContent, ChartKind, ChartSeries, CodeContent, DeckLimits, DeckResource, DeckSpec,
    HyperlinkKind, ImageContent, ListContent, ListItem, LogicalSlide, LogicalSlideKind, PixelSize,
    ResourceKind, RichText, RichTextRun, SafeHyperlink, SemanticContent, SemanticNode,
    SemanticRole, SourceRange, SplitPolicy, StableId, SvgContent, TableCell, TableColumn,
    TableContent, TableRow, TextMarks, validate_deck_spec,
};
use wasmppt_opc::{CompressionMethod, EntryOptions, VecSink, ZipWriter};

const CT: &str = "http://schemas.openxmlformats.org/package/2006/content-types";
const REL: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const OFFICE_REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const PML: &str = "http://schemas.openxmlformats.org/presentationml/2006/main";
const DRAWING: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .expect("usage: write_gate_fixtures OUTPUT_DIRECTORY");
    fs::create_dir_all(&output)?;
    fs::write(output.join("starter.potx"), starter())?;
    let spec = deck_spec();
    let report = validate_deck_spec(&spec, &DeckLimits::default());
    if !report.is_valid() {
        return Err(format!("invalid gate DeckSpec: {:?}", report.diagnostics).into());
    }
    fs::write(
        output.join("deck-spec.wdsf"),
        spec.encode(&DeckLimits::default())?,
    )?;
    assert_supported_roles(&spec)?;
    fs::write(
        output.join("atomic-overflow.wdsf"),
        atomic_overflow_spec().encode(&DeckLimits::default())?,
    )?;
    Ok(())
}

fn starter() -> Vec<u8> {
    let options = EntryOptions::deterministic(CompressionMethod::Deflate);
    let mut writer = ZipWriter::new(VecSink::new());
    let entries = [
        (
            "[Content_Types].xml",
            format!(
                r#"<Types xmlns="{CT}"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Default Extension="png" ContentType="image/png"/><Override PartName="/ppt/presentation.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.template.main+xml"/><Override PartName="/ppt/slideMasters/slideMaster1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideMaster+xml"/><Override PartName="/ppt/slideLayouts/title.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml"/><Override PartName="/ppt/slideLayouts/content.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml"/><Override PartName="/ppt/slideLayouts/statement.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml"/><Override PartName="/ppt/theme/theme1.xml" ContentType="application/vnd.openxmlformats-officedocument.theme+xml"/></Types>"#,
            ),
        ),
        (
            "_rels/.rels",
            format!(
                r#"<Relationships xmlns="{REL}"><Relationship Id="rId1" Type="{OFFICE_REL}/officeDocument" Target="ppt/presentation.xml"/></Relationships>"#,
            ),
        ),
        (
            "ppt/presentation.xml",
            format!(
                r#"<p:presentation xmlns:p="{PML}" xmlns:r="{OFFICE_REL}"><p:sldMasterIdLst><p:sldMasterId id="2147483648" r:id="master"/></p:sldMasterIdLst><p:sldSz cx="12192000" cy="6858000"/><p:notesSz cx="6858000" cy="9144000"/></p:presentation>"#,
            ),
        ),
        (
            "ppt/_rels/presentation.xml.rels",
            format!(
                r#"<Relationships xmlns="{REL}"><Relationship Id="master" Type="{OFFICE_REL}/slideMaster" Target="slideMasters/slideMaster1.xml"/></Relationships>"#,
            ),
        ),
        (
            "ppt/slideMasters/slideMaster1.xml",
            format!(
                r#"<p:sldMaster xmlns:p="{PML}" xmlns:a="{DRAWING}" xmlns:r="{OFFICE_REL}"><p:cSld><p:bg><p:bgPr><a:solidFill><a:schemeClr val="lt1"/></a:solidFill><a:effectLst/></p:bgPr></p:bg><p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/><a:chOff x="0" y="0"/><a:chExt cx="0" cy="0"/></a:xfrm></p:grpSpPr>{}</p:spTree></p:cSld><p:clrMap bg1="lt1" tx1="dk1" bg2="lt2" tx2="dk2" accent1="accent1" accent2="accent2" accent3="accent3" accent4="accent4" accent5="accent5" accent6="accent6" hlink="hlink" folHlink="folHlink"/><p:sldLayoutIdLst><p:sldLayoutId id="2147483649" r:id="title"/><p:sldLayoutId id="2147483650" r:id="content"/><p:sldLayoutId id="2147483651" r:id="statement"/></p:sldLayoutIdLst><p:txStyles><p:titleStyle><a:lvl1pPr><a:defRPr sz="3200" b="1"><a:latin typeface="+mj-lt"/><a:ea typeface="+mj-ea"/><a:cs typeface="+mj-cs"/></a:defRPr></a:lvl1pPr></p:titleStyle><p:bodyStyle><a:lvl1pPr marL="0" indent="0"><a:defRPr sz="1800"><a:latin typeface="+mn-lt"/><a:ea typeface="+mn-ea"/><a:cs typeface="+mn-cs"/></a:defRPr></a:lvl1pPr></p:bodyStyle><p:otherStyle/></p:txStyles></p:sldMaster>"#,
                master_placeholders(),
            ),
        ),
        (
            "ppt/slideMasters/_rels/slideMaster1.xml.rels",
            format!(
                r#"<Relationships xmlns="{REL}"><Relationship Id="title" Type="{OFFICE_REL}/slideLayout" Target="../slideLayouts/title.xml"/><Relationship Id="content" Type="{OFFICE_REL}/slideLayout" Target="../slideLayouts/content.xml"/><Relationship Id="statement" Type="{OFFICE_REL}/slideLayout" Target="../slideLayouts/statement.xml"/><Relationship Id="theme" Type="{OFFICE_REL}/theme" Target="../theme/theme1.xml"/></Relationships>"#,
            ),
        ),
        (
            "ppt/slideLayouts/title.xml",
            layout(
                "wasmppt:title-v1",
                &[placeholder(11, "title", 1), placeholder(12, "subTitle", 2)],
            ),
        ),
        (
            "ppt/slideLayouts/content.xml",
            layout(
                "wasmppt:content-v1",
                &[
                    placeholder(21, "title", 3),
                    placeholder(22, "body", 4),
                    placeholder(23, "sldNum", 6),
                ],
            ),
        ),
        (
            "ppt/slideLayouts/statement.xml",
            layout("wasmppt:statement-v1", &[placeholder(31, "ctrTitle", 5)]),
        ),
        (
            "ppt/slideLayouts/_rels/title.xml.rels",
            layout_relationships(),
        ),
        (
            "ppt/slideLayouts/_rels/content.xml.rels",
            layout_relationships(),
        ),
        (
            "ppt/slideLayouts/_rels/statement.xml.rels",
            layout_relationships(),
        ),
        (
            "ppt/theme/theme1.xml",
            format!(
                r#"<a:theme xmlns:a="{DRAWING}" name="wasmppt Gate"><a:themeElements><a:clrScheme name="Gate"><a:dk1><a:srgbClr val="111827"/></a:dk1><a:lt1><a:srgbClr val="FFFFFF"/></a:lt1><a:dk2><a:srgbClr val="1F2937"/></a:dk2><a:lt2><a:srgbClr val="F3F4F6"/></a:lt2><a:accent1><a:srgbClr val="2563EB"/></a:accent1><a:accent2><a:srgbClr val="0F766E"/></a:accent2><a:accent3><a:srgbClr val="D97706"/></a:accent3><a:accent4><a:srgbClr val="7C3AED"/></a:accent4><a:accent5><a:srgbClr val="DB2777"/></a:accent5><a:accent6><a:srgbClr val="4B5563"/></a:accent6><a:hlink><a:srgbClr val="2563EB"/></a:hlink><a:folHlink><a:srgbClr val="7C3AED"/></a:folHlink></a:clrScheme><a:fontScheme name="Gate"><a:majorFont><a:latin typeface="Aptos Display"/><a:ea typeface="Noto Sans CJK KR"/><a:cs typeface="Arial"/></a:majorFont><a:minorFont><a:latin typeface="Aptos"/><a:ea typeface="Noto Sans CJK KR"/><a:cs typeface="Arial"/></a:minorFont></a:fontScheme><a:fmtScheme name="Gate"><a:fillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:fillStyleLst><a:lnStyleLst><a:ln w="6350"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln><a:ln w="12700"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln><a:ln w="19050"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln></a:lnStyleLst><a:effectStyleLst><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle></a:effectStyleLst><a:bgFillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:bgFillStyleLst></a:fmtScheme></a:themeElements></a:theme>"#,
            ),
        ),
    ];
    for (name, value) in entries {
        writer
            .write_entry(name, value.as_bytes(), &options)
            .expect("gate Starter entry must be writable");
    }
    writer
        .finish()
        .expect("gate Starter must finish")
        .0
        .into_inner()
}

fn master_placeholders() -> String {
    [
        master_placeholder(2, "title", 1, (800_000, 500_000, 10_592_000, 900_000)),
        master_placeholder(
            3,
            "subTitle",
            2,
            (1_000_000, 1_700_000, 10_192_000, 900_000),
        ),
        master_placeholder(4, "title", 3, (600_000, 300_000, 10_992_000, 700_000)),
        master_placeholder(5, "body", 4, (700_000, 1_200_000, 10_792_000, 5_100_000)),
        master_placeholder(
            6,
            "ctrTitle",
            5,
            (1_000_000, 1_600_000, 10_192_000, 2_200_000),
        ),
        master_placeholder(7, "sldNum", 6, (11_000_000, 6_300_000, 500_000, 200_000)),
    ]
    .join("")
}

fn master_placeholder(id: u32, kind: &str, index: u32, frame: (i64, i64, i64, i64)) -> String {
    let (x, y, width, height) = frame;
    format!(
        r#"<p:sp><p:nvSpPr><p:cNvPr id="{id}" name="{kind}"/><p:cNvSpPr/><p:nvPr><p:ph type="{kind}" idx="{index}"/></p:nvPr></p:nvSpPr><p:spPr><a:xfrm><a:off x="{x}" y="{y}"/><a:ext cx="{width}" cy="{height}"/></a:xfrm></p:spPr><p:txBody><a:bodyPr lIns="90000" tIns="45000" rIns="90000" bIns="45000"/><a:lstStyle/><a:p/></p:txBody></p:sp>"#,
    )
}

fn placeholder(id: u32, kind: &str, index: u32) -> String {
    format!(
        r#"<p:sp><p:nvSpPr><p:cNvPr id="{id}" name="{kind}"/><p:cNvSpPr/><p:nvPr><p:ph type="{kind}" idx="{index}"/></p:nvPr></p:nvSpPr><p:spPr/><p:txBody><a:bodyPr/><a:lstStyle/><a:p/></p:txBody></p:sp>"#,
    )
}

fn layout(matching_name: &str, placeholders: &[String]) -> String {
    format!(
        r#"<p:sldLayout xmlns:p="{PML}" xmlns:a="{DRAWING}" matchingName="{matching_name}" type="obj" preserve="1"><p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/><a:chOff x="0" y="0"/><a:chExt cx="0" cy="0"/></a:xfrm></p:grpSpPr>{}</p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sldLayout>"#,
        placeholders.join("")
    )
}

fn layout_relationships() -> String {
    format!(
        r#"<Relationships xmlns="{REL}"><Relationship Id="master" Type="{OFFICE_REL}/slideMaster" Target="../slideMasters/slideMaster1.xml"/></Relationships>"#,
    )
}

fn deck_spec() -> DeckSpec {
    let png = id(230);
    let gif = id(231);
    let svg = id(232);
    let math = id(233);
    let slides = vec![
        slide(
            1,
            LogicalSlideKind::Title,
            false,
            vec![
                text_node(
                    2,
                    SemanticRole::Title,
                    "Deck gate: 한국어 / العربية",
                    SplitPolicy::Never,
                ),
                text_node(
                    3,
                    SemanticRole::Subtitle,
                    "Rich semantics and deterministic hosts",
                    SplitPolicy::Never,
                ),
            ],
        ),
        slide(
            10,
            LogicalSlideKind::Content,
            false,
            vec![
                text_node(
                    11,
                    SemanticRole::Title,
                    "Rich text and nested lists",
                    SplitPolicy::Never,
                ),
                rich_node(12),
                list_node(20),
            ],
        ),
        slide(
            40,
            LogicalSlideKind::Content,
            false,
            vec![
                text_node(
                    41,
                    SemanticRole::Section,
                    "Raster, GIF, SVG, and gallery",
                    SplitPolicy::Never,
                ),
                image_node(42, SemanticRole::Figure, png, "one-pixel PNG"),
                text_node(
                    43,
                    SemanticRole::Caption,
                    "Loss-aware media caption",
                    SplitPolicy::Never,
                ),
                gallery_node(44, gif, svg),
            ],
        ),
        slide(
            70,
            LogicalSlideKind::Content,
            false,
            vec![
                text_node(
                    71,
                    SemanticRole::Title,
                    "Tables paginate with repeated headers",
                    SplitPolicy::Never,
                ),
                table_node(72),
            ],
        ),
        slide(
            132,
            LogicalSlideKind::Content,
            false,
            vec![
                text_node(
                    133,
                    SemanticRole::Title,
                    "Charts and code",
                    SplitPolicy::Never,
                ),
                chart_node(134),
                code_node(135),
            ],
        ),
        slide(
            140,
            LogicalSlideKind::Content,
            false,
            vec![
                text_node(141, SemanticRole::Title, "Diagrams", SplitPolicy::Never),
                svg_node(142, SemanticRole::Diagram, svg, Some("flowchart TD")),
            ],
        ),
        slide(
            145,
            LogicalSlideKind::Content,
            false,
            vec![svg_node(
                146,
                SemanticRole::DisplayMath,
                math,
                Some("x^2 + y^2"),
            )],
        ),
        slide(150, LogicalSlideKind::Content, false, vec![quote_node(151)]),
        slide(
            160,
            LogicalSlideKind::Content,
            false,
            vec![
                text_node(161, SemanticRole::Title, "Definitions", SplitPolicy::Never),
                definition_node(165),
            ],
        ),
        slide(
            175,
            LogicalSlideKind::Content,
            false,
            vec![text_node(
                176,
                SemanticRole::Statement,
                "Every host owns the same immutable plan.",
                SplitPolicy::Never,
            )],
        ),
        slide(
            180,
            LogicalSlideKind::Content,
            true,
            vec![
                text_node(
                    181,
                    SemanticRole::Title,
                    "Hidden authoring page",
                    SplitPolicy::Never,
                ),
                text_node(
                    182,
                    SemanticRole::Prose,
                    "Presentable indices omit this page.",
                    SplitPolicy::Never,
                ),
            ],
        ),
    ];
    DeckSpec {
        id: id(255),
        logical_slides: slides,
        resources: vec![
            DeckResource {
                id: png,
                kind: ResourceKind::RasterImage,
                media_type: "image/png".to_owned(),
                bytes: vec![
                    137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0,
                    0, 1, 0, 0, 0, 1, 8, 6, 0, 0, 0, 31, 21, 196, 137, 0, 0, 0, 13,
                    73, 68, 65, 84, 8, 215, 99, 248, 207, 192, 240, 31, 0, 5, 0, 1, 255,
                    137, 153, 61, 29, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
                ],
                intrinsic_size: Some(PixelSize { width: 1, height: 1 }),
            },
            DeckResource {
                id: gif,
                kind: ResourceKind::RasterImage,
                media_type: "image/gif".to_owned(),
                bytes: b"GIF89a\x01\0\x01\0\x80\0\0\0\0\0\xff\xff\xff!\xf9\x04\x01\0\0\0\0,\0\0\0\0\x01\0\x01\0\0\x02\x02D\x01\0;".to_vec(),
                intrinsic_size: Some(PixelSize { width: 1, height: 1 }),
            },
            DeckResource {
                id: svg,
                kind: ResourceKind::Svg,
                media_type: "image/svg+xml".to_owned(),
                bytes: br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 9"><path fill="#2563eb" d="M0 0h16v9H0z"/></svg>"##.to_vec(),
                intrinsic_size: Some(PixelSize { width: 16, height: 9 }),
            },
            DeckResource {
                id: math,
                kind: ResourceKind::Svg,
                media_type: "image/svg+xml".to_owned(),
                bytes: br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 9"><path d="M1 8L8 1l7 7"/></svg>"#.to_vec(),
                intrinsic_size: Some(PixelSize { width: 16, height: 9 }),
            },
        ],
    }
}

fn atomic_overflow_spec() -> DeckSpec {
    let resource_id = id(4);
    DeckSpec {
        id: id(1),
        logical_slides: vec![slide(
            2,
            LogicalSlideKind::Content,
            false,
            vec![image_node(
                3,
                SemanticRole::Figure,
                resource_id,
                "deliberately unfit atomic image",
            )],
        )],
        resources: vec![DeckResource {
            id: resource_id,
            kind: ResourceKind::RasterImage,
            media_type: "image/png".to_owned(),
            bytes: vec![137, 80, 78, 71],
            intrinsic_size: Some(PixelSize {
                width: 1,
                height: 1_000,
            }),
        }],
    }
}

fn assert_supported_roles(spec: &DeckSpec) -> Result<(), Box<dyn std::error::Error>> {
    fn collect(node: &SemanticNode, roles: &mut BTreeSet<u16>) {
        roles.insert(node.role.code());
        match &node.content {
            SemanticContent::Children(children) => {
                for child in children {
                    collect(child, roles);
                }
            }
            SemanticContent::List(list) => {
                fn collect_list(list: &ListContent, roles: &mut BTreeSet<u16>) {
                    for item in &list.items {
                        for block in &item.blocks {
                            collect(block, roles);
                        }
                        for child in &item.children {
                            collect_list(child, roles);
                        }
                    }
                }
                collect_list(list, roles);
            }
            _ => {}
        }
    }
    let mut actual = BTreeSet::new();
    for slide in &spec.logical_slides {
        for node in &slide.nodes {
            collect(node, &mut actual);
        }
    }
    let expected =
        (SemanticRole::Title.code()..=SemanticRole::Statement.code()).collect::<BTreeSet<_>>();
    if actual != expected {
        return Err(format!("deck gate semantic-role coverage differs: {actual:?}").into());
    }
    if !spec.logical_slides.iter().flat_map(|slide| &slide.nodes).any(|node| {
        matches!(&node.content, SemanticContent::Table(table) if !table.columns.is_empty() && !table.rows.is_empty())
    }) {
        return Err("deck gate does not cover table row/cell/column contracts".into());
    }
    Ok(())
}

fn slide(
    value: u8,
    kind: LogicalSlideKind,
    hidden: bool,
    nodes: Vec<SemanticNode>,
) -> LogicalSlide {
    LogicalSlide {
        id: id(value),
        source: range(value, 0, 999),
        kind,
        hidden,
        nodes,
    }
}

fn text_node(value: u8, role: SemanticRole, text: &str, split: SplitPolicy) -> SemanticNode {
    SemanticNode {
        id: id(value),
        source: range(value, 10, 20),
        role,
        split,
        content: SemanticContent::Text(plain(text)),
    }
}

fn rich_node(value: u8) -> SemanticNode {
    SemanticNode {
        id: id(value),
        source: range(value, 10, 80),
        role: SemanticRole::Prose,
        split: SplitPolicy::Text,
        content: SemanticContent::Text(RichText {
            runs: vec![
                RichTextRun {
                    text: "Bold 한국어, ".to_owned(),
                    marks: TextMarks {
                        bold: true,
                        ..TextMarks::default()
                    },
                    hyperlink: Some(SafeHyperlink {
                        kind: HyperlinkKind::Web,
                        target: "https://example.com/deck-gate".to_owned(),
                    }),
                },
                RichTextRun {
                    text: "italic العربية and inline code".to_owned(),
                    marks: TextMarks {
                        italic: true,
                        inline_code: true,
                        ..TextMarks::default()
                    },
                    hyperlink: None,
                },
            ],
        }),
    }
}

fn list_node(value: u8) -> SemanticNode {
    let item = |identity, text: &str| ListItem {
        id: id(identity),
        source: range(value, 0, 100),
        blocks: vec![text_node(
            identity + 1,
            SemanticRole::ListItem,
            text,
            SplitPolicy::Never,
        )],
        children: vec![],
    };
    let mut first = item(value + 1, "First level");
    first.children.push(ListContent {
        ordered: false,
        start: 1,
        items: vec![item(value + 3, "Nested level")],
    });
    SemanticNode {
        id: id(value),
        source: range(value, 0, 100),
        role: SemanticRole::List,
        split: SplitPolicy::ListItems,
        content: SemanticContent::List(ListContent {
            ordered: true,
            start: 3,
            items: vec![first, item(value + 5, "Second item")],
        }),
    }
}

fn image_node(value: u8, role: SemanticRole, resource_id: StableId, alt: &str) -> SemanticNode {
    SemanticNode {
        id: id(value),
        source: range(value, 10, 20),
        role,
        split: SplitPolicy::Never,
        content: SemanticContent::Image(ImageContent {
            resource_id,
            alt_text: alt.to_owned(),
        }),
    }
}

fn gallery_node(value: u8, gif: StableId, svg: StableId) -> SemanticNode {
    SemanticNode {
        id: id(value),
        source: range(value, 0, 100),
        role: SemanticRole::Gallery,
        split: SplitPolicy::Children,
        content: SemanticContent::Children(vec![
            image_node(
                value + 1,
                SemanticRole::Figure,
                gif,
                "animated GIF first frame",
            ),
            svg_node(value + 2, SemanticRole::Diagram, svg, Some("gallery SVG")),
        ]),
    }
}

fn table_node(value: u8) -> SemanticNode {
    let columns = vec![
        TableColumn {
            id: id(value + 1),
            source: range(value, 1, 2),
            alignment: wasmppt_deck::TableColumnAlignment::Start,
        },
        TableColumn {
            id: id(value + 2),
            source: range(value, 2, 3),
            alignment: wasmppt_deck::TableColumnAlignment::End,
        },
    ];
    let rows = (0..18u8)
        .map(|row| TableRow {
            id: id(value + 3 + row * 3),
            source: range(value, 10 + u32::from(row) * 4, 13 + u32::from(row) * 4),
            cells: vec![
                TableCell {
                    id: id(value + 4 + row * 3),
                    source: range(value, 10 + u32::from(row) * 4, 11 + u32::from(row) * 4),
                    content: plain(if row == 0 { "Region" } else { "서울" }),
                },
                TableCell {
                    id: id(value + 5 + row * 3),
                    source: range(value, 12 + u32::from(row) * 4, 13 + u32::from(row) * 4),
                    content: plain(if row == 0 { "Value" } else { "42" }),
                },
            ],
        })
        .collect();
    SemanticNode {
        id: id(value),
        source: range(value, 0, 100),
        role: SemanticRole::Table,
        split: SplitPolicy::TableRows,
        content: SemanticContent::Table(TableContent {
            columns,
            header_rows: 1,
            rows,
        }),
    }
}

fn chart_node(value: u8) -> SemanticNode {
    SemanticNode {
        id: id(value),
        source: range(value, 10, 40),
        role: SemanticRole::Chart,
        split: SplitPolicy::Never,
        content: SemanticContent::Chart(ChartContent {
            kind: ChartKind::Column,
            categories: vec!["Q1".to_owned(), "Q2".to_owned(), "Q3".to_owned()],
            series: vec![ChartSeries {
                name: "Revenue".to_owned(),
                values: vec![1.0, 2.5, 4.0],
            }],
        }),
    }
}

fn code_node(value: u8) -> SemanticNode {
    SemanticNode {
        id: id(value),
        source: range(value, 40, 90),
        role: SemanticRole::Code,
        split: SplitPolicy::CodeLines,
        content: SemanticContent::Code(CodeContent {
            language: Some("rust".to_owned()),
            code: "fn main() {\n    println!(\"안녕하세요\");\n}\n".to_owned(),
        }),
    }
}

fn svg_node(
    value: u8,
    role: SemanticRole,
    resource_id: StableId,
    source_text: Option<&str>,
) -> SemanticNode {
    SemanticNode {
        id: id(value),
        source: range(value, 10, 30),
        role,
        split: SplitPolicy::Never,
        content: SemanticContent::Svg(SvgContent {
            resource_id,
            source_text: source_text.map(str::to_owned),
        }),
    }
}

fn quote_node(value: u8) -> SemanticNode {
    SemanticNode {
        id: id(value),
        source: range(value, 0, 100),
        role: SemanticRole::Quote,
        split: SplitPolicy::Children,
        content: SemanticContent::Children(vec![
            text_node(
                value + 1,
                SemanticRole::Prose,
                "Measure twice, render once.",
                SplitPolicy::Text,
            ),
            text_node(
                value + 2,
                SemanticRole::Credit,
                "— wasmppt",
                SplitPolicy::Never,
            ),
        ]),
    }
}

fn definition_node(value: u8) -> SemanticNode {
    SemanticNode {
        id: id(value),
        source: range(value, 0, 100),
        role: SemanticRole::Definition,
        split: SplitPolicy::Children,
        content: SemanticContent::Children(vec![
            text_node(
                value + 1,
                SemanticRole::DefinitionTerm,
                "Determinism",
                SplitPolicy::Never,
            ),
            text_node(
                value + 2,
                SemanticRole::DefinitionDescription,
                "Equal inputs yield equal plans and packages.",
                SplitPolicy::Text,
            ),
        ]),
    }
}

fn plain(text: &str) -> RichText {
    RichText {
        runs: vec![RichTextRun {
            text: text.to_owned(),
            marks: TextMarks::default(),
            hyperlink: None,
        }],
    }
}

fn range(_owner: u8, start: u32, end: u32) -> SourceRange {
    SourceRange::new("deck-gate.md", start, end)
}

fn id(value: u8) -> StableId {
    StableId::from_bytes([value; 16])
}
