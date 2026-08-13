use wasmppt_deck::*;

const PAGE: EmuSize = EmuSize {
    width: 12_192_000,
    height: 6_858_000,
};
const FRAME: EmuRect = EmuRect {
    x: 500_000,
    y: 500_000,
    width: 11_192_000,
    height: 5_858_000,
};

#[test]
fn source_ids_do_not_depend_on_deck_position() {
    let source = SourceRange::new("slides/talk.md", 10, 24);
    let before = StableId::from_source(b"project/document", &source, SemanticRole::Prose);
    let after = StableId::from_source(b"project/document", &source, SemanticRole::Prose);

    assert_eq!(before, after);
    assert_ne!(
        before,
        StableId::from_source(b"project/document", &source, SemanticRole::Title)
    );
    assert_ne!(
        before,
        StableId::from_source(
            b"project/document",
            &SourceRange::new("slides/talk.md", 11, 24),
            SemanticRole::Prose,
        )
    );
}

#[test]
fn validates_a_complete_source_ordered_plan() {
    let spec = simple_spec();
    let template = template_plan();
    let plan = valid_plan(&spec, &template);

    assert!(validate_deck_spec(&spec, &DeckLimits::default()).is_valid());
    assert!(validate_deck_plan(&spec, &template, &plan, &DeckLimits::default()).is_valid());
}

#[test]
fn reports_each_plan_integrity_failure_with_a_stable_code() {
    let spec = simple_spec();
    let template = template_plan();
    let plan = valid_plan(&spec, &template);

    let mut loss = plan.clone();
    loss.pages[2].regions[0].fragments.pop();
    assert_code(
        &spec,
        &template,
        &loss,
        DeckDiagnosticCode::PLAN_SOURCE_LOSS,
    );

    let mut duplication = plan.clone();
    let duplicate = duplication.pages[1].regions[0].fragments[0].clone();
    duplication.pages[1].regions[0].fragments.push(duplicate);
    assert_code(
        &spec,
        &template,
        &duplication,
        DeckDiagnosticCode::PLAN_SOURCE_DUPLICATION,
    );

    let mut reordered = plan.clone();
    reordered.pages[2].regions[0].fragments.swap(0, 1);
    assert_code(
        &spec,
        &template,
        &reordered,
        DeckDiagnosticCode::PLAN_SOURCE_REORDERED,
    );

    let mut drift = plan.clone();
    drift.pages[1].regions[0].fragments[0].source_node_id = id(250);
    assert_code(
        &spec,
        &template,
        &drift,
        DeckDiagnosticCode::PLAN_TARGET_DRIFT,
    );

    let mut layout_drift = plan.clone();
    layout_drift.pages[1].template_layout_id = id(250);
    assert_code(
        &spec,
        &template,
        &layout_drift,
        DeckDiagnosticCode::PLAN_TARGET_DRIFT,
    );

    let mut repeated_table_drift = plan.clone();
    repeated_table_drift.pages[1].regions[0].fragments[0].repeat_table_header_rows = 1;
    assert_code(
        &spec,
        &template,
        &repeated_table_drift,
        DeckDiagnosticCode::PLAN_TARGET_DRIFT,
    );

    let mut geometry = plan.clone();
    geometry.pages[0].regions[0].fragments[0].frame.x = FRAME.x + FRAME.width;
    assert_code(
        &spec,
        &template,
        &geometry,
        DeckDiagnosticCode::PLAN_INVALID_GEOMETRY,
    );

    let mut continuation = plan.clone();
    continuation.pages[1].continuation.total = 3;
    assert_code(
        &spec,
        &template,
        &continuation,
        DeckDiagnosticCode::PLAN_INVALID_CONTINUATION,
    );

    let mut continuation_chrome = plan.clone();
    continuation_chrome.pages[1].continuation.label = Some("continued".to_owned());
    continuation_chrome.pages[1]
        .continuation
        .repeated_heading_node_id = Some(id(3));
    assert_code(
        &spec,
        &template,
        &continuation_chrome,
        DeckDiagnosticCode::PLAN_INVALID_CONTINUATION,
    );

    let mut unstable = plan;
    unstable.pages[0].regions[0].fragments[0].id = id(249);
    assert_code(
        &spec,
        &template,
        &unstable,
        DeckDiagnosticCode::PLAN_UNSTABLE_ID,
    );

    let mut invalid_utf8 = valid_plan(&spec, &template);
    invalid_utf8.pages[1].regions[0].fragments[0].slice = FragmentSlice::Text { start: 0, end: 7 };
    invalid_utf8.pages[1].regions[0].fragments[0].id = PlannedFragment::expected_id(
        spec.logical_slides[1].nodes[0].id,
        FragmentSlice::Text { start: 0, end: 7 },
    );
    invalid_utf8.pages[2].regions[0].fragments[0].slice = FragmentSlice::Text { start: 7, end: 11 };
    invalid_utf8.pages[2].regions[0].fragments[0].id = PlannedFragment::expected_id(
        spec.logical_slides[1].nodes[0].id,
        FragmentSlice::Text { start: 7, end: 11 },
    );
    assert_code(
        &spec,
        &template,
        &invalid_utf8,
        DeckDiagnosticCode::PLAN_SOURCE_LOSS,
    );
}

