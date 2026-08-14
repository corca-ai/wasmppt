use wasmppt_deck::{
    DeckDiagnosticCode, DeckLimits, RegionRole, TemplateAssetKind, TemplateLayoutRole,
};
use wasmppt_deck_template::ThemeTemplateCompiler;
use wasmppt_opc::{CompressionMethod, EntryOptions, VecSink, ZipWriter};

const CT: &str = "http://schemas.openxmlformats.org/package/2006/content-types";
const REL: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const OFFICE_REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const PML: &str = "http://schemas.openxmlformats.org/presentationml/2006/main";
const DRAWING: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";

fn package(entries: Vec<(&str, String)>) -> Vec<u8> {
    let options = EntryOptions::deterministic(CompressionMethod::Deflate);
    let mut writer = ZipWriter::new(VecSink::new());
    for (name, value) in entries {
        writer
            .write_entry(name, value.as_bytes(), &options)
            .unwrap();
    }
    writer.finish().unwrap().0.into_inner()
}

fn starter(visible_suffix: &str, extra: Vec<(&str, String)>) -> Vec<u8> {
    starter_with_content(visible_suffix, extra, None)
}

fn starter_with_content(
    visible_suffix: &str,
    extra: Vec<(&str, String)>,
    content_override: Option<Vec<String>>,
) -> Vec<u8> {
    let content_placeholders = content_override.unwrap_or_else(|| {
        vec![
            placeholder(21, "title", 3, false, visible_suffix),
            placeholder(22, "body", 4, true, visible_suffix),
        ]
    });
    let mut entries = vec![
        (
            "[Content_Types].xml",
            format!(
                r#"<Types xmlns="{CT}"><Default Extension="xml" ContentType="application/xml"/><Default Extension="png" ContentType="image/png"/><Override PartName="/ppt/presentation.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.template.main+xml"/><Override PartName="/ppt/slideMasters/slideMaster1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideMaster+xml"/><Override PartName="/ppt/slideLayouts/title.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml"/><Override PartName="/ppt/slideLayouts/content.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml"/><Override PartName="/ppt/slideLayouts/statement.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml"/><Override PartName="/ppt/theme/theme1.xml" ContentType="application/vnd.openxmlformats-officedocument.theme+xml"/></Types>"#,
            ),
        ),
        (
            "_rels/.rels",
            format!(
                r#"<Relationships xmlns="{REL}"><Relationship Id="rId1" Type="{OFFICE_REL}/officeDocument" Target="ppt/presentation.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties" Target="docProps/core.xml"/></Relationships>"#,
            ),
        ),
        (
            "docProps/core.xml",
            r#"<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties"/>"#.to_owned(),
        ),
        (
            "ppt/presentation.xml",
            format!(
                r#"<p:presentation xmlns:p="{PML}"><p:sldSz cx="10000000" cy="5625000"/></p:presentation>"#,
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
                r#"<p:sldMaster xmlns:p="{PML}" xmlns:a="{DRAWING}"><p:cSld><p:bg><p:bgPr><a:solidFill><a:schemeClr val="lt1"/></a:solidFill></p:bgPr></p:bg><p:spTree>
                {master_placeholders}
                <p:pic><p:nvPicPr><p:cNvPr id="90" name="Master Logo {visible_suffix}"/></p:nvPicPr><p:spPr><a:xfrm><a:off x="9000000" y="100000"/><a:ext cx="500000" cy="500000"/></a:xfrm></p:spPr></p:pic>
                </p:spTree></p:cSld><p:txStyles><p:titleStyle><a:lvl1pPr><a:defRPr sz="3600" b="1"><a:latin typeface="+mj-lt"/><a:solidFill><a:schemeClr val="accent1"/></a:solidFill></a:defRPr></a:lvl1pPr></p:titleStyle><p:bodyStyle><a:lvl1pPr marL="1000" indent="-200"><a:defRPr sz="2000"><a:latin typeface="+mn-lt"/></a:defRPr></a:lvl1pPr></p:bodyStyle><p:otherStyle/></p:txStyles></p:sldMaster>"#,
                master_placeholders = master_placeholders(visible_suffix),
            ),
        ),
        (
            "ppt/slideMasters/_rels/slideMaster1.xml.rels",
            format!(
                r#"<Relationships xmlns="{REL}"><Relationship Id="title" Type="{OFFICE_REL}/slideLayout" Target="../slideLayouts/title.xml"/><Relationship Id="content" Type="{OFFICE_REL}/slideLayout" Target="../slideLayouts/content.xml"/><Relationship Id="statement" Type="{OFFICE_REL}/slideLayout" Target="../slideLayouts/statement.xml"/><Relationship Id="theme" Type="{OFFICE_REL}/theme" Target="../theme/theme1.xml"/><Relationship Id="logo" Type="{OFFICE_REL}/image" Target="../media/logo.png"/></Relationships>"#,
            ),
        ),
        (
            "ppt/slideLayouts/title.xml",
            layout(
                "wasmppt:title-v1",
                visible_suffix,
                &[
                    placeholder(11, "title", 1, false, visible_suffix),
                    placeholder(12, "subTitle", 2, false, visible_suffix),
                ],
            ),
        ),
        (
            "ppt/slideLayouts/content.xml",
            layout("wasmppt:content-v1", visible_suffix, &content_placeholders),
        ),
        (
            "ppt/slideLayouts/statement.xml",
            layout(
                "wasmppt:statement-v1",
                visible_suffix,
                &[placeholder(31, "ctrTitle", 5, false, visible_suffix)],
            ),
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
                r#"<a:theme xmlns:a="{DRAWING}"><a:themeElements><a:clrScheme name="Cortex"><a:dk1><a:srgbClr val="111111"/></a:dk1><a:lt1><a:srgbClr val="FAFAFA"/></a:lt1><a:accent1><a:srgbClr val="3366CC"/></a:accent1></a:clrScheme><a:fontScheme name="Cortex"><a:majorFont><a:latin typeface="Aptos Display"/><a:ea typeface="Noto Sans CJK KR"/><a:cs typeface="Arial"/></a:majorFont><a:minorFont><a:latin typeface="Aptos"/><a:ea typeface="Noto Sans CJK KR"/><a:cs typeface="Arial"/></a:minorFont></a:fontScheme></a:themeElements></a:theme>"#,
            ),
        ),
        ("ppt/media/logo.png", "not-a-real-png".to_owned()),
    ];
    entries.extend(extra);
    package(entries)
}

