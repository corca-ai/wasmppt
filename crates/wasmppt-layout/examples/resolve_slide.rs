use std::{env, fs};

use wasmppt_layout::PresentationDocument;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let input = env::args_os()
        .nth(1)
        .ok_or("usage: resolve_slide INPUT [INDEX]")?;
    let index = env::args()
        .nth(2)
        .map(|value| value.parse::<usize>())
        .transpose()?
        .unwrap_or(0);
    let document = PresentationDocument::open(fs::read(input)?)?;
    let resolved = document.resolve_slide(index)?;
    println!(
        "slide {index}: {} elements, {} diagnostics, {} parsed XML parts",
        resolved.slide.elements.len(),
        resolved.diagnostics.len(),
        resolved.trace.parsed_xml_parts.len()
    );
    Ok(())
}
