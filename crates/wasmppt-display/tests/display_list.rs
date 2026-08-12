use wasmppt_display::{DISPLAY_LIST_VERSION, DisplayCommand, DisplayList};
use wasmppt_layout::PresentationDocument;

const FIXTURE: &[u8] = include_bytes!("../../../fixtures/render/basic.pptx");

#[test]
fn lowers_to_stable_compact_binary_commands_and_side_tables() {
    let resolved = PresentationDocument::open(FIXTURE.to_vec())
        .unwrap()
        .resolve_slide(0)
        .unwrap();
    let display = DisplayList::from_slide(&resolved.slide);
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
    let encoded = display.encode();
    assert_eq!(&encoded[0..4], b"WPDL");
    assert_eq!(
        u16::from_le_bytes([encoded[4], encoded[5]]),
        DISPLAY_LIST_VERSION
    );
    assert_eq!(display.structural_signature(), 0xa535_92cd_d09d_0945);
}