#[test]
fn validates_source_resource_link_and_chart_contracts() {
    let mut spec = rich_spec();
    spec.logical_slides[0].nodes[0].source.end = 900;
    if let SemanticContent::Text(text) = &mut spec.logical_slides[0].nodes[0].content {
        text.runs[0].hyperlink = Some(SafeHyperlink {
            kind: HyperlinkKind::Web,
            target: "javascript:alert(1)".to_owned(),
        });
    }
    if let SemanticContent::Image(image) = &mut spec.logical_slides[0].nodes[1].content {
        image.resource_id = id(245);
    }
    if let SemanticContent::Chart(chart) = &mut spec.logical_slides[0].nodes[4].content {
        chart.series[0].values[0] = f64::NAN;
    }

    let report = validate_deck_spec(&spec, &DeckLimits::default());
    let codes = report
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect::<Vec<_>>();
    assert!(codes.contains(&DeckDiagnosticCode::INVALID_SOURCE_RANGE));
    assert!(codes.contains(&DeckDiagnosticCode::UNSAFE_HYPERLINK));
    assert!(codes.contains(&DeckDiagnosticCode::MISSING_RESOURCE));
    assert!(codes.contains(&DeckDiagnosticCode::NON_FINITE_CHART_VALUE));
}

#[test]
fn all_contract_payloads_round_trip_deterministically() {
    let spec = rich_spec();
    let template = template_plan_with_unknown_diagnostic();
    let plan = valid_plan(&simple_spec(), &template_plan_with_unknown_diagnostic());
    let limits = DeckLimits::default();

    let validation = validate_deck_spec(&spec, &limits);
    assert!(validation.is_valid(), "{:?}", validation.diagnostics);

    let spec_bytes = spec.encode(&limits).unwrap();
    let template_bytes = template.encode(&limits).unwrap();
    let plan_bytes = plan.encode(&limits).unwrap();
    assert_eq!(&spec_bytes[..4], b"WDSF");
    assert_eq!(&template_bytes[..4], b"WDTP");
    assert_eq!(&plan_bytes[..4], b"WDPL");
    assert_eq!(DeckSpec::decode(&spec_bytes, &limits).unwrap(), spec);
    assert_eq!(
        DeckTemplatePlan::decode(&template_bytes, &limits).unwrap(),
        template
    );
    assert_eq!(DeckPlan::decode(&plan_bytes, &limits).unwrap(), plan);
    assert_eq!(spec.encode(&limits).unwrap(), spec_bytes);
    assert_eq!(template.encode(&limits).unwrap(), template_bytes);
    assert_eq!(plan.encode(&limits).unwrap(), plan_bytes);
    assert_eq!(template.diagnostics[0].code.known_name(), None);
}

#[test]
fn golden_payloads_are_stable() {
    let limits = DeckLimits::default();
    assert_golden(
        rich_spec().encode(&limits).unwrap(),
        include_str!("../../../fixtures/deck-contracts/deck-spec-v2.hex"),
    );
    assert_golden(
        template_plan_with_unknown_diagnostic()
            .encode(&limits)
            .unwrap(),
        include_str!("../../../fixtures/deck-contracts/template-plan-v2.hex"),
    );
    assert_golden(
        valid_plan(&simple_spec(), &template_plan_with_unknown_diagnostic())
            .encode(&limits)
            .unwrap(),
        include_str!("../../../fixtures/deck-contracts/deck-plan-v2.hex"),
    );
}

