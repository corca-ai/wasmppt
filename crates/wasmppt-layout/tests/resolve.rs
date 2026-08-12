use wasmppt_layout::{ElementKind, Fill, PresentationDocument, ResolveDiagnosticCode, RgbaColor};

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
