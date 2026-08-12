//! Compact backend-neutral display lists lowered from resolved slides.

use wasmppt_layout::{
    ElementKind, EmuRect, EmuSize, Fill, GroupTransform, PresetGeometry, ResolveDiagnosticCode,
    ResolveOutput, ResolvedSlide, RgbaColor, Stroke, Transform,
};

pub const DISPLAY_LIST_VERSION: u16 = 2;
const MAGIC: &[u8; 4] = b"WPDL";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImageResource {
    pub part_name: Option<String>,
    pub relationship_id: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SemanticKind {
    Shape,
    Image,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SemanticElement {
    pub first_command: u32,
    pub command_count: u32,
    pub shape_id: u32,
    pub z_order: u32,
    pub kind: SemanticKind,
    pub bounds: EmuRect,
    pub name: String,
    pub alternative_text: Option<String>,
    pub hyperlink: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisplayDiagnostic {
    pub code: ResolveDiagnosticCode,
    pub part_name: String,
    pub shape_id: Option<u32>,
    pub message: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DisplayCommand {
    Clear {
        color: RgbaColor,
    },
    PushGroup {
        transform: u32,
    },
    PopGroup,
    FillPreset {
        geometry: PresetGeometry,
        transform: Transform,
        color: RgbaColor,
    },
    StrokePreset {
        geometry: PresetGeometry,
        transform: Transform,
        stroke: Stroke,
    },
    DrawImage {
        resource: u32,
        transform: Transform,
        crop: wasmppt_layout::ImageCrop,
    },
    DrawText {
        text: u32,
        bounds: EmuRect,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisplayList {
    pub size: EmuSize,
    pub commands: Vec<DisplayCommand>,
    pub group_transforms: Vec<GroupTransform>,
    pub strings: Vec<String>,
    pub images: Vec<ImageResource>,
    pub semantics: Vec<SemanticElement>,
    pub diagnostics: Vec<DisplayDiagnostic>,
}

impl DisplayList {
    pub fn from_slide(slide: &ResolvedSlide) -> Self {
        Self::lower(slide, Vec::new())
    }

    pub fn from_resolve(output: &ResolveOutput) -> Self {
        Self::lower(
            &output.slide,
            output
                .diagnostics
                .iter()
                .map(|diagnostic| DisplayDiagnostic {
                    code: diagnostic.code,
                    part_name: diagnostic.part_name.clone(),
                    shape_id: diagnostic.shape_id,
                    message: diagnostic.message.clone(),
                })
                .collect(),
        )
    }

    fn lower(slide: &ResolvedSlide, diagnostics: Vec<DisplayDiagnostic>) -> Self {
        let mut list = Self {
            size: slide.size,
            commands: vec![DisplayCommand::Clear {
                color: slide.background,
            }],
            group_transforms: Vec::new(),
            strings: Vec::new(),
            images: Vec::new(),
            semantics: Vec::new(),
            diagnostics,
        };
        for element in &slide.elements {
            let first_command = list.commands.len() as u32;
            for transform in &element.group_transforms {
                let index = list.group_transforms.len() as u32;
                list.group_transforms.push(*transform);
                list.commands
                    .push(DisplayCommand::PushGroup { transform: index });
            }
            match &element.kind {
                ElementKind::Shape { geometry } => {
                    if let Fill::Solid(color) = element.fill {
                        list.commands.push(DisplayCommand::FillPreset {
                            geometry: *geometry,
                            transform: element.transform,
                            color,
                        });
                    }
                    if let Some(stroke) = &element.stroke {
                        list.commands.push(DisplayCommand::StrokePreset {
                            geometry: *geometry,
                            transform: element.transform,
                            stroke: stroke.clone(),
                        });
                    }
                }
                ElementKind::Image {
                    relationship_id,
                    part_name,
                    crop,
                } => {
                    let resource = list.images.len() as u32;
                    list.images.push(ImageResource {
                        part_name: part_name.clone(),
                        relationship_id: relationship_id.clone(),
                    });
                    list.commands.push(DisplayCommand::DrawImage {
                        resource,
                        transform: element.transform,
                        crop: *crop,
                    });
                }
            }
            if !element.text.is_empty() {
                let text = list.strings.len() as u32;
                list.strings.push(element.text.clone());
                list.commands.push(DisplayCommand::DrawText {
                    text,
                    bounds: element.transform.bounds,
                });
            }
            for _ in &element.group_transforms {
                list.commands.push(DisplayCommand::PopGroup);
            }
            list.semantics.push(SemanticElement {
                first_command,
                command_count: list.commands.len() as u32 - first_command,
                shape_id: element.id,
                z_order: element.z_order,
                kind: match &element.kind {
                    ElementKind::Shape { .. } => SemanticKind::Shape,
                    ElementKind::Image { .. } => SemanticKind::Image,
                },
                bounds: element.transform.bounds,
                name: element.name.clone(),
                alternative_text: element.alternative_text.clone(),
                hyperlink: element.hyperlink.clone(),
            });
        }
        list
    }

    /// Stable little-endian wire format used at native/Wasm and backend boundaries.
    pub fn encode(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(32 + self.commands.len() * 48);
        bytes.extend_from_slice(MAGIC);
        push_u16(&mut bytes, DISPLAY_LIST_VERSION);
        push_u16(&mut bytes, 0);
        push_i64(&mut bytes, self.size.width);
        push_i64(&mut bytes, self.size.height);
        push_u32(&mut bytes, self.commands.len() as u32);
        push_u32(&mut bytes, self.group_transforms.len() as u32);
        push_u32(&mut bytes, self.strings.len() as u32);
        push_u32(&mut bytes, self.images.len() as u32);
        push_u32(&mut bytes, self.semantics.len() as u32);
        push_u32(&mut bytes, self.diagnostics.len() as u32);
        for command in &self.commands {
            encode_command(&mut bytes, command);
        }
        for transform in &self.group_transforms {
            encode_group_transform(&mut bytes, *transform);
        }
        for string in &self.strings {
            push_blob(&mut bytes, string.as_bytes());
        }
        for image in &self.images {
            push_blob(
                &mut bytes,
                image.part_name.as_deref().unwrap_or_default().as_bytes(),
            );
            push_blob(&mut bytes, image.relationship_id.as_bytes());
        }
        for semantic in &self.semantics {
            push_u32(&mut bytes, semantic.first_command);
            push_u32(&mut bytes, semantic.command_count);
            push_u32(&mut bytes, semantic.shape_id);
            push_u32(&mut bytes, semantic.z_order);
            bytes.push(match semantic.kind {
                SemanticKind::Shape => 1,
                SemanticKind::Image => 2,
            });
            encode_rect(&mut bytes, semantic.bounds);
            push_blob(&mut bytes, semantic.name.as_bytes());
            push_blob(
                &mut bytes,
                semantic
                    .alternative_text
                    .as_deref()
                    .unwrap_or_default()
                    .as_bytes(),
            );
            push_blob(
                &mut bytes,
                semantic.hyperlink.as_deref().unwrap_or_default().as_bytes(),
            );
        }
        for diagnostic in &self.diagnostics {
            bytes.push(diagnostic_code(diagnostic.code));
            push_u32(&mut bytes, diagnostic.shape_id.unwrap_or(u32::MAX));
            push_blob(&mut bytes, diagnostic.part_name.as_bytes());
            push_blob(&mut bytes, diagnostic.message.as_bytes());
        }
        bytes
    }

    /// Deterministic structural signature over the exact binary display list.
    pub fn structural_signature(&self) -> u64 {
        self.encode()
            .into_iter()
            .fold(0xcbf29ce484222325, |hash, byte| {
                (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
            })
    }
}

fn diagnostic_code(code: ResolveDiagnosticCode) -> u8 {
    match code {
        ResolveDiagnosticCode::MissingDependency => 1,
        ResolveDiagnosticCode::InvalidXml => 2,
        ResolveDiagnosticCode::InvalidValue => 3,
        ResolveDiagnosticCode::UnsupportedGraphicFrame => 4,
        ResolveDiagnosticCode::UnsupportedCustomGeometry => 5,
        ResolveDiagnosticCode::UnsupportedFill => 6,
        ResolveDiagnosticCode::UnsupportedEffect => 7,
        ResolveDiagnosticCode::MissingImage => 8,
        _ => 255,
    }
}

fn encode_command(output: &mut Vec<u8>, command: &DisplayCommand) {
    match command {
        DisplayCommand::Clear { color } => {
            output.push(1);
            encode_color(output, *color);
        }
        DisplayCommand::PushGroup { transform } => {
            output.push(2);
            push_u32(output, *transform);
        }
        DisplayCommand::PopGroup => output.push(3),
        DisplayCommand::FillPreset {
            geometry,
            transform,
            color,
        } => {
            output.push(4);
            output.push(geometry_code(*geometry));
            encode_transform(output, *transform);
            encode_color(output, *color);
        }
        DisplayCommand::StrokePreset {
            geometry,
            transform,
            stroke,
        } => {
            output.push(5);
            output.push(geometry_code(*geometry));
            encode_transform(output, *transform);
            encode_color(output, stroke.color);
            push_i64(output, stroke.width);
            push_blob(
                output,
                stroke.dash.as_deref().unwrap_or_default().as_bytes(),
            );
        }
        DisplayCommand::DrawImage {
            resource,
            transform,
            crop,
        } => {
            output.push(6);
            push_u32(output, *resource);
            encode_transform(output, *transform);
            push_i32(output, crop.left);
            push_i32(output, crop.top);
            push_i32(output, crop.right);
            push_i32(output, crop.bottom);
        }
        DisplayCommand::DrawText { text, bounds } => {
            output.push(7);
            push_u32(output, *text);
            encode_rect(output, *bounds);
        }
    }
}

fn encode_group_transform(output: &mut Vec<u8>, transform: GroupTransform) {
    encode_transform(output, transform.outer);
    push_i64(output, transform.child_origin.x);
    push_i64(output, transform.child_origin.y);
    push_i64(output, transform.child_size.width);
    push_i64(output, transform.child_size.height);
}

fn encode_transform(output: &mut Vec<u8>, transform: Transform) {
    encode_rect(output, transform.bounds);
    push_i32(output, transform.rotation);
    output.push(u8::from(transform.flip_horizontal));
    output.push(u8::from(transform.flip_vertical));
}

fn encode_rect(output: &mut Vec<u8>, rect: EmuRect) {
    push_i64(output, rect.origin.x);
    push_i64(output, rect.origin.y);
    push_i64(output, rect.size.width);
    push_i64(output, rect.size.height);
}

fn encode_color(output: &mut Vec<u8>, color: RgbaColor) {
    output.extend_from_slice(&[color.red, color.green, color.blue, color.alpha]);
}

fn geometry_code(geometry: PresetGeometry) -> u8 {
    match geometry {
        PresetGeometry::Rect => 1,
        PresetGeometry::RoundRect => 2,
        PresetGeometry::Ellipse => 3,
        PresetGeometry::Line => 4,
        PresetGeometry::Triangle => 5,
        PresetGeometry::RightTriangle => 6,
        PresetGeometry::Diamond => 7,
        PresetGeometry::Parallelogram => 8,
        PresetGeometry::Hexagon => 9,
        _ => 255,
    }
}

fn push_blob(output: &mut Vec<u8>, value: &[u8]) {
    push_u32(output, value.len() as u32);
    output.extend_from_slice(value);
}

fn push_u16(output: &mut Vec<u8>, value: u16) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn push_u32(output: &mut Vec<u8>, value: u32) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn push_i32(output: &mut Vec<u8>, value: i32) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn push_i64(output: &mut Vec<u8>, value: i64) {
    output.extend_from_slice(&value.to_le_bytes());
}

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