#[test]
fn decoding_is_bounded_and_fails_closed() {
    let spec = rich_spec();
    let bytes = spec.encode(&DeckLimits::default()).unwrap();
    let tiny = DeckLimits {
        max_payload_bytes: bytes.len() - 1,
        ..DeckLimits::default()
    };
    let error = DeckSpec::decode(&bytes, &tiny).unwrap_err();
    assert_eq!(error.kind(), WireErrorKind::LimitExceeded);
    assert_eq!(error.limit_code(), Some(DeckLimitCode::PAYLOAD_BYTES));

    let mut truncated = bytes.clone();
    truncated.pop();
    assert_eq!(
        DeckSpec::decode(&truncated, &DeckLimits::default())
            .unwrap_err()
            .kind(),
        WireErrorKind::Truncated
    );

    let mut future = bytes;
    future[4..8].copy_from_slice(&3u32.to_le_bytes());
    assert_eq!(
        DeckSpec::decode(&future, &DeckLimits::default())
            .unwrap_err()
            .kind(),
        WireErrorKind::UnsupportedVersion
    );
}

fn assert_code(
    spec: &DeckSpec,
    template: &DeckTemplatePlan,
    plan: &DeckPlan,
    code: DeckDiagnosticCode,
) {
    let report = validate_deck_plan(spec, template, plan, &DeckLimits::default());
    assert!(
        report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == code),
        "missing diagnostic {code:?}: {:?}",
        report.diagnostics
    );
}

fn assert_golden(actual: Vec<u8>, expected: &str) {
    let actual = hex(&actual);
    let expected = expected.trim();
    let first_difference = actual
        .bytes()
        .zip(expected.bytes())
        .position(|(left, right)| left != right)
        .unwrap_or(actual.len().min(expected.len()));
    assert_eq!(
        actual,
        expected,
        "golden differs at hex offset {first_difference}; actual length {}, expected length {}; actual tail {}, expected tail {}",
        actual.len(),
        expected.len(),
        &actual[first_difference.saturating_sub(20)..],
        &expected[first_difference.saturating_sub(20)..]
    );
}

fn simple_spec() -> DeckSpec {
    let title_slide = LogicalSlide {
        id: id(2),
        source: SourceRange::new("talk.md", 0, 20),
        kind: LogicalSlideKind::Title,
        hidden: false,
        nodes: vec![text_node(
            3,
            "talk.md",
            0,
            20,
            SemanticRole::Title,
            "Hello",
            SplitPolicy::Never,
        )],
    };
    let content_slide = LogicalSlide {
        id: id(4),
        source: SourceRange::new("talk.md", 21, 80),
        kind: LogicalSlideKind::Content,
        hidden: true,
        nodes: vec![
            text_node(
                5,
                "talk.md",
                21,
                40,
                SemanticRole::Prose,
                "Alpha βeta",
                SplitPolicy::Text,
            ),
            SemanticNode {
                id: id(6),
                source: SourceRange::new("talk.md", 41, 80),
                role: SemanticRole::List,
                split: SplitPolicy::ListItems,
                content: SemanticContent::List(ListContent {
                    ordered: false,
                    start: 1,
                    items: vec![
                        list_item(7, "talk.md", 41, 60, "One"),
                        list_item(9, "talk.md", 61, 80, "Two"),
                    ],
                }),
            },
        ],
    };
    DeckSpec {
        id: id(1),
        logical_slides: vec![title_slide, content_slide],
        resources: vec![],
    }
}