fn master_placeholders(suffix: &str) -> String {
    [
        master_placeholder(
            1,
            "title",
            1,
            (800_000, 500_000, 8_400_000, 900_000),
            suffix,
        ),
        master_placeholder(
            2,
            "subTitle",
            2,
            (1_000_000, 1_600_000, 8_000_000, 800_000),
            suffix,
        ),
        master_placeholder(
            3,
            "title",
            3,
            (600_000, 300_000, 8_800_000, 700_000),
            suffix,
        ),
        master_placeholder(
            4,
            "body",
            4,
            (700_000, 1_200_000, 8_600_000, 3_800_000),
            suffix,
        ),
        master_placeholder(
            5,
            "ctrTitle",
            5,
            (1_000_000, 1_500_000, 8_000_000, 2_000_000),
            suffix,
        ),
    ]
    .join("")
}

fn master_placeholder(
    id: u32,
    kind: &str,
    index: u32,
    frame: (i64, i64, i64, i64),
    suffix: &str,
) -> String {
    let (x, y, width, height) = frame;
    format!(
        r#"<p:sp><p:nvSpPr><p:cNvPr id="{id}" name="Master {suffix} {id}"/><p:nvPr><p:ph type="{kind}" idx="{index}"/></p:nvPr></p:nvSpPr><p:spPr><a:xfrm><a:off x="{x}" y="{y}"/><a:ext cx="{width}" cy="{height}"/></a:xfrm></p:spPr><p:txBody><a:bodyPr lIns="100" tIns="200" rIns="300" bIns="400"/><a:lstStyle/></p:txBody></p:sp>"#,
    )
}

fn placeholder(id: u32, kind: &str, index: u32, with_style: bool, suffix: &str) -> String {
    let style = if with_style {
        r#"<a:lvl1pPr><a:defRPr sz="1800" i="1"><a:solidFill><a:schemeClr val="accent1"/></a:solidFill></a:defRPr></a:lvl1pPr>"#
    } else {
        ""
    };
    format!(
        r#"<p:sp><p:nvSpPr><p:cNvPr id="{id}" name="Visible {suffix} {id}"/><p:nvPr><p:ph type="{kind}" idx="{index}"/></p:nvPr></p:nvSpPr><p:spPr/><p:txBody><a:bodyPr/><a:lstStyle>{style}</a:lstStyle></p:txBody></p:sp>"#,
    )
}

fn layout(matching_name: &str, suffix: &str, placeholders: &[String]) -> String {
    format!(
        r#"<p:sldLayout xmlns:p="{PML}" xmlns:a="{DRAWING}" matchingName="{matching_name}"><p:cSld name="Visible Layout {suffix}"><p:spTree>{}<p:sp><p:nvSpPr><p:cNvPr id="80" name="Decoration {suffix}"/></p:nvSpPr><p:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="100000" cy="100000"/></a:xfrm></p:spPr></p:sp></p:spTree></p:cSld></p:sldLayout>"#,
        placeholders.join("")
    )
}

