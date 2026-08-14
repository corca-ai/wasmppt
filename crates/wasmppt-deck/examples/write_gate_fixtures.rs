use std::{collections::BTreeSet, env, fs, path::PathBuf};

use wasmppt_deck::{
    ChartContent, ChartKind, ChartSeries, CodeContent, DeckLimits, DeckResource, DeckSpec,
    HyperlinkKind, ImageContent, ListContent, ListItem, LogicalSlide, LogicalSlideKind,
    MediaTextProximity, MediaTextRelation, MediaTextSide, PixelSize, ResourceKind, RichText,
    RichTextRun, SafeHyperlink, SemanticContent, SemanticNode, SemanticRole, SourceRange,
    SplitPolicy, StableId, SvgContent, TableCell, TableColumn, TableContent, TableRow, TextMarks,
    validate_deck_spec,
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
    fs::write(output.join("corpus.json"), corpus_manifest())?;
    Ok(())
}

fn corpus_manifest() -> &'static str {
    concat!(
        "{\n  \"schema\": 2,\n  \"id\": \"autolayout-v3\",\n  \"cases\": [\n",
        "    \"title-variants\", \"long-prose\", \"long-list-with-live-empty-item\",\n",
        "    \"table-continuation\", \"long-code\", \"mixed-media\",\n",
        "    \"aspect-aware-gallery-10\", \"figure-caption\", \"quote-credit\",\n",
        "    \"section\", \"display-math\", \"definition\", \"statement\", \"hidden-page\",\n",
        "    \"media-aspects-4x1-16x9-1x1-3x4-1x4\",\n",
        "    \"media-context-image-caption-short-long\",\n",
        "    \"media-pairs-2-3-5-9\", \"jpeg-exif-orientation-6-8\"\n",
        "  ],\n  \"invariants\": [\n",
        "    \"exact-source-coverage\", \"no-overlap\", \"readable-type\",\n",
        "    \"balanced-flow\", \"no-singleton-final-orphan\", \"bounded-media\",\n",
        "    \"single-editable-table-per-slice\", \"canvas-pptx-geometry-parity\",\n",
        "    \"cross-host-byte-determinism\", \"single-slide-invalidation\",\n",
        "    \"contain-aspect-fidelity\", \"bounded-cover-crop\",\n",
        "    \"media-text-cohesion\", \"gallery-page-balance\",\n",
        "    \"jpeg-exif-display-axis\"\n",
        "  ]\n}\n",
    )
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
                "wasmppt:title-v3",
                &[placeholder(11, "title", 1), placeholder(12, "subTitle", 2)],
            ),
        ),
        (
            "ppt/slideLayouts/content.xml",
            layout(
                "wasmppt:content-envelope-v3",
                &[
                    placeholder(21, "title", 3),
                    placeholder(22, "body", 4),
                    placeholder(24, "pic", 5),
                    placeholder(23, "sldNum", 6),
                ],
            ),
        ),
        (
            "ppt/slideLayouts/statement.xml",
            layout("wasmppt:statement-v3", &[placeholder(31, "ctrTitle", 5)]),
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
        master_placeholder(8, "pic", 5, (300_000, 1_100_000, 11_592_000, 5_250_000)),
        master_placeholder(
            6,
            "ctrTitle",
            5,
            (1_000_000, 1_600_000, 10_192_000, 2_200_000),
        ),
        master_placeholder(7, "sldNum", 6, (11_000_000, 6_400_000, 500_000, 200_000)),
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
    let square = id(230);
    let gif = id(231);
    let svg = id(232);
    let math = id(233);
    let portrait = id(234);
    let wide = id(235);
    let quality_resources = [
        media_resource(0, "4x1", 400, 100, [220, 38, 38]),
        media_resource(1, "16x9", 160, 90, [234, 88, 12]),
        media_resource(2, "1x1", 128, 128, [22, 163, 74]),
        media_resource(3, "3x4", 120, 160, [8, 145, 178]),
        media_resource(4, "1x4", 64, 256, [124, 58, 237]),
    ];
    let exif_resources = [exif_resource(0, "exif-6", 6), exif_resource(1, "exif-8", 8)];
    let mut slides = vec![
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
                image_node(42, SemanticRole::Figure, square, "square PNG"),
                {
                    let mut caption = text_node(
                        43,
                        SemanticRole::Caption,
                        "Loss-aware media caption",
                        SplitPolicy::Never,
                    );
                    caption.source = range(43, 21, 40);
                    caption
                },
                gallery_node(44, [square, portrait, wide, gif]),
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
    slides[2].media_text_relations.push(MediaTextRelation {
        media_node_id: id(42),
        text_node_id: id(43),
        proximity: MediaTextProximity::AdjacentBlocks,
        text_side: MediaTextSide::AfterMedia,
        explicit_caption: true,
    });
    slides.extend(media_quality_slides(&quality_resources, &exif_resources));
    let mut resources = vec![
        DeckResource {
            id: square,
            kind: ResourceKind::RasterImage,
            media_type: "image/png".to_owned(),
            bytes: png(64, 64, [37, 99, 235]),
            intrinsic_size: Some(PixelSize {
                width: 64,
                height: 64,
            }),
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
        DeckResource {
            id: portrait,
            kind: ResourceKind::RasterImage,
            media_type: "image/png".to_owned(),
            bytes: png(48, 96, [15, 118, 110]),
            intrinsic_size: Some(PixelSize {
                width: 48,
                height: 96,
            }),
        },
        DeckResource {
            id: wide,
            kind: ResourceKind::RasterImage,
            media_type: "image/png".to_owned(),
            bytes: png(128, 48, [217, 119, 6]),
            intrinsic_size: Some(PixelSize {
                width: 128,
                height: 48,
            }),
        },
    ];
    resources.extend(
        quality_resources
            .iter()
            .map(|resource| resource.resource.clone()),
    );
    resources.extend(
        exif_resources
            .iter()
            .map(|resource| resource.resource.clone()),
    );
    DeckSpec {
        id: id(255),
        logical_slides: slides,
        resources,
    }
}

#[derive(Clone)]
struct MediaFixture {
    label: &'static str,
    display_size: PixelSize,
    resource: DeckResource,
}

fn media_resource(
    ordinal: u32,
    label: &'static str,
    width: u32,
    height: u32,
    color: [u8; 3],
) -> MediaFixture {
    let resource_id = id(229).derive(b"media-quality-resource", ordinal);
    let display_size = PixelSize { width, height };
    MediaFixture {
        label,
        display_size,
        resource: DeckResource {
            id: resource_id,
            kind: ResourceKind::RasterImage,
            media_type: "image/png".to_owned(),
            bytes: png(width, height, color),
            intrinsic_size: Some(display_size),
        },
    }
}

fn exif_resource(ordinal: u32, label: &'static str, orientation: u16) -> MediaFixture {
    let resource_id = id(229).derive(b"media-quality-exif-resource", ordinal);
    MediaFixture {
        label,
        display_size: PixelSize {
            width: 10,
            height: 40,
        },
        resource: DeckResource {
            id: resource_id,
            kind: ResourceKind::RasterImage,
            media_type: "image/jpeg".to_owned(),
            bytes: jpeg_with_orientation(orientation),
            // Deliberately stale stored-axis hint: byte-derived EXIF display axes must win.
            intrinsic_size: Some(PixelSize {
                width: 40,
                height: 10,
            }),
        },
    }
}

fn media_quality_slides(
    resources: &[MediaFixture; 5],
    exif_resources: &[MediaFixture; 2],
) -> Vec<LogicalSlide> {
    let mut slides = Vec::new();
    let mut ordinal = 0u32;
    for resource in resources {
        for scenario in ["image-only", "caption", "short-copy", "long-prose"] {
            slides.push(media_context_slide(ordinal, resource, scenario));
            ordinal += 1;
        }
        for count in [2usize, 3, 5, 9] {
            slides.push(media_pair_slide(ordinal, resource, count));
            ordinal += 1;
        }
    }
    slides.push(media_context_slide(
        ordinal,
        &exif_resources[0],
        "short-copy",
    ));
    slides.push(media_context_slide(
        ordinal + 1,
        &exif_resources[1],
        "caption",
    ));
    slides
}

fn media_context_slide(ordinal: u32, resource: &MediaFixture, scenario: &str) -> LogicalSlide {
    let case = format!("{}-{scenario}", resource.label);
    let slide_id = media_quality_id(b"slide", ordinal);
    let title = quality_text_node(
        slide_id.derive(b"title", 0),
        &case,
        0,
        SemanticRole::Title,
        &format!("Media quality: {case}"),
        SplitPolicy::Never,
    );
    let figure_id = slide_id.derive(b"figure", 0);
    let figure = quality_image_node(figure_id, &case, 20, resource, 0);
    let mut nodes = vec![title, figure];
    let mut relations = Vec::new();
    if scenario != "image-only" {
        let text_id = slide_id.derive(b"copy", 0);
        let (role, text, split, explicit_caption) = match scenario {
            "caption" => (
                SemanticRole::Caption,
                format!("A concise caption for the {} resource.", resource.label),
                SplitPolicy::Never,
                true,
            ),
            "short-copy" => (
                SemanticRole::Prose,
                format!("Short copy explains the {} visual without overwhelming it.", resource.label),
                SplitPolicy::Never,
                false,
            ),
            "long-prose" => (
                SemanticRole::Prose,
                std::iter::repeat_n(
                    format!(
                        "Long measured prose accompanies the {} visual while preserving readable type and useful media geometry.",
                        resource.label
                    ),
                    14,
                )
                .collect::<Vec<_>>()
                .join(" "),
                SplitPolicy::Text,
                false,
            ),
            _ => unreachable!("bounded media context"),
        };
        nodes.push(quality_text_node(text_id, &case, 40, role, &text, split));
        relations.push(MediaTextRelation {
            media_node_id: figure_id,
            text_node_id: text_id,
            proximity: MediaTextProximity::AdjacentBlocks,
            text_side: MediaTextSide::AfterMedia,
            explicit_caption,
        });
    }
    LogicalSlide {
        id: slide_id,
        source: SourceRange::new(format!("media-quality/{case}.md"), 0, 1_000),
        kind: LogicalSlideKind::Content,
        hidden: false,
        nodes,
        media_text_relations: relations,
    }
}

fn media_pair_slide(ordinal: u32, resource: &MediaFixture, count: usize) -> LogicalSlide {
    let case = format!("{}-{count}-pairs", resource.label);
    let slide_id = media_quality_id(b"slide", ordinal);
    let mut children = Vec::with_capacity(count * 2);
    let mut relations = Vec::with_capacity(count);
    for index in 0..count {
        let item = u32::try_from(index).expect("quality fixture item count is bounded");
        let figure_id = slide_id.derive(b"figure", item);
        let caption_id = slide_id.derive(b"caption", item);
        children.push(quality_image_node(
            figure_id,
            &case,
            100 + item * 20,
            resource,
            item,
        ));
        children.push(quality_text_node(
            caption_id,
            &case,
            110 + item * 20,
            SemanticRole::Caption,
            &format!("Related copy {} of {count}", index + 1),
            SplitPolicy::Never,
        ));
        relations.push(MediaTextRelation {
            media_node_id: figure_id,
            text_node_id: caption_id,
            proximity: MediaTextProximity::AdjacentBlocks,
            text_side: MediaTextSide::AfterMedia,
            explicit_caption: true,
        });
    }
    LogicalSlide {
        id: slide_id,
        source: SourceRange::new(format!("media-quality/{case}.md"), 0, 1_000),
        kind: LogicalSlideKind::Content,
        hidden: false,
        nodes: vec![
            quality_text_node(
                slide_id.derive(b"title", 0),
                &case,
                0,
                SemanticRole::Title,
                &format!("{} related media/text pairs", count),
                SplitPolicy::Never,
            ),
            SemanticNode {
                id: slide_id.derive(b"gallery", 0),
                source: SourceRange::new(format!("media-quality/{case}.md"), 90, 900),
                role: SemanticRole::Gallery,
                split: SplitPolicy::Children,
                content: SemanticContent::Children(children),
            },
        ],
        media_text_relations: relations,
    }
}

fn quality_text_node(
    node_id: StableId,
    case: &str,
    start: u32,
    role: SemanticRole,
    text: &str,
    split: SplitPolicy,
) -> SemanticNode {
    SemanticNode {
        id: node_id,
        source: SourceRange::new(
            format!("media-quality/{case}.md"),
            start,
            start.saturating_add(9),
        ),
        role,
        split,
        content: SemanticContent::Text(plain(text)),
    }
}

fn quality_image_node(
    node_id: StableId,
    case: &str,
    start: u32,
    resource: &MediaFixture,
    item: u32,
) -> SemanticNode {
    SemanticNode {
        id: node_id,
        source: SourceRange::new(
            format!("media-quality/{case}.md"),
            start,
            start.saturating_add(9),
        ),
        role: SemanticRole::Figure,
        split: SplitPolicy::Never,
        content: SemanticContent::Image(ImageContent {
            resource_id: resource.resource.id,
            alt_text: format!(
                "gate-media|{case}|{item}|{}|{}",
                resource.display_size.width, resource.display_size.height
            ),
        }),
    }
}

fn media_quality_id(domain: &[u8], ordinal: u32) -> StableId {
    id(229).derive(domain, ordinal)
}

fn jpeg_with_orientation(orientation: u16) -> Vec<u8> {
    assert!((5..=8).contains(&orientation));
    let base = decode_hex(concat!(
        "ffd8ffe000104a46494600010100000100010000ffdb0043000503040404030504040405050506070c08070707070f0b0b090c110f1212110f111113161c171314",
        "1a1511111821181a1d1d1f1f1f13172224221e241c1e1f1effdb0043010505050706070e08080e1e1411141e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e",
        "1e1e",
        "1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1effc0001108000a002803012200021101031101ffc40015000101000000000000000000",
        "00000000000007ffc40014100100000000000000000000000000000000ffc4001501010100000000000000000000000000000007ffc400141101000000000000",
        "00000000000000000000ffda000c03010002110311003f009a00a827e000000fffd9",
    ));
    assert_eq!(&base[..2], &[0xff, 0xd8]);
    let mut exif = vec![0xff, 0xe1, 0x00, 0x22];
    exif.extend_from_slice(b"Exif\0\0II");
    exif.extend_from_slice(&42u16.to_le_bytes());
    exif.extend_from_slice(&8u32.to_le_bytes());
    exif.extend_from_slice(&1u16.to_le_bytes());
    exif.extend_from_slice(&0x0112u16.to_le_bytes());
    exif.extend_from_slice(&3u16.to_le_bytes());
    exif.extend_from_slice(&1u32.to_le_bytes());
    exif.extend_from_slice(&orientation.to_le_bytes());
    exif.extend_from_slice(&0u16.to_le_bytes());
    exif.extend_from_slice(&0u32.to_le_bytes());
    let mut bytes = Vec::with_capacity(base.len() + exif.len());
    bytes.extend_from_slice(&base[..2]);
    bytes.extend_from_slice(&exif);
    bytes.extend_from_slice(&base[2..]);
    assert_eq!(
        wasmppt_deck::inspect_jpeg_size(&bytes),
        Some(PixelSize {
            width: 10,
            height: 40,
        })
    );
    bytes
}

fn decode_hex(value: &str) -> Vec<u8> {
    assert_eq!(value.len() % 2, 0);
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let digit = |byte: u8| match byte {
                b'0'..=b'9' => byte - b'0',
                b'a'..=b'f' => byte - b'a' + 10,
                _ => panic!("fixture JPEG contains non-hex input"),
            };
            digit(pair[0]) * 16 + digit(pair[1])
        })
        .collect()
}