fn rich_spec() -> DeckSpec {
    let image_id = id(40);
    let svg_id = id(41);
    let nodes = vec![
        SemanticNode {
            id: id(11),
            source: SourceRange::new("deck.md", 0, 20),
            role: SemanticRole::Title,
            split: SplitPolicy::Never,
            content: SemanticContent::Text(RichText {
                runs: vec![RichTextRun {
                    text: "Quarterly ".to_owned(),
                    marks: TextMarks {
                        bold: true,
                        ..TextMarks::default()
                    },
                    hyperlink: Some(SafeHyperlink {
                        kind: HyperlinkKind::Web,
                        target: "https://example.com".to_owned(),
                    }),
                }],
            }),
        },
        SemanticNode {
            id: id(12),
            source: SourceRange::new("deck.md", 21, 40),
            role: SemanticRole::Figure,
            split: SplitPolicy::Never,
            content: SemanticContent::Image(ImageContent {
                resource_id: image_id,
                alt_text: "A chart screenshot".to_owned(),
            }),
        },
        SemanticNode {
            id: id(13),
            source: SourceRange::new("deck.md", 41, 80),
            role: SemanticRole::List,
            split: SplitPolicy::ListItems,
            content: SemanticContent::List(ListContent {
                ordered: true,
                start: 1,
                items: vec![
                    list_item(42, "deck.md", 41, 60, "First"),
                    list_item(44, "deck.md", 61, 80, "Second"),
                ],
            }),
        },
        table_node(),
        SemanticNode {
            id: id(30),
            source: SourceRange::new("deck.md", 181, 220),
            role: SemanticRole::Chart,
            split: SplitPolicy::Never,
            content: SemanticContent::Chart(ChartContent {
                kind: ChartKind::Column,
                categories: vec!["Q1".to_owned(), "Q2".to_owned()],
                series: vec![ChartSeries {
                    name: "Revenue".to_owned(),
                    values: vec![1.5, 2.5],
                }],
            }),
        },
        SemanticNode {
            id: id(31),
            source: SourceRange::new("deck.md", 221, 260),
            role: SemanticRole::Code,
            split: SplitPolicy::Never,
            content: SemanticContent::Code(CodeContent {
                language: Some("rust".to_owned()),
                code: "fn main() {}".to_owned(),
            }),
        },
        SemanticNode {
            id: id(32),
            source: SourceRange::new("deck.md", 261, 300),
            role: SemanticRole::DisplayMath,
            split: SplitPolicy::Never,
            content: SemanticContent::Svg(SvgContent {
                resource_id: svg_id,
                source_text: Some("x^2".to_owned()),
            }),
        },
        SemanticNode {
            id: id(33),
            source: SourceRange::new("deck.md", 301, 380),
            role: SemanticRole::Quote,
            split: SplitPolicy::Children,
            content: SemanticContent::Children(vec![
                text_node(
                    34,
                    "deck.md",
                    301,
                    350,
                    SemanticRole::Prose,
                    "Stay hungry",
                    SplitPolicy::Text,
                ),
                text_node(
                    35,
                    "deck.md",
                    351,
                    380,
                    SemanticRole::Credit,
                    "— Author",
                    SplitPolicy::Never,
                ),
            ]),
        },
    ];
    DeckSpec {
        id: id(10),
        logical_slides: vec![LogicalSlide {
            id: id(36),
            source: SourceRange::new("deck.md", 0, 380),
            kind: LogicalSlideKind::Content,
            hidden: false,
            nodes,
        }],
        resources: vec![
            DeckResource {
                id: image_id,
                kind: ResourceKind::RasterImage,
                media_type: "image/png".to_owned(),
                bytes: vec![1, 2, 3, 4],
                intrinsic_size: Some(PixelSize {
                    width: 640,
                    height: 360,
                }),
            },
            DeckResource {
                id: svg_id,
                kind: ResourceKind::Svg,
                media_type: "image/svg+xml".to_owned(),
                bytes: b"<svg/>".to_vec(),
                intrinsic_size: None,
            },
        ],
    }
}

fn table_node() -> SemanticNode {
    let columns = vec![
        TableColumn {
            id: id(18),
            source: SourceRange::new("deck.md", 81, 90),
        },
        TableColumn {
            id: id(19),
            source: SourceRange::new("deck.md", 91, 100),
        },
    ];
    let rows = (0..2)
        .map(|row| TableRow {
            id: id(20 + row * 3),
            source: SourceRange::new("deck.md", 101 + row as u32 * 40, 140 + row as u32 * 40),
            cells: vec![
                TableCell {
                    id: id(21 + row * 3),
                    source: SourceRange::new(
                        "deck.md",
                        101 + row as u32 * 40,
                        120 + row as u32 * 40,
                    ),
                    content: plain("Region"),
                },
                TableCell {
                    id: id(22 + row * 3),
                    source: SourceRange::new(
                        "deck.md",
                        121 + row as u32 * 40,
                        140 + row as u32 * 40,
                    ),
                    content: plain("Value"),
                },
            ],
        })
        .collect();
    SemanticNode {
        id: id(17),
        source: SourceRange::new("deck.md", 81, 180),
        role: SemanticRole::Table,
        split: SplitPolicy::TableRows,
        content: SemanticContent::Table(TableContent {
            columns,
            header_rows: 1,
            rows,
        }),
    }
}

