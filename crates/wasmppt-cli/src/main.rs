use std::{env, fs, io::BufWriter, path::Path};

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
        Some("validate") => {
            let input = arguments.next().ok_or("validate requires INPUT")?;
            if arguments.next().is_some() {
                return Err("validate accepts exactly one INPUT".to_owned());
            }
            validate(Path::new(&input))
        }
        Some(command) => Err(format!(
            "unknown command {command:?}; use convert or validate"
        )),
    }
}

fn convert(input: &Path, output: &Path) -> Result<(), String> {
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
        .generate_to(&InjectionData::new(), WriteSink::new(BufWriter::new(file)))
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