fn png(width: u32, height: u32, color: [u8; 3]) -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, width, height);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().expect("gate PNG header must encode");
        let pixels = color
            .into_iter()
            .cycle()
            .take(width as usize * height as usize * 3)
            .collect::<Vec<_>>();
        writer
            .write_image_data(&pixels)
            .expect("gate PNG pixels must encode");
    }
    bytes
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
        media_text_relations: Vec::new(),
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
                    text: concat!(
                        "Bold 한국어 introduces a measured paragraph. ",
                        "A second sentence establishes a stable break opportunity. ",
                        "A third sentence is long enough to exercise balanced continuation. ",
                        "A fourth sentence keeps prose editable after pagination. ",
                    )
                    .to_owned(),
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
                    text: concat!(
                        "Italic العربية and inline code continue the same source block. ",
                        "The sixth sentence tests width demand. The seventh tests height demand. ",
                        "The eighth sentence prevents a tiny final prose fragment."
                    )
                    .to_owned(),
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
    let item = |index: u32, text: &str| ListItem {
        id: id(value).derive(b"gate-list-item", index),
        source: range(value, 0, 100),
        blocks: vec![SemanticNode {
            id: id(value).derive(b"gate-list-block", index),
            source: range(value, 0, 100),
            role: SemanticRole::ListItem,
            split: SplitPolicy::Never,
            content: SemanticContent::Text(plain(text)),
        }],
        children: vec![],
    };
    let mut first = item(1, "First level with a nested explanation");
    first.children.push(ListContent {
        ordered: false,
        start: 1,
        items: vec![item(20, "Nested level remains attached")],
    });
    let mut items = vec![first];
    items.extend((2..=10).map(|index| item(index, &format!("Balanced list item {index}"))));
    items.push(ListItem {
        id: id(value).derive(b"gate-list-item", 11),
        source: range(value, 99, 100),
        blocks: vec![],
        children: vec![],
    });
    SemanticNode {
        id: id(value),
        source: range(value, 0, 100),
        role: SemanticRole::List,
        split: SplitPolicy::ListItems,
        content: SemanticContent::List(ListContent {
            ordered: true,
            start: 3,
            items,
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

fn gallery_node(value: u8, resources: [StableId; 4]) -> SemanticNode {
    let mut children = Vec::new();
    for index in 0..10u32 {
        children.push(SemanticNode {
            id: id(value).derive(b"gate-gallery-item", index),
            source: range(value, index * 8, index * 8 + 4),
            role: SemanticRole::Figure,
            split: SplitPolicy::Never,
            content: SemanticContent::Image(ImageContent {
                resource_id: resources[index as usize % resources.len()],
                alt_text: format!("gallery photo {}", index + 1),
            }),
        });
        if matches!(index, 2 | 7) {
            children.push(SemanticNode {
                id: id(value).derive(b"gate-gallery-caption", index),
                source: range(value, index * 8 + 4, index * 8 + 8),
                role: SemanticRole::Caption,
                split: SplitPolicy::Never,
                content: SemanticContent::Text(plain(&format!("Caption {}", index + 1))),
            });
        }
    }
    SemanticNode {
        id: id(value),
        source: range(value, 0, 100),
        role: SemanticRole::Gallery,
        split: SplitPolicy::Children,
        content: SemanticContent::Children(children),
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
            code: (1..=25)
                .map(|line| format!("let value_{line} = {line}; // 안녕하세요"))
                .collect::<Vec<_>>()
                .join("\n"),
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
