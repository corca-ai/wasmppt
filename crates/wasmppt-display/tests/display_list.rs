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
            .any(|command| matches!(command, DisplayCommand::DrawText { .. }))
    );
    assert_eq!(display.images.len(), 1);
    assert_eq!(display.strings, ["Actual title"]);
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
    assert_eq!(display.diagnostics.len(), 2);
    let encoded = display.encode();
    assert_eq!(&encoded[0..4], b"WPDL");
    assert_eq!(
        u16::from_le_bytes([encoded[4], encoded[5]]),
        DISPLAY_LIST_VERSION
    );
    assert_eq!(display.structural_signature(), 0x9862_98b7_5076_e847);
}
