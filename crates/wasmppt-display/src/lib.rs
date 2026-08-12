//! Compact backend-neutral display lists lowered from resolved slides.

use wasmppt_layout::{
    ChartGrouping, ChartKind, CustomPath, ElementKind, EmuPoint, EmuRect, EmuSize, Fill,
    GradientStop, GroupTransform, LineEnd, OuterShadow, PathCommand, PreservedFeature,
    PresetGeometry, ResolveDiagnosticCode, ResolveOutput, ResolvedChart, ResolvedSlide,
    ResolvedTable, ResolvedTextFrame, ResolvedTextStyle, RgbaColor, Stroke, TextAlignment,
    TextAutofit, TextDirection, TextFlow, TextTabAlignment, TextVerticalAlignment, Transform,
};

pub const DISPLAY_LIST_VERSION: u16 = 5;
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
    FillRadialGradientPreset {
        geometry: PresetGeometry,
        transform: Transform,
        stops: Vec<GradientStop>,
    },
    FillPatternPreset {
        geometry: PresetGeometry,
        transform: Transform,
        preset: String,
        foreground: RgbaColor,
        background: RgbaColor,
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
                            Fill::RadialGradient { stops } => {
                                list.commands
                                    .push(DisplayCommand::FillRadialGradientPreset {
                                        geometry: *geometry,
                                        transform: element.transform,
                                        stops: stops.clone(),
                                    });
                            }
                            Fill::Pattern {
                                preset,
                                foreground,
                                background,
                            } => list.commands.push(DisplayCommand::FillPatternPreset {
                                geometry: *geometry,
                                transform: element.transform,
                                preset: preset.clone(),
                                foreground: *foreground,
                                background: *background,
                            }),
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
            if cell.horizontal_merge || cell.vertical_merge {
                x = x.saturating_add(width);
                column_index += span;
                continue;
            }
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
            if cell.borders == wasmppt_layout::TableCellBorders::default() {
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
            } else {
                let cell_bounds = cell_transform.bounds;
                for (stroke, x1, y1, x2, y2) in [
                    (
                        cell.borders.left.as_ref(),
                        cell_bounds.origin.x,
                        cell_bounds.origin.y,
                        cell_bounds.origin.x,
                        cell_bounds.origin.y + cell_bounds.size.height,
                    ),
                    (
                        cell.borders.right.as_ref(),
                        cell_bounds.origin.x + cell_bounds.size.width,
                        cell_bounds.origin.y,
                        cell_bounds.origin.x + cell_bounds.size.width,
                        cell_bounds.origin.y + cell_bounds.size.height,
                    ),
                    (
                        cell.borders.top.as_ref(),
                        cell_bounds.origin.x,
                        cell_bounds.origin.y,
                        cell_bounds.origin.x + cell_bounds.size.width,
                        cell_bounds.origin.y,
                    ),
                    (
                        cell.borders.bottom.as_ref(),
                        cell_bounds.origin.x,
                        cell_bounds.origin.y + cell_bounds.size.height,
                        cell_bounds.origin.x + cell_bounds.size.width,
                        cell_bounds.origin.y + cell_bounds.size.height,
                    ),
                ] {
                    if let Some(stroke) = stroke {
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
                                ..transform
                            },
                            stroke: stroke.clone(),
                        });
                    }
                }
            }
            if let Some(frame) = cell.text_frame.as_ref().filter(|frame| {
                frame
                    .paragraphs
                    .iter()
                    .any(|paragraph| !paragraph.runs.is_empty())
            }) {
                list.commands.push(DisplayCommand::DrawRichText {
                    bounds: cell_transform.bounds,
                    frame: frame.clone(),
                });
            } else if !cell.text.is_empty() {
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
    let title_height = chart.title.as_ref().map_or(0, |_| bounds.size.height / 10);
    let legend_width = if chart.show_legend {
        bounds.size.width / 5
    } else {
        0
    };
    let plot = EmuRect {
        origin: EmuPoint {
            x: bounds.origin.x + padding_x,
            y: bounds.origin.y + padding_y + title_height,
        },
        size: EmuSize {
            width: bounds.size.width - padding_x * 2 - legend_width,
            height: bounds.size.height - padding_y * 2 - title_height,
        },
    };
    if let Some(title) = &chart.title {
        push_chart_text(
            list,
            title,
            EmuRect {
                origin: EmuPoint {
                    x: bounds.origin.x + padding_x,
                    y: bounds.origin.y,
                },
                size: EmuSize {
                    width: bounds.size.width - padding_x * 2,
                    height: padding_y + title_height,
                },
            },
            1_400,
        );
    }
    let (minimum, maximum) = chart_value_bounds(chart);
    let value_range = (maximum - minimum).max(1.0);
    let value_y = |value: f64| {
        plot.origin.y + plot.size.height
            - (((value - minimum) / value_range) * plot.size.height as f64) as i64
    };
    if !matches!(chart.kind, ChartKind::Pie | ChartKind::Doughnut) {
        push_chart_line(
            list,
            transform,
            plot.origin.x,
            value_y(0.0),
            plot.origin.x + plot.size.width,
            value_y(0.0),
            RgbaColor {
                red: 89,
                green: 89,
                blue: 89,
                alpha: 255,
            },
        );
        push_chart_line(
            list,
            transform,
            plot.origin.x,
            plot.origin.y,
            plot.origin.x,
            plot.origin.y + plot.size.height,
            RgbaColor {
                red: 89,
                green: 89,
                blue: 89,
                alpha: 255,
            },
        );
    }
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
            if chart.grouping == ChartGrouping::Standard {
                let bar_width = (slot * 4 / 5) / chart.series.len() as i64;
                for (series_index, series) in chart.series.iter().enumerate() {
                    for (value_index, value) in series.values.iter().enumerate() {
                        let baseline = value_y(0.0);
                        let end = value_y(*value);
                        let x = plot.origin.x
                            + slot * value_index as i64
                            + slot / 10
                            + bar_width * series_index as i64;
                        push_chart_rect(
                            list,
                            transform,
                            x,
                            baseline.min(end),
                            bar_width,
                            (end - baseline).abs(),
                            series.color,
                        );
                    }
                }
            } else {
                for category in 0..category_count {
                    let denominator = if chart.grouping == ChartGrouping::PercentStacked {
                        chart
                            .series
                            .iter()
                            .filter_map(|series| series.values.get(category))
                            .map(|value| value.abs())
                            .sum::<f64>()
                            .max(1.0)
                    } else {
                        1.0
                    };
                    let mut positive = 0.0;
                    let mut negative = 0.0;
                    for series in &chart.series {
                        let value =
                            series.values.get(category).copied().unwrap_or(0.0) / denominator;
                        let accumulated = if value >= 0.0 {
                            &mut positive
                        } else {
                            &mut negative
                        };
                        let start_y = value_y(*accumulated);
                        *accumulated += value;
                        let end_y = value_y(*accumulated);
                        push_chart_rect(
                            list,
                            transform,
                            plot.origin.x + slot * category as i64 + slot / 10,
                            start_y.min(end_y),
                            slot * 4 / 5,
                            (end_y - start_y).abs(),
                            series.color,
                        );
                    }
                }
            }
            push_category_labels(list, chart, plot);
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
            if chart.grouping == ChartGrouping::Standard {
                let bar_height = (slot * 4 / 5) / chart.series.len() as i64;
                for (series_index, series) in chart.series.iter().enumerate() {
                    for (value_index, value) in series.values.iter().enumerate() {
                        let baseline =
                            ((0.0 - minimum) / value_range * plot.size.width as f64) as i64;
                        let end =
                            ((*value - minimum) / value_range * plot.size.width as f64) as i64;
                        let y = plot.origin.y
                            + slot * value_index as i64
                            + slot / 10
                            + bar_height * series_index as i64;
                        push_chart_rect(
                            list,
                            transform,
                            plot.origin.x + baseline.min(end),
                            y,
                            (end - baseline).abs(),
                            bar_height,
                            series.color,
                        );
                    }
                }
            } else {
                for category in 0..category_count {
                    let denominator = if chart.grouping == ChartGrouping::PercentStacked {
                        chart
                            .series
                            .iter()
                            .filter_map(|series| series.values.get(category))
                            .map(|value| value.abs())
                            .sum::<f64>()
                            .max(1.0)
                    } else {
                        1.0
                    };
                    let mut positive = 0.0;
                    let mut negative = 0.0;
                    for series in &chart.series {
                        let value =
                            series.values.get(category).copied().unwrap_or(0.0) / denominator;
                        let accumulated = if value >= 0.0 {
                            &mut positive
                        } else {
                            &mut negative
                        };
                        let value_x = |position: f64| {
                            plot.origin.x
                                + ((position - minimum) / value_range * plot.size.width as f64)
                                    as i64
                        };
                        let start_x = value_x(*accumulated);
                        *accumulated += value;
                        let end_x = value_x(*accumulated);
                        push_chart_rect(
                            list,
                            transform,
                            start_x.min(end_x),
                            plot.origin.y + slot * category as i64 + slot / 10,
                            (end_x - start_x).abs(),
                            slot * 4 / 5,
                            series.color,
                        );
                    }
                }
            }
        }
        ChartKind::Line | ChartKind::Scatter => {
            for series in &chart.series {
                let denominator = series.values.len().saturating_sub(1).max(1) as i64;
                let x_min = series.x_values.iter().copied().fold(0.0_f64, f64::min);
                let x_max = series.x_values.iter().copied().fold(1.0_f64, f64::max);
                let x_range = (x_max - x_min).max(1.0);
                let point = |index: usize, value: f64| {
                    let x = if chart.kind == ChartKind::Scatter {
                        let raw = series.x_values.get(index).copied().unwrap_or(index as f64);
                        plot.origin.x + ((raw - x_min) / x_range * plot.size.width as f64) as i64
                    } else {
                        plot.origin.x + plot.size.width * index as i64 / denominator
                    };
                    EmuPoint {
                        x,
                        y: value_y(value),
                    }
                };
                for (index, values) in series.values.windows(2).enumerate() {
                    let first = point(index, values[0]);
                    let second = point(index + 1, values[1]);
                    push_chart_line(
                        list,
                        transform,
                        first.x,
                        first.y,
                        second.x,
                        second.y,
                        series.color,
                    );
                }
                for (index, value) in series.values.iter().enumerate() {
                    let point = point(index, *value);
                    let radius = plot.size.width.min(plot.size.height) / 100;
                    push_chart_ellipse(
                        list,
                        transform,
                        point.x - radius,
                        point.y - radius,
                        radius * 2,
                        radius * 2,
                        series.color,
                    );
                }
            }
            if chart.kind == ChartKind::Line {
                push_category_labels(list, chart, plot);
            }
        }
        ChartKind::Area => {
            for series in &chart.series {
                if series.values.is_empty() {
                    continue;
                }
                let denominator = series.values.len().saturating_sub(1).max(1) as i64;
                let mut commands = vec![PathCommand::MoveTo(EmuPoint {
                    x: 0,
                    y: value_y(0.0) - plot.origin.y,
                })];
                for (index, value) in series.values.iter().enumerate() {
                    commands.push(PathCommand::LineTo(EmuPoint {
                        x: plot.size.width * index as i64 / denominator,
                        y: value_y(*value) - plot.origin.y,
                    }));
                }
                commands.push(PathCommand::LineTo(EmuPoint {
                    x: plot.size.width,
                    y: value_y(0.0) - plot.origin.y,
                }));
                commands.push(PathCommand::Close);
                let mut color = series.color;
                color.alpha = 140;
                list.commands.push(DisplayCommand::DrawCustomPath {
                    path: CustomPath {
                        size: plot.size,
                        commands,
                    },
                    transform: Transform {
                        bounds: plot,
                        ..transform
                    },
                    fill: Fill::Solid(color),
                    stroke: Some(Stroke {
                        color: series.color,
                        width: 19_050,
                        dash: None,
                        head_end: None,
                        tail_end: None,
                    }),
                });
            }
            push_category_labels(list, chart, plot);
        }
        ChartKind::Pie | ChartKind::Doughnut => {
            if let Some(series) = chart.series.first() {
                lower_pie(
                    list,
                    transform,
                    plot,
                    series,
                    chart.kind == ChartKind::Doughnut,
                );
            }
        }
        ChartKind::Bubble => {
            for series in &chart.series {
                let x_min = series.x_values.iter().copied().fold(0.0_f64, f64::min);
                let x_max = series.x_values.iter().copied().fold(1.0_f64, f64::max);
                let x_range = (x_max - x_min).max(1.0);
                let size_max = series
                    .bubble_sizes
                    .iter()
                    .copied()
                    .fold(1.0_f64, f64::max)
                    .max(1.0);
                for (index, value) in series.values.iter().enumerate() {
                    let raw_x = series.x_values.get(index).copied().unwrap_or(index as f64);
                    let x =
                        plot.origin.x + ((raw_x - x_min) / x_range * plot.size.width as f64) as i64;
                    let y = value_y(*value);
                    let scaled =
                        (series.bubble_sizes.get(index).copied().unwrap_or(1.0) / size_max).sqrt();
                    let diameter = ((plot.size.width.min(plot.size.height) / 5) as f64 * scaled)
                        .max(19_050.0) as i64;
                    push_chart_ellipse(
                        list,
                        transform,
                        x - diameter / 2,
                        y - diameter / 2,
                        diameter,
                        diameter,
                        series.color,
                    );
                }
            }
        }
        ChartKind::Combination => {
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
            let columns = chart
                .series
                .iter()
                .filter(|series| series.kind == ChartKind::Column)
                .collect::<Vec<_>>();
            let column_width = if columns.is_empty() {
                0
            } else {
                slot * 4 / 5 / columns.len() as i64
            };
            for (series_index, series) in columns.into_iter().enumerate() {
                for (value_index, value) in series.values.iter().enumerate() {
                    let baseline = value_y(0.0);
                    let end = value_y(*value);
                    push_chart_rect(
                        list,
                        transform,
                        plot.origin.x
                            + slot * value_index as i64
                            + slot / 10
                            + column_width * series_index as i64,
                        baseline.min(end),
                        column_width,
                        (end - baseline).abs(),
                        series.color,
                    );
                }
            }
            let bars = chart
                .series
                .iter()
                .filter(|series| series.kind == ChartKind::Bar)
                .collect::<Vec<_>>();
            let row_slot = plot.size.height / category_count as i64;
            let bar_height = if bars.is_empty() {
                0
            } else {
                row_slot * 4 / 5 / bars.len() as i64
            };
            for (series_index, series) in bars.into_iter().enumerate() {
                for (value_index, value) in series.values.iter().enumerate() {
                    let baseline = ((0.0 - minimum) / value_range * plot.size.width as f64) as i64;
                    let end = ((*value - minimum) / value_range * plot.size.width as f64) as i64;
                    push_chart_rect(
                        list,
                        transform,
                        plot.origin.x + baseline.min(end),
                        plot.origin.y
                            + row_slot * value_index as i64
                            + row_slot / 10
                            + bar_height * series_index as i64,
                        (end - baseline).abs(),
                        bar_height,
                        series.color,
                    );
                }
            }
            for series in chart
                .series
                .iter()
                .filter(|series| matches!(series.kind, ChartKind::Line | ChartKind::Scatter))
            {
                let denominator = series.values.len().saturating_sub(1).max(1) as i64;
                let x_min = series.x_values.iter().copied().fold(0.0_f64, f64::min);
                let x_max = series.x_values.iter().copied().fold(1.0_f64, f64::max);
                let x_range = (x_max - x_min).max(1.0);
                let point_x = |index: usize| {
                    if series.kind == ChartKind::Scatter {
                        let raw = series.x_values.get(index).copied().unwrap_or(index as f64);
                        plot.origin.x + ((raw - x_min) / x_range * plot.size.width as f64) as i64
                    } else {
                        plot.origin.x + plot.size.width * index as i64 / denominator
                    }
                };
                for (index, values) in series.values.windows(2).enumerate() {
                    push_chart_line(
                        list,
                        transform,
                        point_x(index),
                        value_y(values[0]),
                        point_x(index + 1),
                        value_y(values[1]),
                        series.color,
                    );
                }
            }
            for series in chart
                .series
                .iter()
                .filter(|series| series.kind == ChartKind::Area && !series.values.is_empty())
            {
                let denominator = series.values.len().saturating_sub(1).max(1) as i64;
                let mut commands = vec![PathCommand::MoveTo(EmuPoint {
                    x: 0,
                    y: value_y(0.0) - plot.origin.y,
                })];
                for (index, value) in series.values.iter().enumerate() {
                    commands.push(PathCommand::LineTo(EmuPoint {
                        x: plot.size.width * index as i64 / denominator,
                        y: value_y(*value) - plot.origin.y,
                    }));
                }
                commands.push(PathCommand::LineTo(EmuPoint {
                    x: plot.size.width,
                    y: value_y(0.0) - plot.origin.y,
                }));
                commands.push(PathCommand::Close);
                let mut color = series.color;
                color.alpha = 140;
                list.commands.push(DisplayCommand::DrawCustomPath {
                    path: CustomPath {
                        size: plot.size,
                        commands,
                    },
                    transform: Transform {
                        bounds: plot,
                        ..transform
                    },
                    fill: Fill::Solid(color),
                    stroke: Some(Stroke {
                        color: series.color,
                        width: 19_050,
                        dash: None,
                        head_end: None,
                        tail_end: None,
                    }),
                });
            }
            push_category_labels(list, chart, plot);
        }
        _ => {}
    }
    if chart.show_legend {
        lower_chart_legend(
            list,
            transform,
            chart,
            plot.origin.x + plot.size.width + padding_x / 3,
            plot.origin.y,
            legend_width,
        );
    }
}

