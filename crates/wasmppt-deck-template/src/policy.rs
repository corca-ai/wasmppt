use wasmppt_opc::{ReadAt, ZipArchive};
use wasmppt_xml::{TokenKind, XmlDocument};

pub(crate) fn inspect_active_content<S: ReadAt>(archive: &ZipArchive<S>) -> Vec<String> {
    let mut problems = Vec::new();
    for entry in archive.entries() {
        if prohibited_part(&entry.name) {
            problems.push(format!("prohibited active-content part {}", entry.name));
        }
        if entry.name != "[Content_Types].xml"
            && !entry.name.ends_with(".rels")
            && !entry.name.ends_with(".xml")
        {
            continue;
        }
        let source = match archive.read_entry(entry) {
            Ok(source) => source,
            Err(error) => {
                problems.push(format!("cannot inspect {}: {error}", entry.name));
                continue;
            }
        };
        let document = match XmlDocument::parse(source) {
            Ok(document) => document,
            Err(error) => {
                problems.push(format!("cannot inspect {}: {error}", entry.name));
                continue;
            }
        };
        for token in document.tokens() {
            let TokenKind::Start { attributes, .. } = &token.kind else {
                continue;
            };
            for attribute in attributes {
                let lower = attribute.value.to_ascii_lowercase();
                let prohibited = (entry.name == "[Content_Types].xml"
                    && attribute.name.local == "ContentType"
                    && prohibited_content_type(&lower))
                    || (entry.name.ends_with(".rels")
                        && attribute.name.local == "Type"
                        && prohibited_relationship_type(&lower))
                    || (attribute.name.local == "action"
                        && (lower.contains("macro") || lower.contains("program")));
                if prohibited {
                    problems.push(format!(
                        "prohibited active-content metadata in {}",
                        entry.name
                    ));
                }
            }
        }
    }
    problems.sort();
    problems.dedup();
    problems
}

fn prohibited_part(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.contains("vbaproject")
        || lower.contains("vbadata")
        || lower.contains("/activex/")
        || lower.contains("/embeddings/")
        || lower.starts_with("customui/")
        || lower.starts_with("_xmlsignatures/")
        || lower.ends_with("origin.sigs")
}

fn prohibited_content_type(lower: &str) -> bool {
    lower.contains("macroenabled")
        || lower.contains("vba")
        || lower.contains("activex")
        || lower.contains("oleobject")
        || lower.contains("digital-signature")
}

fn prohibited_relationship_type(lower: &str) -> bool {
    [
        "vbaproject",
        "vbadata",
        "activex",
        "oleobject",
        "customui",
        "digital-signature",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
        || lower.ends_with("/package")
        || lower.ends_with("/control")
}