fn template_plan() -> DeckTemplatePlan {
    DeckTemplatePlan {
        id: id(50),
        template_hash: [7; 32],
        cache_key: [8; 32],
        validator_version: 1,
        compiler_policy: "test-policy".to_owned(),
        page_size: PAGE,
        theme: TemplateTheme::default(),
        layouts: vec![TemplateLayout {
            id: id(52),
            role: TemplateLayoutRole::Content,
            matching_name: "wasmppt:content-v1".to_owned(),
            source_part: "ppt/slideLayouts/slideLayout1.xml".to_owned(),
            master_part: "ppt/slideMasters/slideMaster1.xml".to_owned(),
            region_ids: vec![id(51)],
            asset_ids: vec![],
            background: None,
        }],
        regions: vec![TemplateRegion {
            id: id(51),
            layout_id: id(52),
            role: RegionRole::Body,
            placeholder: PlaceholderIdentity {
                kind: "body".to_owned(),
                index: 1,
            },
            frame: FRAME,
            margins: TextMargins::default(),
            text_levels: vec![],
            accepts: vec![SemanticRole::Title, SemanticRole::Prose, SemanticRole::List],
            required: true,
        }],
        assets: vec![],
        diagnostics: vec![],
    }
}

fn template_plan_with_unknown_diagnostic() -> DeckTemplatePlan {
    let mut template = template_plan();
    template.diagnostics.push(DeckDiagnostic {
        code: DeckDiagnosticCode(65_000),
        severity: DiagnosticSeverity::Warning,
        message: "future diagnostic".to_owned(),
        source: Some(SourceRange::new("template.potx", 0, 4)),
        node_id: None,
        page_id: None,
    });
    template
}

fn valid_plan(spec: &DeckSpec, template: &DeckTemplatePlan) -> DeckPlan {
    let title_slide = &spec.logical_slides[0];
    let content_slide = &spec.logical_slides[1];
    let title = &title_slide.nodes[0];
    let prose = &content_slide.nodes[0];
    let list = &content_slide.nodes[1];
    DeckPlan {
        id: id(60),
        spec_id: spec.id,
        template_id: template.id,
        page_size: PAGE,
        pages: vec![
            page(
                title_slide,
                1,
                1,
                vec![fragment(title.id, FragmentSlice::Whole, 600_000)],
            ),
            page(
                content_slide,
                1,
                2,
                vec![fragment(
                    prose.id,
                    FragmentSlice::Text { start: 0, end: 6 },
                    600_000,
                )],
            ),
            page(
                content_slide,
                2,
                2,
                vec![
                    fragment(prose.id, FragmentSlice::Text { start: 6, end: 11 }, 600_000),
                    fragment(
                        list.id,
                        FragmentSlice::ListItems { start: 0, end: 2 },
                        1_600_000,
                    ),
                ],
            ),
        ],
        diagnostics: vec![],
    }
}

fn page(
    slide: &LogicalSlide,
    ordinal: u32,
    total: u32,
    fragments: Vec<PlannedFragment>,
) -> PhysicalPage {
    PhysicalPage {
        id: slide.id.derive(b"physical-page", ordinal),
        logical_slide_id: slide.id,
        template_layout_id: id(52),
        hidden: slide.hidden,
        continuation: Continuation {
            ordinal,
            total,
            repeated_heading_node_id: None,
            label: (total > 1).then(|| format!("{ordinal}/{total}")),
        },
        regions: vec![PlannedRegion {
            template_region_id: id(51),
            frame: FRAME,
            fragments,
        }],
    }
}

fn fragment(source_node_id: StableId, slice: FragmentSlice, y: Emu) -> PlannedFragment {
    PlannedFragment {
        id: PlannedFragment::expected_id(source_node_id, slice),
        source_node_id,
        slice,
        frame: EmuRect {
            x: 600_000,
            y,
            width: 10_000_000,
            height: 800_000,
        },
        type_choice: TypeChoice {
            font_size: 2_400,
            columns: 1,
            fit: ContentFit::None,
        },
        repeat_table_header_rows: 0,
    }
}

fn text_node(
    identity: u8,
    source: &str,
    start: u32,
    end: u32,
    role: SemanticRole,
    text: &str,
    split: SplitPolicy,
) -> SemanticNode {
    SemanticNode {
        id: id(identity),
        source: SourceRange::new(source, start, end),
        role,
        split,
        content: SemanticContent::Text(plain(text)),
    }
}

fn list_item(identity: u8, source: &str, start: u32, end: u32, text: &str) -> ListItem {
    ListItem {
        id: id(identity),
        source: SourceRange::new(source, start, end),
        blocks: vec![SemanticNode {
            id: id(identity + 1),
            source: SourceRange::new(source, start, end),
            role: SemanticRole::ListItem,
            split: SplitPolicy::Never,
            content: SemanticContent::Text(plain(text)),
        }],
        children: vec![],
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

fn id(value: u8) -> StableId {
    StableId::from_bytes([value; 16])
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