fn chart_value_bounds(chart: &ResolvedChart) -> (f64, f64) {
    if chart.grouping == ChartGrouping::Standard
        || !matches!(chart.kind, ChartKind::Column | ChartKind::Bar)
    {
        let minimum = chart
            .series
            .iter()
            .flat_map(|series| series.values.iter())
            .copied()
            .fold(0.0_f64, f64::min);
        let maximum = chart
            .series
            .iter()
            .flat_map(|series| series.values.iter())
            .copied()
            .fold(0.0_f64, f64::max)
            .max(1.0);
        return (minimum, maximum);
    }
    let category_count = chart
        .series
        .iter()
        .map(|series| series.values.len())
        .max()
        .unwrap_or(0);
    let mut minimum = 0.0_f64;
    let mut maximum = 0.0_f64;
    for category in 0..category_count {
        let denominator = if chart.grouping == ChartGrouping::PercentStacked {
            chart
                .series
                .iter()
                .filter_map(|series| series.values.get(category))
                .map(|value| value.abs())
                .sum::<f64>()
                .max(1.0)
        } else {
            1.0
        };
        let (positive, negative) = chart
            .series
            .iter()
            .filter_map(|series| series.values.get(category))
            .map(|value| value / denominator)
            .fold((0.0, 0.0), |(positive, negative), value| {
                if value >= 0.0 {
                    (positive + value, negative)
                } else {
                    (positive, negative + value)
                }
            });
        minimum = minimum.min(negative);
        maximum = maximum.max(positive);
    }
    (minimum, maximum.max(1.0))
}

