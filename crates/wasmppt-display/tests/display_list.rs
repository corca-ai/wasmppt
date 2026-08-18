use wasmppt_display::{DISPLAY_LIST_VERSION, DisplayCommand, DisplayList, SemanticKind};
use wasmppt_layout::PresentationDocument;

const FIXTURE: &[u8] = include_bytes!("../../../fixtures/render/basic.pptx");

#[test]
fn lowers_to_stable_compact_binary_commands_and_side_tables() {
    let resolved = PresentationDocument::open(FIXTURE.to_vec())
        .unwrap()
        .resolve_slide(0)
        .unwrap();
    let display = DisplayList::from_resolve(&resolved);
    assert!(matches!(display.commands[0], DisplayCommand::Clear { .. }));
    assert!(
        display
            .commands
            .iter()
            .any(|command| matches!(command, DisplayCommand::DrawImage { .. }))
    );
    assert!(
        display
            .commands
            .iter()
            .any(|command| matches!(command, DisplayCommand::DrawRichText { .. }))
    );
    let text_style = display
        .commands
        .iter()
        .find_map(|command| match command {
            DisplayCommand::DrawRichText { frame, .. } => frame
                .paragraphs
                .first()
                .and_then(|paragraph| paragraph.runs.first())
                .map(|run| &run.style),
            _ => None,
        })
        .unwrap();
    assert!(text_style.font_size > 0);
    assert!(
        display
            .commands
            .iter()
            .any(|command| matches!(command, DisplayCommand::DrawCustomPath { .. }))
    );
    assert_eq!(display.images.len(), 1);
    assert!(display.strings.is_empty());
    let title = display
        .semantics
        .iter()
        .find(|semantic| semantic.name == "Title")
        .unwrap();
    assert_eq!(title.shape_id, 2);
    assert_eq!(
        title.alternative_text.as_deref(),
        Some("Quarterly report title")
    );
    assert_eq!(
        title.hyperlink.as_deref(),
        Some("https://example.com/report")
    );
    let photo = display
        .semantics
        .iter()
        .find(|semantic| semantic.name == "Photo")
        .unwrap();
    assert_eq!(photo.kind, SemanticKind::Image);
    assert_eq!(
        photo.alternative_text.as_deref(),
        Some("Quarterly report photo")
    );
    assert_eq!(display.diagnostics.len(), 1);
    let encoded = display.encode();
    assert_eq!(&encoded[0..4], b"WPDL");
    assert_eq!(
        u16::from_le_bytes([encoded[4], encoded[5]]),
        DISPLAY_LIST_VERSION
    );
    assert_eq!(display.structural_signature(), 0x0fcb_609b_5b81_f3c8);
}

#[test]
fn lowers_tables_and_supported_chart_kinds_to_shared_primitives() {
    let resolved = PresentationDocument::open(FIXTURE.to_vec())
        .unwrap()
        .resolve_slide(1)
        .unwrap();
    let display = DisplayList::from_resolve(&resolved);
    let table = display
        .semantics
        .iter()
        .find(|semantic| semantic.name == "Sales Table")
        .unwrap();
    assert_eq!(table.kind, SemanticKind::Table);
    assert!(table.command_count >= 10);
    let chart = display
        .semantics
        .iter()
        .find(|semantic| semantic.name == "Sales Chart")
        .unwrap();
    assert_eq!(chart.kind, SemanticKind::Chart);
    assert!(chart.command_count >= 8);
    assert!(
        display
            .commands
            .iter()
            .any(|command| matches!(command, DisplayCommand::StrokePreset { .. }))
    );
    let rich_text = display.commands.iter().filter_map(|command| match command {
        DisplayCommand::DrawRichText { frame, .. } => Some(
            frame
                .paragraphs
                .iter()
                .flat_map(|paragraph| paragraph.runs.iter())
                .map(|run| run.text.as_str())
                .collect::<String>(),
        ),
        _ => None,
    });
    assert!(rich_text.clone().any(|text| text == "Quarter"));
    assert!(rich_text.clone().any(|text| text == "42"));
    let quarter_link = display.commands.iter().find_map(|command| match command {
        DisplayCommand::DrawRichText { frame, .. } => frame
            .paragraphs
            .iter()
            .flat_map(|paragraph| paragraph.runs.iter())
            .find(|run| run.text == "Quarter")
            .and_then(|run| run.hyperlink.as_deref()),
        _ => None,
    });
    assert_eq!(quarter_link, Some("https://example.org/quarter"));
}
