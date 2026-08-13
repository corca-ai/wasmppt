use std::{env, fs, io::BufWriter, path::Path};

use wasmppt_display::DisplayList;
use wasmppt_layout::{PresentationDocument, ResolveDiagnosticCode};
use wasmppt_opc::{DiagnosticCode, PackageGraph, WriteSink, ZipArchive};
use wasmppt_template::{InjectionData, PreparedTemplate, TemplateCompiler};
use wasmppt_xml::XmlDocument;

fn main() {
    if let Err(error) = run() {
        eprintln!("wasmppt: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut arguments = env::args().skip(1);
    match arguments.next().as_deref() {
        None | Some("--version" | "-V") => {
            println!("wasmppt {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Some("convert") => {
            let input = arguments
                .next()
                .ok_or("convert requires INPUT and OUTPUT")?;
            let output = arguments
                .next()
                .ok_or("convert requires INPUT and OUTPUT")?;
            if arguments.next().is_some() {
                return Err("convert accepts exactly INPUT and OUTPUT".to_owned());
            }
            convert(Path::new(&input), Path::new(&output))
        }
        Some("inject-text") => {
            let input = arguments
                .next()
                .ok_or("inject-text requires INPUT OUTPUT BINDING_ID VALUE")?;
            let output = arguments
                .next()
                .ok_or("inject-text requires INPUT OUTPUT BINDING_ID VALUE")?;
            let binding = arguments
                .next()
                .ok_or("inject-text requires INPUT OUTPUT BINDING_ID VALUE")?;
            let value = arguments
                .next()
                .ok_or("inject-text requires INPUT OUTPUT BINDING_ID VALUE")?;
            if arguments.next().is_some() {
                return Err("inject-text accepts exactly INPUT OUTPUT BINDING_ID VALUE".to_owned());
            }
            inject_text(Path::new(&input), Path::new(&output), &binding, &value)
        }
        Some("validate") => {
            let input = arguments.next().ok_or("validate requires INPUT")?;
            if arguments.next().is_some() {
                return Err("validate accepts exactly one INPUT".to_owned());
            }
            validate(Path::new(&input))
        }
        Some("audit-macro-free") => {
            let input = arguments.next().ok_or("audit-macro-free requires INPUT")?;
            if arguments.next().is_some() {
                return Err("audit-macro-free accepts exactly one INPUT".to_owned());
            }
            audit_macro_free(Path::new(&input))
        }
        Some("resolve") => {
            let input = arguments
                .next()
                .ok_or("resolve requires INPUT and SLIDE_INDEX")?;
            let slide_index = arguments
                .next()
                .ok_or("resolve requires INPUT and SLIDE_INDEX")?
                .parse::<usize>()
                .map_err(|_| "SLIDE_INDEX must be a non-negative integer")?;
            if arguments.next().is_some() {
                return Err("resolve accepts exactly INPUT and SLIDE_INDEX".to_owned());
            }
            resolve(Path::new(&input), slide_index)
        }
        Some(command) => Err(format!(
            "unknown command {command:?}; use convert, inject-text, validate, audit-macro-free, or resolve"
        )),
    }
}

fn audit_macro_free(input: &Path) -> Result<(), String> {
    let bytes =
        fs::read(input).map_err(|error| format!("cannot read {}: {error}", input.display()))?;
    let archive = ZipArchive::from_bytes(bytes).map_err(|error| error.to_string())?;
    let prohibited_parts = archive
        .entries()
        .iter()
        .filter(|entry| {
            let lower = entry.name.to_ascii_lowercase();
            lower.contains("vbaproject")
                || lower.contains("vbadata")
                || lower.starts_with("_xmlsignatures/")
                || lower.ends_with("origin.sigs")
        })
        .map(|entry| entry.name.as_str())
        .collect::<Vec<_>>();
    if !prohibited_parts.is_empty() {
        return Err(format!(
            "forbidden macro or signature parts: {}",
            prohibited_parts.join(", ")
        ));
    }
    for entry in archive.entries().iter().filter(|entry| {
        entry.name.ends_with(".xml")
            || entry.name.ends_with(".rels")
            || entry.name == "[Content_Types].xml"
    }) {
        let content = archive
            .read_entry(entry)
            .map_err(|error| error.to_string())?;
        let lower = String::from_utf8_lossy(&content).to_ascii_lowercase();
        if lower.contains("vbaproject")
            || lower.contains("vbadata")
            || lower.contains("digital-signature")
            || lower.contains("action=\"ppaction://macro")
        {
            return Err(format!(
                "forbidden macro, signature, or Action reference in {}",
                entry.name
            ));
        }
    }
    println!("macro-free: {} entries", archive.entries().len());
    Ok(())
}

fn resolve(input: &Path, slide_index: usize) -> Result<(), String> {
    let bytes =
        fs::read(input).map_err(|error| format!("cannot read {}: {error}", input.display()))?;
    let deck = PresentationDocument::open(bytes).map_err(|error| error.to_string())?;
    let resolved = deck
        .resolve_slide(slide_index)
        .map_err(|error| error.to_string())?;
    let display = DisplayList::from_resolve(&resolved);
    println!(
        "resolved slide {slide_index}: {} commands, {} diagnostics, {} parsed parts, signature {:016x}",
        display.commands.len(),
        resolved.diagnostics.len(),
        resolved.trace.parsed_xml_parts.len(),
        display.structural_signature()
    );
    for diagnostic in &resolved.diagnostics {
        eprintln!(
            "render {} {} shape {:?}: {}",
            resolve_diagnostic_code(diagnostic.code),
            diagnostic.part_name,
            diagnostic.shape_id,
            diagnostic.message
        );
    }
    for element in &resolved.slide.elements {
        println!(
            "shape {} {:?} {:?}: {}",
            element.id, element.name, element.source, element.text
        );
        for property in &element.provenance {
            println!("  {} <- {:?}", property.property, property.source);
        }
    }
    Ok(())
}

const fn resolve_diagnostic_code(code: ResolveDiagnosticCode) -> &'static str {
    match code {
        ResolveDiagnosticCode::MissingDependency => "missing-dependency",
        ResolveDiagnosticCode::InvalidXml => "invalid-xml",
        ResolveDiagnosticCode::InvalidValue => "invalid-value",
        ResolveDiagnosticCode::UnsupportedGraphicFrame => "unsupported-graphic-frame",
        ResolveDiagnosticCode::UnsupportedCustomGeometry => "unsupported-custom-geometry",
        ResolveDiagnosticCode::UnsupportedFill => "unsupported-fill",
        ResolveDiagnosticCode::UnsupportedEffect => "unsupported-effect",
        ResolveDiagnosticCode::MissingImage => "missing-image",
        ResolveDiagnosticCode::UnsupportedSmartArt => "unsupported-smart-art",
        ResolveDiagnosticCode::UnsupportedMetafile => "unsupported-metafile",
        ResolveDiagnosticCode::UnsupportedAnimation => "unsupported-animation",
        ResolveDiagnosticCode::UnsupportedTransition => "unsupported-transition",
        ResolveDiagnosticCode::UnsupportedActiveContent => "unsupported-active-content",
        ResolveDiagnosticCode::UnsupportedThreeD => "unsupported-three-d",
        ResolveDiagnosticCode::UnsupportedChartKind => "unsupported-chart-kind",
        _ => "unknown",
    }
}

fn convert(input: &Path, output: &Path) -> Result<(), String> {
    generate(input, output, &InjectionData::new())
}

fn inject_text(input: &Path, output: &Path, binding: &str, value: &str) -> Result<(), String> {
    let data = InjectionData::new().with_text(binding, value);
    generate(input, output, &data)
}

fn generate(input: &Path, output: &Path, data: &InjectionData) -> Result<(), String> {
    let bytes =
        fs::read(input).map_err(|error| format!("cannot read {}: {error}", input.display()))?;
    let archive = ZipArchive::from_bytes(bytes.clone()).map_err(|error| error.to_string())?;
    let compiled = TemplateCompiler::new(Default::default())
        .compile(&archive)
        .map_err(|error| error.to_string())?;
    if !compiled.diagnostics.is_empty() {
        for diagnostic in &compiled.diagnostics {
            eprintln!("binding {:?}: {}", diagnostic.code, diagnostic.message);
        }
    }
    let prepared =
        PreparedTemplate::new(bytes, compiled.plan).map_err(|error| error.to_string())?;
    let file = fs::File::create(output)
        .map_err(|error| format!("cannot create {}: {error}", output.display()))?;
    let (_, stats) = prepared
        .generate_to(data, WriteSink::new(BufWriter::new(file)))
        .map_err(|error| error.to_string())?;
    println!(
        "wrote {}: {} raw-copied, {} rewritten, {} removed",
        output.display(),
        stats.zip.raw_copied_entries,
        stats.rewritten_entries,
        stats.removed_entries
    );
    Ok(())
}

fn validate(input: &Path) -> Result<(), String> {
    let bytes =
        fs::read(input).map_err(|error| format!("cannot read {}: {error}", input.display()))?;
    let archive = ZipArchive::from_bytes(bytes).map_err(|error| error.to_string())?;
    for entry in archive.entries().iter().filter(|entry| {
        entry.name.ends_with(".xml")
            || entry.name.ends_with(".rels")
            || entry.name == "[Content_Types].xml"
    }) {
        let bytes = archive
            .read_entry(entry)
            .map_err(|error| error.to_string())?;
        XmlDocument::parse(bytes).map_err(|error| format!("{}: {error}", entry.name))?;
    }
    let graph = PackageGraph::build(&archive).map_err(|error| error.to_string())?;
    let fatal = graph.diagnostics().iter().filter(|diagnostic| {
        matches!(
            diagnostic.code,
            DiagnosticCode::MissingContentTypes
                | DiagnosticCode::InvalidContentTypesXml
                | DiagnosticCode::InvalidContentTypesRoot
                | DiagnosticCode::DuplicateContentType
                | DiagnosticCode::MissingContentType
                | DiagnosticCode::InvalidRelationshipsXml
                | DiagnosticCode::InvalidRelationshipsRoot
                | DiagnosticCode::DuplicateRelationshipId
                | DiagnosticCode::InvalidRelationshipTarget
                | DiagnosticCode::MissingRelationshipTarget
                | DiagnosticCode::MixedConformance
        )
    });
    let messages = fatal
        .map(|diagnostic| diagnostic.message.as_str())
        .collect::<Vec<_>>();
    if !messages.is_empty() {
        return Err(format!(
            "package graph validation failed: {}",
            messages.join("; ")
        ));
    }
    println!(
        "valid: {} entries, {} semantic parts, {:?}",
        archive.entries().len(),
        graph.parts().len(),
        graph.conformance()
    );
    Ok(())
}