fn push_category_labels(list: &mut DisplayList, chart: &ResolvedChart, plot: EmuRect) {
    let Some(categories) = chart
        .series
        .iter()
        .find(|series| !series.categories.is_empty())
        .map(|series| &series.categories)
    else {
        return;
    };
    let slot = plot.size.width / categories.len().max(1) as i64;
    for (index, category) in categories.iter().enumerate() {
        push_chart_text(
            list,
            category,
            EmuRect {
                origin: EmuPoint {
                    x: plot.origin.x + slot * index as i64,
                    y: plot.origin.y + plot.size.height,
                },
                size: EmuSize {
                    width: slot,
                    height: plot.size.height / 10,
                },
            },
            900,
        );
    }
}

fn lower_chart_legend(
    list: &mut DisplayList,
    transform: Transform,
    chart: &ResolvedChart,
    x: i64,
    y: i64,
    width: i64,
) {
    let row_height = 228_600;
    for (index, series) in chart.series.iter().enumerate() {
        let row_y = y + row_height * index as i64;
        push_chart_rect(
            list,
            transform,
            x,
            row_y + row_height / 4,
            row_height / 2,
            row_height / 2,
            series.color,
        );
        push_chart_text(
            list,
            &series.name,
            EmuRect {
                origin: EmuPoint {
                    x: x + row_height,
                    y: row_y,
                },
                size: EmuSize {
                    width: width.saturating_sub(row_height),
                    height: row_height,
                },
            },
            900,
        );
    }
}