fn layout_relationships() -> String {
    format!(
        r#"<Relationships xmlns="{REL}"><Relationship Id="master" Type="{OFFICE_REL}/slideMaster" Target="../slideMasters/slideMaster1.xml"/></Relationships>"#,
    )
}

#[test]
fn compiles_exact_geometry_inherited_regions_theme_and_preserved_assets() {
    let result = ThemeTemplateCompiler::default()
        .compile(starter("A", vec![]))
        .unwrap();
    assert!(result.cacheable, "{:?}", result.plan.diagnostics);
    assert_eq!(result.plan.page_size.width, 10_000_000);
    assert_eq!(result.plan.page_size.height, 5_625_000);
    assert_eq!(result.plan.layouts.len(), 3);
    assert_eq!(result.plan.layouts[0].role, TemplateLayoutRole::Title);
    assert_eq!(result.plan.regions.len(), 5);
    assert!(result.plan.assets.len() >= 6);
    assert_eq!(
        result.plan.theme.major_fonts.latin.as_deref(),
        Some("Aptos Display")
    );
    assert_eq!(
        result
            .plan
            .theme
            .colors
            .iter()
            .find(|color| color.slot == "accent1")
            .unwrap()
            .rgb,
        0x3366CC
    );

    let content_body = result
        .plan
        .regions
        .iter()
        .find(|region| region.role == RegionRole::Body)
        .unwrap();
    assert_eq!(content_body.frame.width, 8_600_000);
    assert_eq!(content_body.margins.left, 100);
    assert_eq!(content_body.text_levels[0].font_size, Some(1800));
    assert_eq!(content_body.text_levels[0].italic, Some(true));
    assert_eq!(content_body.text_levels[0].margin_left, Some(1000));
    assert!(
        content_body
            .accepts
            .contains(&wasmppt_deck::SemanticRole::Section)
    );
    assert!(
        content_body
            .accepts
            .contains(&wasmppt_deck::SemanticRole::DefinitionTerm)
    );
    assert!(
        content_body
            .accepts
            .contains(&wasmppt_deck::SemanticRole::DefinitionDescription)
    );
    let title_details = result
        .plan
        .regions
        .iter()
        .find(|region| region.role == RegionRole::Subtitle)
        .unwrap();
    assert!(
        title_details
            .accepts
            .contains(&wasmppt_deck::SemanticRole::Prose)
    );
    assert!(
        result
            .plan
            .assets
            .iter()
            .all(|asset| asset.source_xml.end > asset.source_xml.start)
    );
}

