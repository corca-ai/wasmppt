//! Compact backend-neutral display lists lowered from resolved slides.

use wasmppt_layout::{
    ChartKind, CustomPath, ElementKind, EmuPoint, EmuRect, EmuSize, Fill, GradientStop,
    GroupTransform, LineEnd, OuterShadow, PathCommand, PreservedFeature, PresetGeometry,
    ResolveDiagnosticCode, ResolveOutput, ResolvedChart, ResolvedSlide, ResolvedTable,
    ResolvedTextFrame, ResolvedTextStyle, RgbaColor, Stroke, TextAlignment, TextAutofit,
    TextVerticalAlignment, Transform,
};

pub const DISPLAY_LIST_VERSION: u16 = 4;
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
    Table,
    Chart,
    PreservedGraphic,
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
        style: ResolvedTextStyle,
    },
    DrawRichText {
        bounds: EmuRect,
        frame: ResolvedTextFrame,
    },
    FillGradientPreset {
        geometry: PresetGeometry,
        transform: Transform,
        angle: i32,
        stops: Vec<GradientStop>,
    },
    DrawCustomPath {
        path: CustomPath,
        transform: Transform,
        fill: Fill,
        stroke: Option<Stroke>,
    },
    DrawOuterShadow {
        geometry: PresetGeometry,
        transform: Transform,
        shadow: OuterShadow,
    },
    DrawUnsupported {
        transform: Transform,
        feature: PreservedFeature,
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
                    if let Some(shadow) = element.outer_shadow {
                        list.commands.push(DisplayCommand::DrawOuterShadow {
                            geometry: *geometry,
                            transform: element.transform,
                            shadow,
                        });
                    }
                    if let Some(path) = &element.custom_path {
                        list.commands.push(DisplayCommand::DrawCustomPath {
                            path: path.clone(),
                            transform: element.transform,
                            fill: element.fill.clone(),
                            stroke: element.stroke.clone(),
                        });
                    } else {
                        match &element.fill {
                            Fill::Solid(color) => list.commands.push(DisplayCommand::FillPreset {
                                geometry: *geometry,
                                transform: element.transform,
                                color: *color,
                            }),
                            Fill::LinearGradient { angle, stops } => {
                                list.commands.push(DisplayCommand::FillGradientPreset {
                                    geometry: *geometry,
                                    transform: element.transform,
                                    angle: *angle,
                                    stops: stops.clone(),
                                });
                            }
                            Fill::None => {}
                        }
                        if let Some(stroke) = &element.stroke {
                            list.commands.push(DisplayCommand::StrokePreset {
                                geometry: *geometry,
                                transform: element.transform,
                                stroke: stroke.clone(),
                            });
                        }
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
                ElementKind::Table { table } => lower_table(&mut list, element.transform, table),
                ElementKind::Chart { chart } => lower_chart(&mut list, element.transform, chart),
                ElementKind::PreservedGraphic { feature } => {
                    list.commands.push(DisplayCommand::DrawUnsupported {
                        transform: element.transform,
                        feature: *feature,
                    });
                }
            }
            if let Some(frame) = element.text_frame.as_ref().filter(|frame| {
                frame
                    .paragraphs
                    .iter()
                    .any(|paragraph| !paragraph.runs.is_empty())
            }) {
                list.commands.push(DisplayCommand::DrawRichText {
                    bounds: element.transform.bounds,
                    frame: frame.clone(),
                });
            } else if !element.text.is_empty() {
                let text = list.strings.len() as u32;
                list.strings.push(element.text.clone());
                list.commands.push(DisplayCommand::DrawText {
                    text,
                    bounds: element.transform.bounds,
                    style: element.text_style.clone(),
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
                    ElementKind::Table { .. } => SemanticKind::Table,
                    ElementKind::Chart { .. } => SemanticKind::Chart,
                    ElementKind::PreservedGraphic { .. } => SemanticKind::PreservedGraphic,
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
                SemanticKind::Table => 3,
                SemanticKind::Chart => 4,
                SemanticKind::PreservedGraphic => 5,
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

fn lower_table(list: &mut DisplayList, transform: Transform, table: &ResolvedTable) {
    let bounds = transform.bounds;
    let column_count = table.column_widths.len().max(
        table
            .rows
            .iter()
            .map(|row| row.cells.len())
            .max()
            .unwrap_or(0),
    );
    if column_count == 0 || table.rows.is_empty() {
        return;
    }
    let declared_width: i64 = table.column_widths.iter().sum();
    let column_widths = if table.column_widths.len() == column_count && declared_width > 0 {
        table
            .column_widths
            .iter()
            .map(|width| bounds.size.width.saturating_mul(*width) / declared_width)
            .collect::<Vec<_>>()
    } else {
        vec![bounds.size.width / column_count as i64; column_count]
    };
    let declared_height: i64 = table.rows.iter().map(|row| row.height).sum();
    let row_heights = table
        .rows
        .iter()
        .map(|row| {
            if declared_height > 0 {
                bounds.size.height.saturating_mul(row.height) / declared_height
            } else {
                bounds.size.height / table.rows.len() as i64
            }
        })
        .collect::<Vec<_>>();
    let mut y = bounds.origin.y;
    for (row_index, row) in table.rows.iter().enumerate() {
        let mut x = bounds.origin.x;
        let height = row_heights[row_index];
        let mut column_index = 0;
        for cell in &row.cells {
            let span = cell.column_span.max(1) as usize;
            let width = column_widths
                .iter()
                .skip(column_index)
                .take(span)
                .sum::<i64>();
            let row_span = cell.row_span.max(1) as usize;
            let cell_height = row_heights
                .iter()
                .skip(row_index)
                .take(row_span)
                .sum::<i64>();
            let cell_transform = Transform {
                bounds: EmuRect {
                    origin: EmuPoint { x, y },
                    size: EmuSize {
                        width,
                        height: cell_height,
                    },
                },
                rotation: transform.rotation,
                flip_horizontal: transform.flip_horizontal,
                flip_vertical: transform.flip_vertical,
            };
            list.commands.push(DisplayCommand::FillPreset {
                geometry: PresetGeometry::Rect,
                transform: cell_transform,
                color: cell.fill,
            });
            list.commands.push(DisplayCommand::StrokePreset {
                geometry: PresetGeometry::Rect,
                transform: cell_transform,
                stroke: Stroke {
                    color: RgbaColor {
                        red: 127,
                        green: 127,
                        blue: 127,
                        alpha: 255,
                    },
                    width: 9_525,
                    dash: None,
                    head_end: None,
                    tail_end: None,
                },
            });
            if !cell.text.is_empty() {
                let text = list.strings.len() as u32;
                list.strings.push(cell.text.clone());
                list.commands.push(DisplayCommand::DrawText {
                    text,
                    bounds: cell_transform.bounds,
                    style: ResolvedTextStyle::default(),
                });
            }
            x = x.saturating_add(width);
            column_index += span;
        }
        y = y.saturating_add(height);
    }
}

fn lower_chart(list: &mut DisplayList, transform: Transform, chart: &ResolvedChart) {
    if chart.series.is_empty() {
        return;
    }
    let bounds = transform.bounds;
    let padding_x = bounds.size.width / 12;
    let padding_y = bounds.size.height / 10;
    let plot = EmuRect {
        origin: EmuPoint {
            x: bounds.origin.x + padding_x,
            y: bounds.origin.y + padding_y,
        },
        size: EmuSize {
            width: bounds.size.width - padding_x * 2,
            height: bounds.size.height - padding_y * 2,
        },
    };
    let maximum = chart
        .series
        .iter()
        .flat_map(|series| series.values.iter())
        .copied()
        .fold(0.0_f64, |maximum, value| maximum.max(value.abs()))
        .max(1.0);
    match chart.kind {
        ChartKind::Column => {
            let category_count = chart
                .series
                .iter()
                .map(|series| series.values.len())
                .max()
                .unwrap_or(0);
            if category_count == 0 {
                return;
            }
            let slot = plot.size.width / category_count as i64;
            let bar_width = (slot * 4 / 5) / chart.series.len() as i64;
            for (series_index, series) in chart.series.iter().enumerate() {
                for (value_index, value) in series.values.iter().enumerate() {
                    let height = ((value.abs() / maximum) * plot.size.height as f64) as i64;
                    let x = plot.origin.x
                        + slot * value_index as i64
                        + slot / 10
                        + bar_width * series_index as i64;
                    push_chart_rect(
                        list,
                        transform,
                        x,
                        plot.origin.y + plot.size.height - height,
                        bar_width,
                        height,
                        series.color,
                    );
                }
            }
        }
        ChartKind::Bar => {
            let category_count = chart
                .series
                .iter()
                .map(|series| series.values.len())
                .max()
                .unwrap_or(0);
            if category_count == 0 {
                return;
            }
            let slot = plot.size.height / category_count as i64;
            let bar_height = (slot * 4 / 5) / chart.series.len() as i64;
            for (series_index, series) in chart.series.iter().enumerate() {
                for (value_index, value) in series.values.iter().enumerate() {
                    let width = ((value.abs() / maximum) * plot.size.width as f64) as i64;
                    let y = plot.origin.y
                        + slot * value_index as i64
                        + slot / 10
                        + bar_height * series_index as i64;
                    push_chart_rect(
                        list,
                        transform,
                        plot.origin.x,
                        y,
                        width,
                        bar_height,
                        series.color,
                    );
                }
            }
        }
        ChartKind::Line => {
            for series in &chart.series {
                let denominator = series.values.len().saturating_sub(1).max(1) as i64;
                for (index, values) in series.values.windows(2).enumerate() {
                    let x1 = plot.origin.x + plot.size.width * index as i64 / denominator;
                    let x2 = plot.origin.x + plot.size.width * (index as i64 + 1) / denominator;
                    let y1 = plot.origin.y + plot.size.height
                        - ((values[0].abs() / maximum) * plot.size.height as f64) as i64;
                    let y2 = plot.origin.y + plot.size.height
                        - ((values[1].abs() / maximum) * plot.size.height as f64) as i64;
                    list.commands.push(DisplayCommand::StrokePreset {
                        geometry: PresetGeometry::Line,
                        transform: Transform {
                            bounds: EmuRect {
                                origin: EmuPoint { x: x1, y: y1 },
                                size: EmuSize {
                                    width: x2 - x1,
                                    height: y2 - y1,
                                },
                            },
                            rotation: transform.rotation,
                            flip_horizontal: transform.flip_horizontal,
                            flip_vertical: transform.flip_vertical,
                        },
                        stroke: Stroke {
                            color: series.color,
                            width: 19_050,
                            dash: None,
                            head_end: None,
                            tail_end: None,
                        },
                    });
                }
            }
        }
        _ => {}
    }
}

fn push_chart_rect(
    list: &mut DisplayList,
    parent: Transform,
    x: i64,
    y: i64,
    width: i64,
    height: i64,
    color: RgbaColor,
) {
    list.commands.push(DisplayCommand::FillPreset {
        geometry: PresetGeometry::Rect,
        transform: Transform {
            bounds: EmuRect {
                origin: EmuPoint { x, y },
                size: EmuSize { width, height },
            },
            rotation: parent.rotation,
            flip_horizontal: parent.flip_horizontal,
            flip_vertical: parent.flip_vertical,
        },
        color,
    });
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
        ResolveDiagnosticCode::UnsupportedSmartArt => 9,
        ResolveDiagnosticCode::UnsupportedMetafile => 10,
        ResolveDiagnosticCode::UnsupportedAnimation => 11,
        ResolveDiagnosticCode::UnsupportedTransition => 12,
        ResolveDiagnosticCode::UnsupportedActiveContent => 13,
        ResolveDiagnosticCode::UnsupportedThreeD => 14,
        ResolveDiagnosticCode::UnsupportedChartKind => 15,
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
            output.push(line_end_code(stroke.head_end));
            output.push(line_end_code(stroke.tail_end));
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
        DisplayCommand::DrawText {
            text,
            bounds,
            style,
        } => {
            output.push(7);
            push_u32(output, *text);
            encode_rect(output, *bounds);
            encode_text_style(output, style);
        }
        DisplayCommand::DrawUnsupported { transform, feature } => {
            output.push(8);
            encode_transform(output, *transform);
            output.push(match feature {
                PreservedFeature::SmartArt => 1,
                PreservedFeature::Metafile => 2,
                PreservedFeature::OleObject => 3,
                PreservedFeature::UnknownGraphicFrame => 4,
            });
        }
        DisplayCommand::DrawRichText { bounds, frame } => {
            output.push(9);
            encode_rect(output, *bounds);
            output.push(vertical_alignment_code(frame.vertical_alignment));
            push_i64(output, frame.margin_left);
            push_i64(output, frame.margin_top);
            push_i64(output, frame.margin_right);
            push_i64(output, frame.margin_bottom);
            output.push(u8::from(frame.wrap));
            output.push(match frame.autofit {
                TextAutofit::None => 0,
                TextAutofit::ShrinkText => 1,
                TextAutofit::ResizeShape => 2,
            });
            push_u32(output, frame.paragraphs.len() as u32);
            for paragraph in &frame.paragraphs {
                output.push(text_alignment_code(paragraph.alignment));
                push_blob(
                    output,
                    paragraph.bullet.as_deref().unwrap_or_default().as_bytes(),
                );
                output.push(paragraph.level);
                push_i64(output, paragraph.margin_left);
                push_i64(output, paragraph.indent);
                push_i32(output, paragraph.line_spacing.unwrap_or(i32::MIN));
                push_i32(output, paragraph.space_before.unwrap_or(i32::MIN));
                push_i32(output, paragraph.space_after.unwrap_or(i32::MIN));
                push_u32(output, paragraph.runs.len() as u32);
                for run in &paragraph.runs {
                    push_blob(output, run.text.as_bytes());
                    encode_text_style(output, &run.style);
                    push_blob(
                        output,
                        run.east_asian_font_family
                            .as_deref()
                            .unwrap_or_default()
                            .as_bytes(),
                    );
                    push_blob(
                        output,
                        run.complex_script_font_family
                            .as_deref()
                            .unwrap_or_default()
                            .as_bytes(),
                    );
                }
            }
        }
        DisplayCommand::FillGradientPreset {
            geometry,
            transform,
            angle,
            stops,
        } => {
            output.push(10);
            output.push(geometry_code(*geometry));
            encode_transform(output, *transform);
            push_i32(output, *angle);
            encode_gradient_stops(output, stops);
        }
        DisplayCommand::DrawCustomPath {
            path,
            transform,
            fill,
            stroke,
        } => {
            output.push(11);
            encode_transform(output, *transform);
            push_i64(output, path.size.width);
            push_i64(output, path.size.height);
            push_u32(output, path.commands.len() as u32);
            for command in &path.commands {
                match command {
                    PathCommand::MoveTo(point) => {
                        output.push(1);
                        push_i64(output, point.x);
                        push_i64(output, point.y);
                    }
                    PathCommand::LineTo(point) => {
                        output.push(2);
                        push_i64(output, point.x);
                        push_i64(output, point.y);
                    }
                    PathCommand::Close => output.push(3),
                }
            }
            encode_fill(output, fill);
            output.push(u8::from(stroke.is_some()));
            if let Some(stroke) = stroke {
                encode_stroke(output, stroke);
            }
        }
        DisplayCommand::DrawOuterShadow {
            geometry,
            transform,
            shadow,
        } => {
            output.push(12);
            output.push(geometry_code(*geometry));
            encode_transform(output, *transform);
            encode_color(output, shadow.color);
            push_i64(output, shadow.blur_radius);
            push_i64(output, shadow.distance);
            push_i32(output, shadow.direction);
        }
    }
}

fn encode_fill(output: &mut Vec<u8>, fill: &Fill) {
    match fill {
        Fill::None => output.push(0),
        Fill::Solid(color) => {
            output.push(1);
            encode_color(output, *color);
        }
        Fill::LinearGradient { angle, stops } => {
            output.push(2);
            push_i32(output, *angle);
            encode_gradient_stops(output, stops);
        }
    }
}

fn encode_gradient_stops(output: &mut Vec<u8>, stops: &[GradientStop]) {
    push_u32(output, stops.len() as u32);
    for stop in stops {
        push_i32(output, stop.position);
        encode_color(output, stop.color);
    }
}

fn encode_stroke(output: &mut Vec<u8>, stroke: &Stroke) {
    encode_color(output, stroke.color);
    push_i64(output, stroke.width);
    push_blob(
        output,
        stroke.dash.as_deref().unwrap_or_default().as_bytes(),
    );
    output.push(line_end_code(stroke.head_end));
    output.push(line_end_code(stroke.tail_end));
}

fn line_end_code(value: Option<LineEnd>) -> u8 {
    match value {
        None => 0,
        Some(LineEnd::Triangle) => 1,
        Some(LineEnd::Stealth) => 2,
        Some(LineEnd::Diamond) => 3,
        Some(LineEnd::Oval) => 4,
        Some(LineEnd::Arrow) => 5,
    }
}

fn encode_text_style(output: &mut Vec<u8>, style: &ResolvedTextStyle) {
    push_i32(output, style.font_size);
    encode_color(output, style.color);
    push_blob(
        output,
        style.font_family.as_deref().unwrap_or_default().as_bytes(),
    );
    output.push(u8::from(style.bold));
    output.push(u8::from(style.italic));
    output.push(text_alignment_code(style.alignment));
    output.push(vertical_alignment_code(style.vertical_alignment));
    push_i64(output, style.margin_left);
    push_i64(output, style.margin_top);
    push_i64(output, style.margin_right);
    push_i64(output, style.margin_bottom);
}

fn text_alignment_code(alignment: TextAlignment) -> u8 {
    match alignment {
        TextAlignment::Left => 1,
        TextAlignment::Center => 2,
        TextAlignment::Right => 3,
        TextAlignment::Justify => 4,
    }
}

fn vertical_alignment_code(alignment: TextVerticalAlignment) -> u8 {
    match alignment {
        TextVerticalAlignment::Top => 1,
        TextVerticalAlignment::Center => 2,
        TextVerticalAlignment::Bottom => 3,
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