fn lower_pie(
    list: &mut DisplayList,
    transform: Transform,
    plot: EmuRect,
    series: &wasmppt_layout::ChartSeries,
    doughnut: bool,
) {
    let total = series
        .values
        .iter()
        .copied()
        .filter(|value| *value > 0.0)
        .sum::<f64>();
    if total <= 0.0 {
        return;
    }
    let colors = [
        series.color,
        RgbaColor {
            red: 237,
            green: 125,
            blue: 49,
            alpha: 255,
        },
        RgbaColor {
            red: 165,
            green: 165,
            blue: 165,
            alpha: 255,
        },
        RgbaColor {
            red: 255,
            green: 192,
            blue: 0,
            alpha: 255,
        },
        RgbaColor {
            red: 91,
            green: 155,
            blue: 213,
            alpha: 255,
        },
    ];
    let size = plot.size.width.min(plot.size.height);
    let bounds = EmuRect {
        origin: EmuPoint {
            x: plot.origin.x + (plot.size.width - size) / 2,
            y: plot.origin.y + (plot.size.height - size) / 2,
        },
        size: EmuSize {
            width: size,
            height: size,
        },
    };
    let units = 10_000_i64;
    let center = units / 2;
    let outer = units / 2;
    let inner = if doughnut { units * 3 / 10 } else { 0 };
    let mut angle = -std::f64::consts::FRAC_PI_2;
    for (index, value) in series.values.iter().enumerate() {
        if *value <= 0.0 {
            continue;
        }
        let sweep = *value / total * std::f64::consts::TAU;
        let segments = ((sweep.abs() / (std::f64::consts::PI / 18.0)).ceil() as usize).max(1);
        let point = |radius: i64, radians: f64| EmuPoint {
            x: center + (radians.cos() * radius as f64).round() as i64,
            y: center + (radians.sin() * radius as f64).round() as i64,
        };
        let mut commands = if doughnut {
            vec![PathCommand::MoveTo(point(inner, angle))]
        } else {
            vec![PathCommand::MoveTo(EmuPoint {
                x: center,
                y: center,
            })]
        };
        commands.push(PathCommand::LineTo(point(outer, angle)));
        for segment in 1..=segments {
            commands.push(PathCommand::LineTo(point(
                outer,
                angle + sweep * segment as f64 / segments as f64,
            )));
        }
        if doughnut {
            for segment in (0..=segments).rev() {
                commands.push(PathCommand::LineTo(point(
                    inner,
                    angle + sweep * segment as f64 / segments as f64,
                )));
            }
        }
        commands.push(PathCommand::Close);
        list.commands.push(DisplayCommand::DrawCustomPath {
            path: CustomPath {
                size: EmuSize {
                    width: units,
                    height: units,
                },
                commands,
            },
            transform: Transform {
                bounds,
                ..transform
            },
            fill: Fill::Solid(colors[index % colors.len()]),
            stroke: Some(Stroke {
                color: RgbaColor {
                    red: 255,
                    green: 255,
                    blue: 255,
                    alpha: 255,
                },
                width: 9_525,
                dash: None,
                head_end: None,
                tail_end: None,
            }),
        });
        angle += sweep;
    }
}

