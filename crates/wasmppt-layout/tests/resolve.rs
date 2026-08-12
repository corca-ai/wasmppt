use wasmppt_layout::{
    ChartKind, ElementKind, Fill, PresentationDocument, PreservedFeature, ResolveDiagnosticCode,
    RgbaColor,
};

const FIXTURE: &[u8] = include_bytes!("../../../fixtures/render/basic.pptx");

#[test]
fn opening_is_lazy_and_one_slide_touches_only_its_dependency_branch() {
    let deck = PresentationDocument::open(FIXTURE.to_vec()).unwrap();
    assert_eq!(deck.slide_count(), 2);
    assert_eq!(deck.open_trace().parsed_xml_parts, ["ppt/presentation.xml"]);

    let output = deck.resolve_slide(0).unwrap();
    assert_eq!(output.slide.size.width, 12_192_000);
    assert_eq!(output.slide.size.height, 6_858_000);
    assert!(
        output
            .trace
            .parsed_xml_parts
            .contains(&"ppt/slides/slide1.xml".to_owned())
    );
    assert!(
        output
            .trace
            .parsed_xml_parts
            .contains(&"ppt/theme/theme1.xml".to_owned())
    );
    assert!(
        !output
            .trace
            .visited_parts
            .iter()
            .any(|part| part.contains("slide2") || part.contains("theme2"))
    );
    assert!(output.trace.decoded_media_parts.is_empty());
    assert!(
        output
            .trace
            .visited_parts
            .contains(&"ppt/media/image1.png".to_owned())
    );
    assert_eq!(
        deck.read_part("ppt/media/image1.png").unwrap(),
        b"fixture image bytes"
    );
    assert!(deck.read_part("ppt/media/missing.png").is_err());
}

#[test]
fn resolves_inheritance_theme_groups_geometry_images_and_diagnostics() {
    let output = PresentationDocument::open(FIXTURE.to_vec())
        .unwrap()
        .resolve_slide(0)
        .unwrap();
    let title = output
        .slide
        .elements
        .iter()
        .find(|element| element.name == "Title")
        .unwrap();
    assert_eq!(title.text, "Actual title");
    assert_eq!(
        title.alternative_text.as_deref(),
        Some("Quarterly report title")
    );
    assert_eq!(
        title.hyperlink.as_deref(),
        Some("https://example.com/report")
    );
    assert_eq!(title.transform.bounds.origin.x, 1_000_000);
    assert_eq!(title.transform.bounds.size.width, 8_000_000);
    assert_eq!(title.group_transforms.len(), 1);
    assert_eq!(title.group_transforms[0].outer.rotation, 60_000);
    assert_eq!(
        title.fill,
        Fill::Solid(RgbaColor {
            red: 91,
            green: 132,
            blue: 173,
            alpha: 255,
        })
    );
    assert_eq!(title.stroke.as_ref().unwrap().width, 12_700);

    let photo = output
        .slide
        .elements
        .iter()
        .find(|element| element.name == "Photo")
        .unwrap();
    let ElementKind::Image {
        part_name, crop, ..
    } = &photo.kind
    else {
        panic!("photo must resolve as an image")
    };
    assert_eq!(part_name.as_deref(), Some("ppt/media/image1.png"));
    assert_eq!(
        (crop.left, crop.top, crop.right, crop.bottom),
        (1000, 2000, 3000, 4000)
    );
    assert_eq!(
        photo.alternative_text.as_deref(),
        Some("Quarterly report photo")
    );

    assert!(
        output
            .slide
            .elements
            .iter()
            .any(|element| element.name == "Master decoration")
    );
    assert!(
        output
            .slide
            .elements
            .iter()
            .any(|element| element.name == "Layout decoration")
    );
    assert!(
        output.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == ResolveDiagnosticCode::UnsupportedCustomGeometry
        })
    );
    assert!(
        output.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == ResolveDiagnosticCode::UnsupportedGraphicFrame
        })
    );
}

#[test]
fn dependency_invalidation_is_exact_for_disjoint_branches() {
    let deck = PresentationDocument::open(FIXTURE.to_vec()).unwrap();
    assert_eq!(deck.invalidated_slides("ppt/theme/theme1.xml"), [0]);
    assert_eq!(deck.invalidated_slides("ppt/theme/theme2.xml"), [1]);
    assert_eq!(
        deck.invalidated_slides("ppt/slideLayouts/slideLayout2.xml"),
        [1]
    );
    assert_eq!(deck.invalidated_slides("ppt/media/image1.png"), [0]);
    assert!(deck.invalidated_slides("ppt/missing.xml").is_empty());
}

#[test]
fn reads_tables_chart_caches_and_preserves_advanced_content_explicitly() {
    let output = PresentationDocument::open(FIXTURE.to_vec())
        .unwrap()
        .resolve_slide(1)
        .unwrap();
    let table = output
        .slide
        .elements
        .iter()
        .find_map(|element| match &element.kind {
            ElementKind::Table { table } => Some(table),
            _ => None,
        })
        .unwrap();
    assert_eq!(table.column_widths, [2_500_000, 2_500_000]);
    assert_eq!(table.rows.len(), 2);
    assert_eq!(table.rows[0].cells[0].text, "Quarter");
    assert_eq!(table.rows[1].cells[1].text, "42");

    let chart = output
        .slide
        .elements
        .iter()
        .find_map(|element| match &element.kind {
            ElementKind::Chart { chart } => Some(chart),
            _ => None,
        })
        .unwrap();
    assert_eq!(chart.kind, ChartKind::Column);
    assert_eq!(chart.series[0].name, "Sales");
    assert_eq!(chart.series[0].categories, ["Q1", "Q2", "Q3"]);
    assert_eq!(chart.series[0].values, [42.0, 64.0, 53.0]);
    assert_eq!(
        chart.embedded_workbook.as_deref(),
        Some("ppt/embeddings/sales.xlsx")
    );
    assert!(
        output
            .trace
            .visited_parts
            .contains(&"ppt/embeddings/sales.xlsx".to_owned())
    );
    assert!(
        !output
            .trace
            .parsed_xml_parts
            .contains(&"ppt/embeddings/sales.xlsx".to_owned())
    );

    assert!(output.slide.elements.iter().any(|element| {
        matches!(
            element.kind,
            ElementKind::PreservedGraphic {
                feature: PreservedFeature::SmartArt
            }
        )
    }));
    assert!(output.slide.elements.iter().any(|element| {
        matches!(
            &element.kind,
            ElementKind::Image { part_name: Some(name), .. } if name == "ppt/media/preview.emf"
        )
    }));
    for code in [
        ResolveDiagnosticCode::UnsupportedSmartArt,
        ResolveDiagnosticCode::UnsupportedAnimation,
        ResolveDiagnosticCode::UnsupportedTransition,
        ResolveDiagnosticCode::UnsupportedThreeD,
    ] {
        assert!(
            output
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == code),
            "missing diagnostic {code:?}"
        );
    }
}