#[test]
fn visible_names_and_example_slides_do_not_drive_discovery() {
    let first = ThemeTemplateCompiler::default()
        .compile(starter("First", vec![]))
        .unwrap();
    let second = ThemeTemplateCompiler::default()
        .compile(starter(
            "Renamed",
            vec![(
                "ppt/slides/example.xml",
                format!(r#"<p:sld xmlns:p="{PML}"/>"#),
            )],
        ))
        .unwrap();
    let roles = |result: &wasmppt_deck_template::ThemeCompileResult| {
        result
            .plan
            .layouts
            .iter()
            .map(|layout| (layout.matching_name.clone(), layout.role))
            .collect::<Vec<_>>()
    };
    assert_eq!(roles(&first), roles(&second));
}

#[test]
fn preserves_page_furniture_as_assets_without_semantic_regions() {
    let bytes = starter_with_content(
        "Furniture",
        vec![],
        Some(vec![
            placeholder(21, "title", 3, false, "Furniture"),
            placeholder(22, "body", 4, true, "Furniture"),
            placeholder(23, "sldNum", 9, false, "Furniture"),
        ]),
    );
    let result = ThemeTemplateCompiler::default().compile(bytes).unwrap();

    assert!(result.cacheable, "{:?}", result.plan.diagnostics);
    assert_eq!(result.plan.regions.len(), 5);
    assert!(
        result
            .plan
            .regions
            .iter()
            .all(|region| region.role != RegionRole::Footer)
    );
    assert!(
        result
            .plan
            .assets
            .iter()
            .any(|asset| asset.kind == TemplateAssetKind::Footer)
    );
}

#[test]
fn reports_missing_and_duplicate_contract_problems_together() {
    let entries = vec![
        (
            "[Content_Types].xml",
            format!(
                r#"<Types xmlns="{CT}"><Default Extension="xml" ContentType="application/xml"/><Override PartName="/ppt/presentation.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.template.main+xml"/><Override PartName="/ppt/slideLayouts/a.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml"/><Override PartName="/ppt/slideLayouts/b.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml"/></Types>"#
            ),
        ),
        (
            "_rels/.rels",
            format!(
                r#"<Relationships xmlns="{REL}"><Relationship Id="r" Type="{OFFICE_REL}/officeDocument" Target="ppt/presentation.xml"/></Relationships>"#
            ),
        ),
        (
            "ppt/presentation.xml",
            format!(r#"<p:presentation xmlns:p="{PML}"><p:sldSz cx="1" cy="1"/></p:presentation>"#),
        ),
        (
            "ppt/slideLayouts/a.xml",
            layout("wasmppt:title-v1", "A", &[]),
        ),
        (
            "ppt/slideLayouts/b.xml",
            layout("wasmppt:title-v1", "B", &[]),
        ),
    ];
    let bytes = package(entries);
    let result = ThemeTemplateCompiler::default().compile(bytes).unwrap();
    let codes = result
        .plan
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect::<Vec<_>>();
    assert!(!result.cacheable);
    assert!(codes.contains(&DeckDiagnosticCode::TEMPLATE_DUPLICATE_LAYOUT));
    assert!(codes.contains(&DeckDiagnosticCode::TEMPLATE_MISSING_LAYOUT));
    assert!(codes.contains(&DeckDiagnosticCode::TEMPLATE_MISSING_THEME));
}

#[test]
fn reports_duplicate_and_missing_placeholders_in_one_result() {
    let bytes = starter_with_content(
        "Placeholders",
        vec![],
        Some(vec![
            placeholder(41, "body", 4, false, "First"),
            placeholder(42, "body", 4, false, "Second"),
        ]),
    );
    let result = ThemeTemplateCompiler::default().compile(bytes).unwrap();
    let messages = result
        .plan
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.message.as_str())
        .collect::<Vec<_>>();
    assert!(!result.cacheable);
    assert!(messages.iter().any(|message| message.contains("body:4")));
    assert!(
        messages
            .iter()
            .any(|message| message.contains("missing required Title"))
    );
    assert!(
        messages
            .iter()
            .any(|message| message.contains("2 placeholders") && message.contains("Body"))
    );
}

#[test]
fn arbitrary_existing_potx_fails_with_stable_starter_diagnostics() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/host-adapters/minimal.potx");
    let bytes = std::fs::read(path).unwrap();
    let first = ThemeTemplateCompiler::default()
        .compile(bytes.clone())
        .unwrap();
    let second = ThemeTemplateCompiler::default().compile(bytes).unwrap();
    assert!(!first.cacheable);
    assert!(
        first
            .plan
            .diagnostics
            .iter()
            .any(|diagnostic| { diagnostic.code == DeckDiagnosticCode::TEMPLATE_MISSING_LAYOUT })
    );
    assert_eq!(first.plan.diagnostics, second.plan.diagnostics);
}

#[test]
fn rejects_macro_content_before_cache_and_is_deterministic() {
    let bytes = starter("Macro", vec![("ppt/vbaProject.bin", "macro".to_owned())]);
    let first = ThemeTemplateCompiler::default()
        .compile(bytes.clone())
        .unwrap();
    let second = ThemeTemplateCompiler::default().compile(bytes).unwrap();
    assert!(!first.cacheable);
    assert!(
        first
            .plan
            .diagnostics
            .iter()
            .any(|diagnostic| { diagnostic.code == DeckDiagnosticCode::TEMPLATE_UNSAFE_CONTENT })
    );
    assert_eq!(first.plan.cache_key, second.plan.cache_key);
    assert_eq!(
        first.plan.encode(&DeckLimits::default()).unwrap(),
        second.plan.encode(&DeckLimits::default()).unwrap()
    );
}

#[test]
fn rejects_an_exact_embedded_package_relationship() {
    let bytes = starter(
        "Package",
        vec![(
            "ppt/slideLayouts/_rels/package.xml.rels",
            format!(
                r#"<Relationships xmlns="{REL}"><Relationship Id="embedded" Type="{OFFICE_REL}/package" Target="../embeddings/object.bin"/></Relationships>"#,
            ),
        )],
    );
    let result = ThemeTemplateCompiler::default().compile(bytes).unwrap();

    assert!(!result.cacheable);
    assert!(result.plan.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == DeckDiagnosticCode::TEMPLATE_UNSAFE_CONTENT
            && diagnostic.message.contains("package.xml.rels")
    }));
}