fn push_chart_text(list: &mut DisplayList, value: &str, bounds: EmuRect, font_size: i32) {
    let text = list.strings.len() as u32;
    list.strings.push(value.to_owned());
    list.commands.push(DisplayCommand::DrawText {
        text,
        bounds,
        style: ResolvedTextStyle {
            font_size,
            alignment: TextAlignment::Center,
            vertical_alignment: TextVerticalAlignment::Center,
            ..ResolvedTextStyle::default()
        },
    });
}

fn push_chart_line(
    list: &mut DisplayList,
    parent: Transform,
    x1: i64,
    y1: i64,
    x2: i64,
    y2: i64,
    color: RgbaColor,
) {
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
            ..parent
        },
        stroke: Stroke {
            color,
            width: 12_700,
            dash: None,
            head_end: None,
            tail_end: None,
        },
    });
}

fn push_chart_ellipse(
    list: &mut DisplayList,
    parent: Transform,
    x: i64,
    y: i64,
    width: i64,
    height: i64,
    color: RgbaColor,
) {
    list.commands.push(DisplayCommand::FillPreset {
        geometry: PresetGeometry::Ellipse,
        transform: Transform {
            bounds: EmuRect {
                origin: EmuPoint { x, y },
                size: EmuSize { width, height },
            },
            ..parent
        },
        color,
    });
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
            output.push(match frame.flow {
                TextFlow::Horizontal => 0,
                TextFlow::Vertical => 1,
                TextFlow::Vertical270 => 2,
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
                output.push(match paragraph.direction {
                    TextDirection::LeftToRight => 0,
                    TextDirection::RightToLeft => 1,
                });
                push_u32(output, paragraph.tabs.len() as u32);
                for tab in &paragraph.tabs {
                    push_i64(output, tab.position);
                    output.push(match tab.alignment {
                        TextTabAlignment::Left => 0,
                        TextTabAlignment::Center => 1,
                        TextTabAlignment::Right => 2,
                        TextTabAlignment::Decimal => 3,
                    });
                }
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
                    PathCommand::QuadraticTo { control, end } => {
                        output.push(4);
                        push_i64(output, control.x);
                        push_i64(output, control.y);
                        push_i64(output, end.x);
                        push_i64(output, end.y);
                    }
                    PathCommand::CubicTo {
                        control1,
                        control2,
                        end,
                    } => {
                        output.push(5);
                        for point in [control1, control2, end] {
                            push_i64(output, point.x);
                            push_i64(output, point.y);
                        }
                    }
                    PathCommand::ArcTo {
                        width_radius,
                        height_radius,
                        start_angle,
                        sweep_angle,
                    } => {
                        output.push(6);
                        push_i64(output, *width_radius);
                        push_i64(output, *height_radius);
                        push_i32(output, *start_angle);
                        push_i32(output, *sweep_angle);
                    }
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
        DisplayCommand::FillRadialGradientPreset {
            geometry,
            transform,
            stops,
        } => {
            output.push(13);
            output.push(geometry_code(*geometry));
            encode_transform(output, *transform);
            encode_gradient_stops(output, stops);
        }
        DisplayCommand::FillPatternPreset {
            geometry,
            transform,
            preset,
            foreground,
            background,
        } => {
            output.push(14);
            output.push(geometry_code(*geometry));
            encode_transform(output, *transform);
            push_blob(output, preset.as_bytes());
            encode_color(output, *foreground);
            encode_color(output, *background);
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
        Fill::RadialGradient { stops } => {
            output.push(3);
            encode_gradient_stops(output, stops);
        }
        Fill::Pattern {
            preset,
            foreground,
            background,
        } => {
            output.push(4);
            push_blob(output, preset.as_bytes());
            encode_color(output, *foreground);
            encode_color(output, *background);
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
    output.push(u8::from(style.underline));
    output.push(u8::from(style.strike));
    push_i32(output, style.character_spacing);
    push_i32(output, style.baseline);
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
        PresetGeometry::Pentagon => 10,
        PresetGeometry::Octagon => 11,
        PresetGeometry::Star5 => 12,
        PresetGeometry::Plus => 13,
        PresetGeometry::Chevron => 14,
        PresetGeometry::RightArrow => 15,
        PresetGeometry::LeftArrow => 16,
        PresetGeometry::UpArrow => 17,
        PresetGeometry::DownArrow => 18,
        PresetGeometry::Trapezoid => 19,
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

#[cfg(test)]
mod tests {
    use super::*;
    use wasmppt_layout::ChartSeries;

    #[test]
    fn combination_chart_uses_each_series_kind_independent_of_source_order() {
        let line_color = RgbaColor {
            red: 1,
            green: 2,
            blue: 3,
            alpha: 255,
        };
        let column_color = RgbaColor {
            red: 4,
            green: 5,
            blue: 6,
            alpha: 255,
        };
        let series = |kind, color| ChartSeries {
            kind,
            name: String::new(),
            categories: vec!["Q1".to_owned(), "Q2".to_owned()],
            x_values: Vec::new(),
            values: vec![1.0, 2.0],
            bubble_sizes: Vec::new(),
            color,
        };
        let chart = ResolvedChart {
            kind: ChartKind::Combination,
            grouping: ChartGrouping::Standard,
            series: vec![
                series(ChartKind::Line, line_color),
                series(ChartKind::Column, column_color),
            ],
            title: None,
            show_legend: false,
            embedded_workbook: None,
        };
        let size = EmuSize {
            width: 9_144_000,
            height: 6_858_000,
        };
        let mut list = DisplayList {
            size,
            commands: Vec::new(),
            group_transforms: Vec::new(),
            strings: Vec::new(),
            images: Vec::new(),
            semantics: Vec::new(),
            diagnostics: Vec::new(),
        };
        lower_chart(
            &mut list,
            Transform {
                bounds: EmuRect {
                    origin: EmuPoint { x: 0, y: 0 },
                    size,
                },
                rotation: 0,
                flip_horizontal: false,
                flip_vertical: false,
            },
            &chart,
        );
        assert!(list.commands.iter().any(|command| matches!(
            command,
            DisplayCommand::FillPreset { color, .. } if *color == column_color
        )));
        assert!(list.commands.iter().any(|command| matches!(
            command,
            DisplayCommand::StrokePreset { stroke, .. } if stroke.color == line_color
        )));
    }

    #[test]
    fn stacked_chart_bounds_use_signed_category_totals() {
        let make_series = |values| ChartSeries {
            kind: ChartKind::Column,
            name: String::new(),
            categories: Vec::new(),
            x_values: Vec::new(),
            values,
            bubble_sizes: Vec::new(),
            color: RgbaColor {
                red: 1,
                green: 2,
                blue: 3,
                alpha: 255,
            },
        };
        let chart = ResolvedChart {
            kind: ChartKind::Column,
            grouping: ChartGrouping::Stacked,
            series: vec![make_series(vec![8.0, -4.0]), make_series(vec![8.0, -4.0])],
            title: None,
            show_legend: false,
            embedded_workbook: None,
        };
        assert_eq!(chart_value_bounds(&chart), (-8.0, 16.0));
        let percent = ResolvedChart {
            grouping: ChartGrouping::PercentStacked,
            ..chart
        };
        assert_eq!(chart_value_bounds(&percent), (-1.0, 1.0));
    }
}
