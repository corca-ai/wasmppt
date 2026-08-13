use wasmppt_opc::{ReadAt, ZipArchive};
use wasmppt_xml::{TokenKind, XmlDocument};

pub(crate) fn prohibited_content<S: ReadAt>(
    archive: &ZipArchive<S>,
) -> Result<Option<String>, String> {
    for entry in archive.entries() {
        if prohibited_part(&entry.name) {
            return Ok(Some(format!("prohibited package part {}", entry.name)));
        }
        if entry.name != "[Content_Types].xml"
            && !entry.name.ends_with(".rels")
            && !entry.name.ends_with(".xml")
        {
            continue;
        }

        let source = archive
            .read_entry(entry)
            .map_err(|error| format!("cannot inspect {}: {error}", entry.name))?;
        let document = XmlDocument::parse(source)
            .map_err(|error| format!("cannot inspect {}: {error}", entry.name))?;
        for token in document.tokens() {
            let TokenKind::Start { attributes, .. } = &token.kind else {
                continue;
            };
            for attribute in attributes {
                let prohibited = (entry.name == "[Content_Types].xml"
                    && attribute.name.local == "ContentType"
                    && prohibited_content_type(&attribute.value))
                    || (entry.name.ends_with(".rels")
                        && attribute.name.local == "Type"
                        && prohibited_relationship_type(&attribute.value))
                    || (attribute.name.local == "action"
                        && attribute.value.to_ascii_lowercase().contains("macro"));
                if prohibited {
                    return Ok(Some(format!(
                        "prohibited package metadata in {}",
                        entry.name
                    )));
                }
            }
        }
    }
    Ok(None)
}

pub(crate) fn prohibited_part(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.contains("vbaproject")
        || lower.contains("vbadata")
        || lower.starts_with("_xmlsignatures/")
        || lower.ends_with("origin.sigs")
}

pub(crate) fn prohibited_content_type(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.contains("vba") || lower.contains("digital-signature")
}

pub(crate) fn prohibited_relationship_type(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.contains("vbaproject") || lower.contains("vbadata") || lower.contains("digital-signature")
}

pub(crate) fn is_template_main_type(value: &str) -> bool {
    matches!(
        value,
        "application/vnd.openxmlformats-officedocument.presentationml.template.main+xml"
            | "application/vnd.ms-powerpoint.template.macroEnabled.main+xml"
            | "application/vnd.ms-powerpoint.presentation.macroEnabled.main+xml"
    )
}
