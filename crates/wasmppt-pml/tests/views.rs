use wasmppt_pml::{PresentationView, SlideView};

#[test]
fn presentation_extracts_slide_relationship_ids_and_retains_extensions() {
    let xml = br#"<p:presentation xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p14="urn:future"><p:sldIdLst><p:sldId id="256" r:id="rId7"/></p:sldIdLst><p:extLst><p:ext uri="future"><p14:unknown answer="42"/></p:ext></p:extLst></p:presentation>"#;
    let view = PresentationView::parse(xml.as_slice()).unwrap();
    assert_eq!(view.slide_relationship_ids(), ["rId7"]);
    assert_eq!(view.document().source(), xml);
}

#[test]
fn slide_exposes_shape_metadata_and_exact_text_ranges() {
    let xml = br#"<p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><p:cSld><p:spTree><p:sp><p:nvSpPr><p:cNvPr id="3" name="Revenue" descr="binding:revenue"/></p:nvSpPr><p:txBody><a:p><a:r><a:t>A &amp; B</a:t></a:r></a:p></p:txBody></p:sp></p:spTree></p:cSld></p:sld>"#;
    let view = SlideView::parse(xml.as_slice()).unwrap();
    assert_eq!(view.shapes().len(), 1);
    let shape = &view.shapes()[0];
    assert_eq!(shape.id, Some(3));
    assert_eq!(shape.name.as_deref(), Some("Revenue"));
    assert_eq!(shape.description.as_deref(), Some("binding:revenue"));
    assert_eq!(shape.text_runs[0].text, "A & B");
    assert_eq!(
        view.document()
            .source_range(shape.text_runs[0].source_range.clone()),
        b"A &amp; B"
    );
}
