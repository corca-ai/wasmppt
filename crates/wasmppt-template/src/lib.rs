//! Immutable, versioned template plans compiled from loss-aware PowerPoint packages.

use std::{collections::HashMap, ops::Range};

use sha2::{Digest, Sha256};
use wasmppt_opc::{DiagnosticCode as GraphDiagnosticCode, PackageGraph, ReadAt, ZipArchive};
use wasmppt_pml::{ShapeView, SlideView};
use wasmppt_xml::{TokenKind, XmlDocument};

mod inject;
mod payload;
mod policy;

pub use inject::{
    ChartData, ChartSeriesData, GenerateError, GenerateErrorCode, GenerateOutput, GenerateStats,
    GenerationCursor, ImageCrop, ImageData, ImageFitPolicy, InjectionData, LiveSession,
    LiveSessionUpdate, OverlayStats, PreparedOverlay, PreparedTemplate, RichTextRunData,
    SemanticShapeData, TableOverflowPolicy, TablePolicyData,
};
pub use payload::{INJECTION_SCHEMA_VERSION, InjectionDecodeError};

pub const PLAN_SCHEMA_VERSION: u32 = 2;
pub const BINDING_SCHEMA_VERSION: u32 = 2;
pub const MANIFEST_PART: &str = "wasmppt/bindings.xml";
const ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MacroPolicy {
    Strip,
    Reject,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompilerOptions {
    pub macro_policy: MacroPolicy,
    pub allow_visible_tokens: bool,
}

impl Default for CompilerOptions {
    fn default() -> Self {
        Self {
            macro_policy: MacroPolicy::Strip,
            allow_visible_tokens: true,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlanIdentity {
    pub template_sha256: [u8; 32],
    pub engine_version: String,
    pub binding_schema: u32,
    pub macro_policy: MacroPolicy,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BindingKind {
    Text,
    Image,
    Chart,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum BindingSource {
    VisibleToken,
    ShapeMetadata,
    Manifest,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StylePolicy {
    PreserveFirstRun,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RelationshipAction {
    None,
    ReplaceImage { relationship_id: String },
    ReplaceChart { relationship_id: String },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TextSpan {
    pub source_range: Range<u32>,
    pub decoded_start: u32,
    pub decoded_end: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BindingTarget {
    pub id: String,
    pub kind: BindingKind,
    pub source: BindingSource,
    pub part_name: String,
    pub shape_id: Option<u32>,
    pub shape_name: Option<String>,
    pub text_spans: Vec<TextSpan>,
    pub style_policy: StylePolicy,
    pub relationship_action: RelationshipAction,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Completeness {
    pub graph_valid: bool,
    pub bindings_unambiguous: bool,
    pub raw_copy_partition_complete: bool,
    pub unknown_markup_preserved: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TemplatePlan {
    pub schema_version: u32,
    pub identity: PlanIdentity,
    pub completeness: Completeness,
    pub bindings: Vec<BindingTarget>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum BindingDiagnosticCode {
    MissingTarget,
    DuplicateId,
    AmbiguousTarget,
    UnsupportedKind,
    InvalidManifest,
    InvalidSlide,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BindingDiagnostic {
    pub code: BindingDiagnosticCode,
    pub binding_id: Option<String>,
    pub part_name: Option<String>,
    pub message: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompileOutput {
    pub plan: TemplatePlan,
    pub diagnostics: Vec<BindingDiagnostic>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum CompileErrorCode {
    InvalidTemplate,
    MacroPresent,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompileError {
    code: CompileErrorCode,
    message: String,
    cause_code: Option<&'static str>,
}

impl CompileError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            code: CompileErrorCode::InvalidTemplate,
            message: message.into(),
            cause_code: None,
        }
    }

    fn package(error: wasmppt_opc::Error) -> Self {
        Self {
            code: CompileErrorCode::InvalidTemplate,
            message: error.to_string(),
            cause_code: Some(opc_error_code(error.code())),
        }
    }

    fn macro_present(message: impl Into<String>) -> Self {
        Self {
            code: CompileErrorCode::MacroPresent,
            message: message.into(),
            cause_code: None,
        }
    }

    pub const fn code(&self) -> CompileErrorCode {
        self.code
    }

    pub const fn cause_code(&self) -> Option<&'static str> {
        self.cause_code
    }
}

impl std::fmt::Display for CompileError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for CompileError {}

pub struct TemplateCompiler {
    options: CompilerOptions,
}

impl TemplateCompiler {
    pub fn new(options: CompilerOptions) -> Self {
        Self { options }
    }

    pub fn compile<S: ReadAt>(
        &self,
        archive: &ZipArchive<S>,
    ) -> Result<CompileOutput, CompileError> {
        if self.options.macro_policy == MacroPolicy::Reject {
            if let Some(reason) = policy::prohibited_content(archive).map_err(CompileError::new)? {
                return Err(CompileError::macro_present(reason));
            }
        }
        let identity = PlanIdentity {
            template_sha256: hash_source(archive.source())?,
            engine_version: ENGINE_VERSION.to_owned(),
            binding_schema: BINDING_SCHEMA_VERSION,
            macro_policy: self.options.macro_policy,
        };
        let graph = PackageGraph::build(archive)
            .map_err(|error| CompileError::new(format!("cannot build package graph: {error}")))?;
        let graph_valid = !graph.diagnostics().iter().any(|diagnostic| {
            matches!(
                diagnostic.code,
                GraphDiagnosticCode::MissingContentTypes
                    | GraphDiagnosticCode::InvalidContentTypesXml
                    | GraphDiagnosticCode::InvalidRelationshipsXml
                    | GraphDiagnosticCode::DuplicateRelationshipId
                    | GraphDiagnosticCode::MissingRelationshipTarget
                    | GraphDiagnosticCode::InvalidRelationshipTarget
            )
        });
        let mut diagnostics = Vec::new();
        let manifest = load_manifest(archive, &mut diagnostics)?;
        let mut candidates = Vec::new();
        let mut shapes_by_part = HashMap::<String, Vec<ShapeView>>::new();

        let mut slide_entries = archive
            .entries()
            .iter()
            .filter(|entry| {
                entry.name.starts_with("ppt/slides/")
                    && entry.name.ends_with(".xml")
                    && !entry.name.contains("/_rels/")
            })
            .collect::<Vec<_>>();
        slide_entries.sort_unstable_by(|left, right| left.name.cmp(&right.name));
        for entry in slide_entries {
            let bytes = archive.read_entry(entry).map_err(|error| {
                let mut compile_error = CompileError::package(error);
                compile_error.message = format!(
                    "cannot read slide {}: {}",
                    entry.name, compile_error.message
                );
                compile_error
            })?;
            match SlideView::parse(bytes) {
                Ok(slide) => {
                    for shape in slide.shapes() {
                        if let Some(description) = &shape.description {
                            if let Some((kind, id)) = metadata_binding(description) {
                                match kind {
                                    BindingKind::Text => {
                                        if shape.text_runs.is_empty() {
                                            diagnostics.push(missing_text_target(
                                                id,
                                                &entry.name,
                                                "text binding shape has no writable text runs",
                                            ));
                                        } else {
                                            candidates.push(candidate_for_text(
                                                id,
                                                BindingSource::ShapeMetadata,
                                                &entry.name,
                                                shape,
                                                None,
                                            ));
                                        }
                                    }
                                    BindingKind::Image => {
                                        if let Some(relationship_id) = &shape.image_relationship_id
                                        {
                                            candidates.push(candidate_for_image(
                                                id,
                                                BindingSource::ShapeMetadata,
                                                &entry.name,
                                                shape,
                                                relationship_id,
                                            ));
                                        } else {
                                            diagnostics.push(BindingDiagnostic {
                                                code: BindingDiagnosticCode::MissingTarget,
                                                binding_id: Some(id),
                                                part_name: Some(entry.name.clone()),
                                                message: "image binding shape has no embedded image relationship".to_owned(),
                                            });
                                        }
                                    }
                                    BindingKind::Chart => {
                                        if let Some(relationship_id) = &shape.chart_relationship_id
                                        {
                                            candidates.push(candidate_for_chart(
                                                id,
                                                BindingSource::ShapeMetadata,
                                                &entry.name,
                                                shape,
                                                relationship_id,
                                            ));
                                        } else {
                                            diagnostics.push(BindingDiagnostic {
                                                code: BindingDiagnosticCode::MissingTarget,
                                                binding_id: Some(id),
                                                part_name: Some(entry.name.clone()),
                                                message:
                                                    "chart binding frame has no chart relationship"
                                                        .to_owned(),
                                            });
                                        }
                                    }
                                }
                            }
                        }
                        if self.options.allow_visible_tokens {
                            candidates.extend(visible_candidates(&entry.name, shape));
                        }
                    }
                    shapes_by_part.insert(entry.name.clone(), slide.shapes().to_vec());
                }
                Err(error) => diagnostics.push(BindingDiagnostic {
                    code: BindingDiagnosticCode::InvalidSlide,
                    binding_id: None,
                    part_name: Some(entry.name.clone()),
                    message: error.to_string(),
                }),
            }
        }

        for declaration in manifest {
            let kind = match declaration.kind.as_str() {
                "text" => BindingKind::Text,
                "image" => BindingKind::Image,
                "chart" => BindingKind::Chart,
                _ => {
                    diagnostics.push(BindingDiagnostic {
                        code: BindingDiagnosticCode::UnsupportedKind,
                        binding_id: Some(declaration.id),
                        part_name: Some(declaration.part),
                        message: format!("unsupported binding kind {}", declaration.kind),
                    });
                    continue;
                }
            };
            let matches = shapes_by_part
                .get(&declaration.part)
                .into_iter()
                .flatten()
                .filter(|shape| {
                    declaration.shape_id.is_none_or(|id| shape.id == Some(id))
                        && declaration
                            .shape_name
                            .as_deref()
                            .is_none_or(|name| shape.name.as_deref() == Some(name))
                })
                .collect::<Vec<_>>();
            match matches.as_slice() {
                [] => diagnostics.push(BindingDiagnostic {
                    code: BindingDiagnosticCode::MissingTarget,
                    binding_id: Some(declaration.id),
                    part_name: Some(declaration.part),
                    message: "manifest binding target was not found".to_owned(),
                }),
                [shape] => match kind {
                    BindingKind::Text => {
                        if shape.text_runs.is_empty() {
                            diagnostics.push(missing_text_target(
                                declaration.id,
                                &declaration.part,
                                "manifest text binding shape has no writable text runs",
                            ));
                        } else {
                            candidates.push(candidate_for_text(
                                declaration.id,
                                BindingSource::Manifest,
                                &declaration.part,
                                shape,
                                None,
                            ));
                        }
                    }
                    BindingKind::Image => match &shape.image_relationship_id {
                        Some(relationship_id) => candidates.push(candidate_for_image(
                            declaration.id,
                            BindingSource::Manifest,
                            &declaration.part,
                            shape,
                            relationship_id,
                        )),
                        None => diagnostics.push(BindingDiagnostic {
                            code: BindingDiagnosticCode::MissingTarget,
                            binding_id: Some(declaration.id),
                            part_name: Some(declaration.part),
                            message: "image binding shape has no embedded image relationship"
                                .to_owned(),
                        }),
                    },
                    BindingKind::Chart => match &shape.chart_relationship_id {
                        Some(relationship_id) => candidates.push(candidate_for_chart(
                            declaration.id,
                            BindingSource::Manifest,
                            &declaration.part,
                            shape,
                            relationship_id,
                        )),
                        None => diagnostics.push(BindingDiagnostic {
                            code: BindingDiagnosticCode::MissingTarget,
                            binding_id: Some(declaration.id),
                            part_name: Some(declaration.part),
                            message: "chart binding frame has no chart relationship".to_owned(),
                        }),
                    },
                },
                _ => diagnostics.push(BindingDiagnostic {
                    code: BindingDiagnosticCode::AmbiguousTarget,
                    binding_id: Some(declaration.id),
                    part_name: Some(declaration.part),
                    message: "manifest selector matches more than one shape".to_owned(),
                }),
            }
        }

        let bindings = finalize_candidates(candidates, &mut diagnostics);
        let bindings_unambiguous = !diagnostics.iter().any(|diagnostic| {
            matches!(
                diagnostic.code,
                BindingDiagnosticCode::DuplicateId
                    | BindingDiagnosticCode::AmbiguousTarget
                    | BindingDiagnosticCode::MissingTarget
                    | BindingDiagnosticCode::UnsupportedKind
                    | BindingDiagnosticCode::InvalidManifest
            )
        });
        Ok(CompileOutput {
            plan: TemplatePlan {
                schema_version: PLAN_SCHEMA_VERSION,
                identity,
                completeness: Completeness {
                    graph_valid,
                    bindings_unambiguous,
                    raw_copy_partition_complete: graph_valid,
                    unknown_markup_preserved: true,
                },
                bindings,
            },
            diagnostics,
        })
    }
}

#[derive(Clone)]
struct Candidate {
    target: BindingTarget,
}

fn missing_text_target(id: String, part_name: &str, message: &str) -> BindingDiagnostic {
    BindingDiagnostic {
        code: BindingDiagnosticCode::MissingTarget,
        binding_id: Some(id),
        part_name: Some(part_name.to_owned()),
        message: message.to_owned(),
    }
}

fn candidate_for_text(
    id: String,
    source: BindingSource,
    part_name: &str,
    shape: &ShapeView,
    spans: Option<Vec<TextSpan>>,
) -> Candidate {
    Candidate {
        target: BindingTarget {
            id,
            kind: BindingKind::Text,
            source,
            part_name: part_name.to_owned(),
            shape_id: shape.id,
            shape_name: shape.name.clone(),
            text_spans: spans.unwrap_or_else(|| {
                shape
                    .text_runs
                    .iter()
                    .map(|run| TextSpan {
                        source_range: to_u32_range(run.source_range.clone()),
                        decoded_start: 0,
                        decoded_end: u32::try_from(run.text.len()).unwrap_or(u32::MAX),
                    })
                    .collect()
            }),
            style_policy: StylePolicy::PreserveFirstRun,
            relationship_action: RelationshipAction::None,
        },
    }
}

fn candidate_for_image(
    id: String,
    source: BindingSource,
    part_name: &str,
    shape: &ShapeView,
    relationship_id: &str,
) -> Candidate {
    Candidate {
        target: BindingTarget {
            id,
            kind: BindingKind::Image,
            source,
            part_name: part_name.to_owned(),
            shape_id: shape.id,
            shape_name: shape.name.clone(),
            text_spans: Vec::new(),
            style_policy: StylePolicy::PreserveFirstRun,
            relationship_action: RelationshipAction::ReplaceImage {
                relationship_id: relationship_id.to_owned(),
            },
        },
    }
}

fn candidate_for_chart(
    id: String,
    source: BindingSource,
    part_name: &str,
    shape: &ShapeView,
    relationship_id: &str,
) -> Candidate {
    Candidate {
        target: BindingTarget {
            id,
            kind: BindingKind::Chart,
            source,
            part_name: part_name.to_owned(),
            shape_id: shape.id,
            shape_name: shape.name.clone(),
            text_spans: Vec::new(),
            style_policy: StylePolicy::PreserveFirstRun,
            relationship_action: RelationshipAction::ReplaceChart {
                relationship_id: relationship_id.to_owned(),
            },
        },
    }
}

fn visible_candidates(part_name: &str, shape: &ShapeView) -> Vec<Candidate> {
    let joined = shape
        .text_runs
        .iter()
        .map(|run| run.text.as_str())
        .collect::<String>();
    let mut output = Vec::new();
    let mut cursor = 0;
    while let Some(relative) = joined[cursor..].find("{{") {
        let start = cursor + relative;
        let Some(close) = joined[start + 2..].find("}}") else {
            break;
        };
        let end = start + 2 + close + 2;
        let id = &joined[start + 2..end - 2];
        if valid_binding_id(id) {
            let spans = map_joined_span(shape, start..end);
            output.push(candidate_for_text(
                id.to_owned(),
                BindingSource::VisibleToken,
                part_name,
                shape,
                Some(spans),
            ));
        }
        cursor = end;
    }
    output
}

fn map_joined_span(shape: &ShapeView, wanted: Range<usize>) -> Vec<TextSpan> {
    let mut offset = 0usize;
    let mut spans = Vec::new();
    for run in &shape.text_runs {
        let run_range = offset..offset + run.text.len();
        let start = wanted.start.max(run_range.start);
        let end = wanted.end.min(run_range.end);
        if start < end {
            spans.push(TextSpan {
                source_range: to_u32_range(run.source_range.clone()),
                decoded_start: u32::try_from(start - run_range.start).unwrap_or(u32::MAX),
                decoded_end: u32::try_from(end - run_range.start).unwrap_or(u32::MAX),
            });
        }
        offset = run_range.end;
    }
    spans
}

fn finalize_candidates(
    mut candidates: Vec<Candidate>,
    diagnostics: &mut Vec<BindingDiagnostic>,
) -> Vec<BindingTarget> {
    candidates.sort_unstable_by(|left, right| {
        left.target
            .id
            .cmp(&right.target.id)
            .then(right.target.source.cmp(&left.target.source))
            .then(left.target.part_name.cmp(&right.target.part_name))
            .then(left.target.shape_id.cmp(&right.target.shape_id))
    });
    let mut output = Vec::new();
    let mut cursor = 0;
    while cursor < candidates.len() {
        let id = candidates[cursor].target.id.clone();
        let end = candidates[cursor..]
            .iter()
            .position(|candidate| candidate.target.id != id)
            .map_or(candidates.len(), |relative| cursor + relative);
        let group = &candidates[cursor..end];
        let highest = group[0].target.source;
        let preferred = group
            .iter()
            .filter(|candidate| candidate.target.source == highest)
            .collect::<Vec<_>>();
        if preferred.len() == 1 {
            output.push(preferred[0].target.clone());
        } else {
            diagnostics.push(BindingDiagnostic {
                code: BindingDiagnosticCode::DuplicateId,
                binding_id: Some(id),
                part_name: None,
                message: "binding ID resolves to multiple equally preferred targets".to_owned(),
            });
        }
        cursor = end;
    }
    output
}

fn metadata_binding(description: &str) -> Option<(BindingKind, String)> {
    [
        ("wasmppt:text:", BindingKind::Text),
        ("wasmppt:image:", BindingKind::Image),
        ("wasmppt:chart:", BindingKind::Chart),
    ]
    .into_iter()
    .find_map(|(prefix, kind)| {
        description
            .strip_prefix(prefix)
            .filter(|id| valid_binding_id(id))
            .map(|id| (kind, id.to_owned()))
    })
}

fn valid_binding_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
}

#[derive(Clone, Debug)]
struct ManifestBinding {
    id: String,
    kind: String,
    part: String,
    shape_id: Option<u32>,
    shape_name: Option<String>,
}

fn load_manifest<S: ReadAt>(
    archive: &ZipArchive<S>,
    diagnostics: &mut Vec<BindingDiagnostic>,
) -> Result<Vec<ManifestBinding>, CompileError> {
    let Some(entry) = archive.entry(MANIFEST_PART) else {
        return Ok(Vec::new());
    };
    let bytes = archive.read_entry(entry).map_err(|error| {
        let mut compile_error = CompileError::package(error);
        compile_error.message = format!("cannot read binding manifest: {}", compile_error.message);
        compile_error
    })?;
    let document = match XmlDocument::parse(bytes) {
        Ok(document) => document,
        Err(error) => {
            diagnostics.push(BindingDiagnostic {
                code: BindingDiagnosticCode::InvalidManifest,
                binding_id: None,
                part_name: Some(MANIFEST_PART.to_owned()),
                message: error.to_string(),
            });
            return Ok(Vec::new());
        }
    };
    let mut bindings = Vec::new();
    for token in document.tokens() {
        let TokenKind::Start {
            name, attributes, ..
        } = &token.kind
        else {
            continue;
        };
        if name.local != "bind" {
            continue;
        }
        let attr = |local: &str| {
            document
                .attribute(attributes, None, local)
                .map(|attribute| attribute.value.as_str())
        };
        let (Some(id), Some(kind), Some(part)) = (attr("id"), attr("kind"), attr("part")) else {
            diagnostics.push(BindingDiagnostic {
                code: BindingDiagnosticCode::InvalidManifest,
                binding_id: None,
                part_name: Some(MANIFEST_PART.to_owned()),
                message: "manifest bind requires id, kind, and part".to_owned(),
            });
            continue;
        };
        let part = part.trim_start_matches('/').to_owned();
        let shape_id = attr("shapeId").and_then(|value| value.parse().ok());
        let shape_name = attr("shapeName").map(str::to_owned);
        if !valid_binding_id(id) || (shape_id.is_none() && shape_name.is_none()) {
            diagnostics.push(BindingDiagnostic {
                code: BindingDiagnosticCode::InvalidManifest,
                binding_id: Some(id.to_owned()),
                part_name: Some(part),
                message: "manifest binding has an invalid ID or no shape selector".to_owned(),
            });
            continue;
        }
        bindings.push(ManifestBinding {
            id: id.to_owned(),
            kind: kind.to_owned(),
            part,
            shape_id,
            shape_name,
        });
    }
    Ok(bindings)
}

fn hash_source<S: ReadAt>(source: &S) -> Result<[u8; 32], CompileError> {
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 64 * 1024];
    let mut offset = 0u64;
    while offset < source.len() {
        let amount = usize::try_from((source.len() - offset).min(buffer.len() as u64))
            .expect("hash chunk fits usize");
        source
            .read_at(offset, &mut buffer[..amount])
            .map_err(|error| {
                let mut compile_error = CompileError::package(error);
                compile_error.message = format!("cannot hash template: {}", compile_error.message);
                compile_error
            })?;
        hasher.update(&buffer[..amount]);
        offset += amount as u64;
    }
    Ok(hasher.finalize().into())
}

const fn opc_error_code(code: wasmppt_opc::ErrorCode) -> &'static str {
    use wasmppt_opc::ErrorCode;
    match code {
        ErrorCode::Io => "io",
        ErrorCode::Truncated => "truncated",
        ErrorCode::InvalidSignature => "invalid-signature",
        ErrorCode::InvalidField => "invalid-field",
        ErrorCode::InvalidPath => "invalid-path",
        ErrorCode::DuplicateEntry => "duplicate-entry",
        ErrorCode::UnsupportedCompression => "unsupported-compression",
        ErrorCode::UnsupportedEncryption => "unsupported-encryption",
        ErrorCode::UnsupportedMultiDisk => "unsupported-multi-disk",
        ErrorCode::UnsupportedZip64 => "unsupported-zip64",
        ErrorCode::LimitExceeded => "limit-exceeded",
        ErrorCode::OverlappingEntries => "overlapping-entries",
        ErrorCode::ChecksumMismatch => "checksum-mismatch",
        ErrorCode::SizeMismatch => "size-mismatch",
        _ => "unknown",
    }
}

const fn xml_error_code(code: wasmppt_xml::XmlErrorCode) -> &'static str {
    use wasmppt_xml::XmlErrorCode;
    match code {
        XmlErrorCode::InvalidUtf8 => "invalid-utf8",
        XmlErrorCode::Truncated => "truncated",
        XmlErrorCode::InvalidSyntax => "invalid-syntax",
        XmlErrorCode::UndeclaredPrefix => "undeclared-prefix",
        XmlErrorCode::MismatchedEndTag => "mismatched-end-tag",
        XmlErrorCode::DtdForbidden => "dtd-forbidden",
        XmlErrorCode::Entity => "entity",
        XmlErrorCode::LimitExceeded => "limit-exceeded",
        _ => "unknown",
    }
}

fn to_u32_range(range: Range<usize>) -> Range<u32> {
    u32::try_from(range.start).unwrap_or(u32::MAX)..u32::try_from(range.end).unwrap_or(u32::MAX)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReuseDecision {
    Reuse,
    Recompile(PlanMismatch),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlanMismatch {
    Schema,
    Template,
    Engine,
    BindingSchema,
    MacroPolicy,
    Incomplete,
}

impl TemplatePlan {
    pub fn reuse_decision(&self, expected: &PlanIdentity) -> ReuseDecision {
        let mismatch = if self.schema_version != PLAN_SCHEMA_VERSION {
            Some(PlanMismatch::Schema)
        } else if self.identity.template_sha256 != expected.template_sha256 {
            Some(PlanMismatch::Template)
        } else if self.identity.engine_version != expected.engine_version {
            Some(PlanMismatch::Engine)
        } else if self.identity.binding_schema != expected.binding_schema {
            Some(PlanMismatch::BindingSchema)
        } else if self.identity.macro_policy != expected.macro_policy {
            Some(PlanMismatch::MacroPolicy)
        } else if !self.completeness.graph_valid
            || !self.completeness.bindings_unambiguous
            || !self.completeness.raw_copy_partition_complete
            || !self.completeness.unknown_markup_preserved
        {
            Some(PlanMismatch::Incomplete)
        } else {
            None
        };
        mismatch.map_or(ReuseDecision::Reuse, ReuseDecision::Recompile)
    }

    pub fn structural_signature(&self) -> [u8; 32] {
        Sha256::digest(self.encode()).into()
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"WPPL");
        put_u32(&mut bytes, self.schema_version);
        bytes.extend_from_slice(&self.identity.template_sha256);
        put_string(&mut bytes, &self.identity.engine_version);
        put_u32(&mut bytes, self.identity.binding_schema);
        bytes.push(self.identity.macro_policy as u8);
        let flags = u8::from(self.completeness.graph_valid)
            | (u8::from(self.completeness.bindings_unambiguous) << 1)
            | (u8::from(self.completeness.raw_copy_partition_complete) << 2)
            | (u8::from(self.completeness.unknown_markup_preserved) << 3);
        bytes.push(flags);
        put_u32(&mut bytes, self.bindings.len() as u32);
        for binding in &self.bindings {
            put_string(&mut bytes, &binding.id);
            bytes.push(binding.kind as u8);
            bytes.push(binding.source as u8);
            put_string(&mut bytes, &binding.part_name);
            put_option_u32(&mut bytes, binding.shape_id);
            put_option_string(&mut bytes, binding.shape_name.as_deref());
            bytes.push(binding.style_policy as u8);
            match &binding.relationship_action {
                RelationshipAction::None => bytes.push(0),
                RelationshipAction::ReplaceImage { relationship_id } => {
                    bytes.push(1);
                    put_string(&mut bytes, relationship_id);
                }
                RelationshipAction::ReplaceChart { relationship_id } => {
                    bytes.push(2);
                    put_string(&mut bytes, relationship_id);
                }
            }
            put_u32(&mut bytes, binding.text_spans.len() as u32);
            for span in &binding.text_spans {
                put_u32(&mut bytes, span.source_range.start);
                put_u32(&mut bytes, span.source_range.end);
                put_u32(&mut bytes, span.decoded_start);
                put_u32(&mut bytes, span.decoded_end);
            }
        }
        bytes
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, PlanDecodeError> {
        PlanReader::new(bytes).read_plan()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlanDecodeError;

impl std::fmt::Display for PlanDecodeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("invalid or unsupported TemplatePlan bytes")
    }
}

impl std::error::Error for PlanDecodeError {}

struct PlanReader<'a> {
    bytes: &'a [u8],
    cursor: usize,
}

impl<'a> PlanReader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, cursor: 0 }
    }

    fn read_plan(mut self) -> Result<TemplatePlan, PlanDecodeError> {
        if self.take(4)? != b"WPPL" {
            return Err(PlanDecodeError);
        }
        let schema_version = self.u32()?;
        if schema_version != PLAN_SCHEMA_VERSION {
            return Err(PlanDecodeError);
        }
        let template_sha256 = self.take(32)?.try_into().map_err(|_| PlanDecodeError)?;
        let engine_version = self.string()?;
        let binding_schema = self.u32()?;
        let macro_policy = match self.byte()? {
            0 => MacroPolicy::Strip,
            1 => MacroPolicy::Reject,
            _ => return Err(PlanDecodeError),
        };
        let flags = self.byte()?;
        let count = self.u32()? as usize;
        if count > 100_000 {
            return Err(PlanDecodeError);
        }
        let mut bindings = Vec::with_capacity(count);
        for _ in 0..count {
            let id = self.string()?;
            let kind = match self.byte()? {
                0 => BindingKind::Text,
                1 => BindingKind::Image,
                2 => BindingKind::Chart,
                _ => return Err(PlanDecodeError),
            };
            let source = match self.byte()? {
                0 => BindingSource::VisibleToken,
                1 => BindingSource::ShapeMetadata,
                2 => BindingSource::Manifest,
                _ => return Err(PlanDecodeError),
            };
            let part_name = self.string()?;
            let shape_id = self.option_u32()?;
            let shape_name = self.option_string()?;
            let style_policy = match self.byte()? {
                0 => StylePolicy::PreserveFirstRun,
                _ => return Err(PlanDecodeError),
            };
            let relationship_action = match self.byte()? {
                0 => RelationshipAction::None,
                1 => RelationshipAction::ReplaceImage {
                    relationship_id: self.string()?,
                },
                2 => RelationshipAction::ReplaceChart {
                    relationship_id: self.string()?,
                },
                _ => return Err(PlanDecodeError),
            };
            let span_count = self.u32()? as usize;
            if span_count > 100_000 {
                return Err(PlanDecodeError);
            }
            let mut text_spans = Vec::with_capacity(span_count);
            for _ in 0..span_count {
                text_spans.push(TextSpan {
                    source_range: self.u32()?..self.u32()?,
                    decoded_start: self.u32()?,
                    decoded_end: self.u32()?,
                });
            }
            bindings.push(BindingTarget {
                id,
                kind,
                source,
                part_name,
                shape_id,
                shape_name,
                text_spans,
                style_policy,
                relationship_action,
            });
        }
        if self.cursor != self.bytes.len() {
            return Err(PlanDecodeError);
        }
        Ok(TemplatePlan {
            schema_version,
            identity: PlanIdentity {
                template_sha256,
                engine_version,
                binding_schema,
                macro_policy,
            },
            completeness: Completeness {
                graph_valid: flags & 1 != 0,
                bindings_unambiguous: flags & 2 != 0,
                raw_copy_partition_complete: flags & 4 != 0,
                unknown_markup_preserved: flags & 8 != 0,
            },
            bindings,
        })
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], PlanDecodeError> {
        let end = self.cursor.checked_add(length).ok_or(PlanDecodeError)?;
        let value = self.bytes.get(self.cursor..end).ok_or(PlanDecodeError)?;
        self.cursor = end;
        Ok(value)
    }

    fn byte(&mut self) -> Result<u8, PlanDecodeError> {
        Ok(self.take(1)?[0])
    }

    fn u32(&mut self) -> Result<u32, PlanDecodeError> {
        Ok(u32::from_le_bytes(
            self.take(4)?.try_into().map_err(|_| PlanDecodeError)?,
        ))
    }

    fn string(&mut self) -> Result<String, PlanDecodeError> {
        let length = self.u32()? as usize;
        if length > 16 * 1024 * 1024 {
            return Err(PlanDecodeError);
        }
        std::str::from_utf8(self.take(length)?)
            .map(str::to_owned)
            .map_err(|_| PlanDecodeError)
    }

    fn option_u32(&mut self) -> Result<Option<u32>, PlanDecodeError> {
        match self.byte()? {
            0 => Ok(None),
            1 => Ok(Some(self.u32()?)),
            _ => Err(PlanDecodeError),
        }
    }

    fn option_string(&mut self) -> Result<Option<String>, PlanDecodeError> {
        match self.byte()? {
            0 => Ok(None),
            1 => Ok(Some(self.string()?)),
            _ => Err(PlanDecodeError),
        }
    }
}

fn put_u32(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn put_string(bytes: &mut Vec<u8>, value: &str) {
    put_u32(bytes, value.len() as u32);
    bytes.extend_from_slice(value.as_bytes());
}

fn put_option_u32(bytes: &mut Vec<u8>, value: Option<u32>) {
    match value {
        Some(value) => {
            bytes.push(1);
            put_u32(bytes, value);
        }
        None => bytes.push(0),
    }
}

fn put_option_string(bytes: &mut Vec<u8>, value: Option<&str>) {
    match value {
        Some(value) => {
            bytes.push(1);
            put_string(bytes, value);
        }
        None => bytes.push(0),
    }
}

/// Host-owned persistence port. IndexedDB, KV, files, and databases stay in adapters.
pub trait BinaryPlanStore {
    type Error;

    fn load(&self, key: &[u8; 32]) -> Result<Option<Vec<u8>>, Self::Error>;
    fn store(&self, key: &[u8; 32], bytes: &[u8]) -> Result<(), Self::Error>;
}

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
