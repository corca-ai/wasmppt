use std::collections::{BTreeMap, HashSet};

use wasmppt_opc::{MemorySource, PackageGraph, PartId, RelationshipTarget, ZipArchive};
use wasmppt_xml::{Attribute, TokenKind, XmlDocument, decode_entities};

use crate::{
    ChartKind, ChartSeries, ElementKind, EmuPoint, EmuSize, Fill, GroupTransform, ImageCrop,
    LayoutError, Placeholder, PreservedFeature, PresetGeometry, ResolutionTrace, ResolveDiagnostic,
    ResolveDiagnosticCode, ResolveOutput, ResolvedChart, ResolvedElement, ResolvedSlide,
    ResolvedTable, ResolvedTableCell, ResolvedTableRow, RgbaColor, SourceLevel, Stroke, Transform,
    plain_i64,
};

const WHITE: RgbaColor = RgbaColor {
    red: 255,
    green: 255,
    blue: 255,
    alpha: 255,
};
const BLACK: RgbaColor = RgbaColor {
    red: 0,
    green: 0,
    blue: 0,
    alpha: 255,
};

#[derive(Clone, Debug, Default)]
struct RawShape {
    id: u32,
    name: String,
    placeholder: Option<Placeholder>,
    transform: Option<Transform>,
    groups: Vec<GroupTransform>,
    fill: Option<Fill>,
    stroke: Option<Stroke>,
    text: Option<String>,
    alternative_text: Option<String>,
    hyperlink_relationship_id: Option<String>,
    table: Option<ResolvedTable>,
    chart_relationship_id: Option<String>,
    preserved_graphic: Option<PreservedFeature>,
    geometry: Option<PresetGeometry>,
    image_relationship_id: Option<String>,
    crop: ImageCrop,
}

#[derive(Debug, Default)]
struct ParsedPart {
    shapes: Vec<RawShape>,
    background: Option<RgbaColor>,
    diagnostics: Vec<(ResolveDiagnosticCode, Option<u32>, String)>,
}

#[derive(Clone, Debug)]
struct Theme {
    colors: BTreeMap<String, RgbaColor>,
    mapping: BTreeMap<String, String>,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            colors: BTreeMap::from([
                ("dk1".to_owned(), BLACK),
                ("lt1".to_owned(), WHITE),
                (
                    "accent1".to_owned(),
                    RgbaColor {
                        red: 68,
                        green: 114,
                        blue: 196,
                        alpha: 255,
                    },
                ),
            ]),
            mapping: BTreeMap::from([
                ("bg1".to_owned(), "lt1".to_owned()),
                ("tx1".to_owned(), "dk1".to_owned()),
                ("bg2".to_owned(), "lt2".to_owned()),
                ("tx2".to_owned(), "dk2".to_owned()),
            ]),
        }
    }
}

pub fn resolve_slide_parts(
    archive: &ZipArchive<MemorySource>,
    graph: &PackageGraph,
    slide_id: PartId,
    slide_size: EmuSize,
) -> Result<ResolveOutput, LayoutError> {
    let layout_id = related(graph, slide_id, "/slideLayout");
    let master_id = layout_id.and_then(|part| related(graph, part, "/slideMaster"));
    let theme_id = master_id.and_then(|part| related(graph, part, "/theme"));
    let mut trace = ResolutionTrace::default();
    let mut diagnostics = Vec::new();

    let theme = if let Some(part) = theme_id {
        let (name, document) = parse_part(archive, graph, part, &mut trace)?;
        parse_theme(&document).map_err(|message| {
            LayoutError::new(format!("cannot parse theme part {name}: {message}"))
        })?
    } else {
        Theme::default()
    };
    let master = if let Some(part) = master_id {
        let (name, document) = parse_part(archive, graph, part, &mut trace)?;
        let mut adjusted = theme.clone();
        apply_color_map(&document, &mut adjusted);
        let parsed = parse_drawing_part(&document, &adjusted);
        append_diagnostics(&mut diagnostics, &name, &parsed);
        Some((part, parsed, adjusted))
    } else {
        None
    };
    let layout = if let Some(part) = layout_id {
        let (name, document) = parse_part(archive, graph, part, &mut trace)?;
        let mut adjusted = master
            .as_ref()
            .map_or_else(|| theme.clone(), |(_, _, theme)| theme.clone());
        apply_color_map(&document, &mut adjusted);
        let parsed = parse_drawing_part(&document, &adjusted);
        append_diagnostics(&mut diagnostics, &name, &parsed);
        Some((part, parsed, adjusted))
    } else {
        None
    };
    let (slide_name, slide_document) = parse_part(archive, graph, slide_id, &mut trace)?;
    let mut slide_theme = layout
        .as_ref()
        .map_or_else(|| theme.clone(), |(_, _, theme)| theme.clone());
    apply_color_map(&slide_document, &mut slide_theme);
    let slide = parse_drawing_part(&slide_document, &slide_theme);
    append_diagnostics(&mut diagnostics, &slide_name, &slide);

    let background = slide
        .background
        .or_else(|| layout.as_ref().and_then(|(_, part, _)| part.background))
        .or_else(|| master.as_ref().and_then(|(_, part, _)| part.background))
        .unwrap_or(WHITE);
    let mut elements = Vec::new();
    {
        let mut element_resolver = ElementResolver {
            archive,
            graph,
            diagnostics: &mut diagnostics,
            trace: &mut trace,
        };
        if let Some((part, parsed, _)) = &master {
            append_non_placeholder(
                &mut elements,
                &mut element_resolver,
                parsed,
                SourceLevel::Master,
                *part,
            );
        }
        if let Some((part, parsed, _)) = &layout {
            append_non_placeholder(
                &mut elements,
                &mut element_resolver,
                parsed,
                SourceLevel::Layout,
                *part,
            );
        }
        for raw in &slide.shapes {
            let inherited_layout = raw.placeholder.as_ref().and_then(|placeholder| {
                layout.as_ref().and_then(|(_, part, _)| {
                    part.shapes.iter().find(|candidate| {
                        placeholder_matches(placeholder, candidate.placeholder.as_ref())
                    })
                })
            });
            let inherited_master = raw.placeholder.as_ref().and_then(|placeholder| {
                master.as_ref().and_then(|(_, part, _)| {
                    part.shapes.iter().find(|candidate| {
                        placeholder_matches(placeholder, candidate.placeholder.as_ref())
                    })
                })
            });
            let merged = merge_shape(raw, inherited_layout, inherited_master);
            elements.push(resolve_element(
                &merged,
                &mut element_resolver,
                SourceLevel::Slide,
                slide_id,
                &slide_name,
            ));
        }
    }
    for (z_order, element) in elements.iter_mut().enumerate() {
        element.z_order = z_order as u32;
    }
    deduplicate_trace(&mut trace);
    Ok(ResolveOutput {
        slide: ResolvedSlide {
            part_name: slide_name,
            size: slide_size,
            background,
            elements,
        },
        diagnostics,
        trace,
    })
}

fn parse_part(
    archive: &ZipArchive<MemorySource>,
    graph: &PackageGraph,
    part: PartId,
    trace: &mut ResolutionTrace,
) -> Result<(String, XmlDocument), LayoutError> {
    let name = graph.part_name(graph.part(part)).to_owned();
    trace.visited_parts.push(name.clone());
    let entry = archive
        .entry(&name)
        .ok_or_else(|| LayoutError::new(format!("reachable part {name} has no ZIP entry")))?;
    let bytes = archive
        .read_entry(entry)
        .map_err(|error| LayoutError::new(format!("cannot read part {name}: {error}")))?;
    let document = XmlDocument::parse(bytes)
        .map_err(|error| LayoutError::new(format!("cannot parse part {name}: {error}")))?;
    trace.parsed_xml_parts.push(name.clone());
    Ok((name, document))
}

fn related(graph: &PackageGraph, source: PartId, relationship_suffix: &str) -> Option<PartId> {
    graph
        .part(source)
        .relationships
        .iter()
        .find(|relationship| {
            graph
                .relationship_type(relationship)
                .ends_with(relationship_suffix)
        })
        .and_then(|relationship| match relationship.target {
            RelationshipTarget::Internal(part) => Some(part),
            _ => None,
        })
}

fn relationship_target(graph: &PackageGraph, source: PartId, id: &str) -> Option<PartId> {
    graph
        .part(source)
        .relationships
        .iter()
        .find(|relationship| graph.relationship_id(relationship) == id)
        .and_then(|relationship| match relationship.target {
            RelationshipTarget::Internal(part) => Some(part),
            _ => None,
        })
}

fn append_non_placeholder(
    output: &mut Vec<ResolvedElement>,
    resolver: &mut ElementResolver<'_>,
    part: &ParsedPart,
    source: SourceLevel,
    part_id: PartId,
) {
    let part_name = resolver
        .graph
        .part_name(resolver.graph.part(part_id))
        .to_owned();
    for shape in part
        .shapes
        .iter()
        .filter(|shape| shape.placeholder.is_none())
    {
        output.push(resolve_element(
            shape, resolver, source, part_id, &part_name,
        ));
    }
}

struct ElementResolver<'a> {
    archive: &'a ZipArchive<MemorySource>,
    graph: &'a PackageGraph,
    diagnostics: &'a mut Vec<ResolveDiagnostic>,
    trace: &'a mut ResolutionTrace,
}

fn resolve_element(
    shape: &RawShape,
    resolver: &mut ElementResolver<'_>,
    source: SourceLevel,
    source_part: PartId,
    source_name: &str,
) -> ResolvedElement {
    let graph = resolver.graph;
    let kind = if let Some(table) = &shape.table {
        ElementKind::Table {
            table: table.clone(),
        }
    } else if let Some(relationship_id) = &shape.chart_relationship_id {
        let target = relationship_target(graph, source_part, relationship_id);
        let chart = target
            .and_then(|part| {
                let (part_name, document) =
                    parse_part(resolver.archive, graph, part, resolver.trace).ok()?;
                let mut chart = parse_chart(&document);
                chart.embedded_workbook = related(graph, part, "/package").map(|workbook| {
                    let name = graph.part_name(graph.part(workbook)).to_owned();
                    resolver.trace.visited_parts.push(name.clone());
                    name
                });
                if matches!(
                    chart.kind,
                    ChartKind::Pie | ChartKind::Area | ChartKind::Scatter | ChartKind::Other
                ) {
                    resolver.diagnostics.push(ResolveDiagnostic {
                        code: ResolveDiagnosticCode::UnsupportedChartKind,
                        part_name,
                        shape_id: Some(shape.id),
                        message: format!(
                            "{:?} chart data is read and preserved but not rendered yet",
                            chart.kind
                        ),
                    });
                }
                Some(chart)
            })
            .unwrap_or_else(|| {
                resolver.diagnostics.push(ResolveDiagnostic {
                    code: ResolveDiagnosticCode::MissingDependency,
                    part_name: source_name.to_owned(),
                    shape_id: Some(shape.id),
                    message: format!("chart relationship {relationship_id} has no readable target"),
                });
                ResolvedChart {
                    kind: ChartKind::Other,
                    series: Vec::new(),
                    embedded_workbook: None,
                }
            });
        ElementKind::Chart { chart }
    } else if let Some(feature) = shape.preserved_graphic {
        ElementKind::PreservedGraphic { feature }
    } else if let Some(relationship_id) = &shape.image_relationship_id {
        let target = graph
            .part(source_part)
            .relationships
            .iter()
            .find(|relationship| graph.relationship_id(relationship) == relationship_id)
            .and_then(|relationship| match relationship.target {
                RelationshipTarget::Internal(part) => Some(part),
                _ => None,
            });
        let part_name = target.map(|part| graph.part_name(graph.part(part)).to_owned());
        if let Some(name) = &part_name {
            resolver.trace.visited_parts.push(name.clone());
            if name.ends_with(".emf") || name.ends_with(".wmf") {
                resolver.diagnostics.push(ResolveDiagnostic {
                    code: ResolveDiagnosticCode::UnsupportedMetafile,
                    part_name: source_name.to_owned(),
                    shape_id: Some(shape.id),
                    message: format!("metafile {name} is preserved and requires a preview backend"),
                });
                return resolved_element(
                    shape,
                    source,
                    ElementKind::PreservedGraphic {
                        feature: PreservedFeature::Metafile,
                    },
                    graph,
                    source_part,
                );
            }
        } else {
            resolver.diagnostics.push(ResolveDiagnostic {
                code: ResolveDiagnosticCode::MissingImage,
                part_name: source_name.to_owned(),
                shape_id: Some(shape.id),
                message: format!("image relationship {relationship_id} has no internal target"),
            });
        }
        ElementKind::Image {
            relationship_id: relationship_id.clone(),
            part_name,
            crop: shape.crop,
        }
    } else {
        ElementKind::Shape {
            geometry: shape.geometry.unwrap_or(PresetGeometry::Rect),
        }
    };
    resolved_element(shape, source, kind, graph, source_part)
}

fn resolved_element(
    shape: &RawShape,
    source: SourceLevel,
    kind: ElementKind,
    graph: &PackageGraph,
    source_part: PartId,
) -> ResolvedElement {
    ResolvedElement {
        id: shape.id,
        name: shape.name.clone(),
        source,
        z_order: 0,
        placeholder: shape.placeholder.clone(),
        transform: shape.transform.unwrap_or_default(),
        group_transforms: shape.groups.clone(),
        fill: shape.fill.clone().unwrap_or(Fill::None),
        stroke: shape.stroke.clone(),
        text: shape.text.clone().unwrap_or_default(),
        alternative_text: shape.alternative_text.clone(),
        hyperlink: shape.hyperlink_relationship_id.as_ref().and_then(|id| {
            graph
                .part(source_part)
                .relationships
                .iter()
                .find(|relationship| graph.relationship_id(relationship) == id)
                .and_then(|relationship| match &relationship.target {
                    RelationshipTarget::External(target) => Some(target.clone()),
                    RelationshipTarget::Internal(part) => {
                        Some(graph.part_name(graph.part(*part)).to_owned())
                    }
                    RelationshipTarget::Missing(_) => None,
                })
        }),
        kind,
    }
}

fn merge_shape(local: &RawShape, layout: Option<&RawShape>, master: Option<&RawShape>) -> RawShape {
    let fallback = |select: fn(&RawShape) -> Option<Transform>| {
        select(local)
            .or_else(|| layout.and_then(select))
            .or_else(|| master.and_then(select))
    };
    RawShape {
        id: local.id,
        name: local.name.clone(),
        placeholder: local.placeholder.clone(),
        transform: fallback(|shape| shape.transform),
        groups: if local.groups.is_empty() {
            layout
                .filter(|shape| !shape.groups.is_empty())
                .or_else(|| master.filter(|shape| !shape.groups.is_empty()))
                .map_or_else(Vec::new, |shape| shape.groups.clone())
        } else {
            local.groups.clone()
        },
        fill: local
            .fill
            .clone()
            .or_else(|| layout.and_then(|shape| shape.fill.clone()))
            .or_else(|| master.and_then(|shape| shape.fill.clone())),
        stroke: local
            .stroke
            .clone()
            .or_else(|| layout.and_then(|shape| shape.stroke.clone()))
            .or_else(|| master.and_then(|shape| shape.stroke.clone())),
        text: local
            .text
            .clone()
            .or_else(|| layout.and_then(|shape| shape.text.clone()))
            .or_else(|| master.and_then(|shape| shape.text.clone())),
        alternative_text: local
            .alternative_text
            .clone()
            .or_else(|| layout.and_then(|shape| shape.alternative_text.clone()))
            .or_else(|| master.and_then(|shape| shape.alternative_text.clone())),
        hyperlink_relationship_id: local.hyperlink_relationship_id.clone(),
        table: local.table.clone(),
        chart_relationship_id: local.chart_relationship_id.clone(),
        preserved_graphic: local.preserved_graphic,
        geometry: local
            .geometry
            .or_else(|| layout.and_then(|shape| shape.geometry))
            .or_else(|| master.and_then(|shape| shape.geometry)),
        image_relationship_id: local.image_relationship_id.clone(),
        crop: local.crop,
    }
}

fn placeholder_matches(expected: &Placeholder, candidate: Option<&Placeholder>) -> bool {
    candidate.is_some_and(|candidate| {
        candidate.index == expected.index
            || (candidate.kind == expected.kind && (candidate.index == 0 || expected.index == 0))
    })
}

fn parse_drawing_part(document: &XmlDocument, theme: &Theme) -> ParsedPart {
    let mut parsed = ParsedPart {
        background: parse_background(document, theme),
        ..ParsedPart::default()
    };
    let mut groups = Vec::<(usize, GroupTransform)>::new();
    for (index, token) in document.tokens().iter().enumerate() {
        match &token.kind {
            TokenKind::Start {
                name,
                attributes: _,
                empty: _,
            } if name.local == "grpSp" => {
                if let Some(end) = element_end(document, index) {
                    groups.push((token.depth, parse_group_transform(document, index, end)));
                }
            }
            TokenKind::Start { name, .. }
                if matches!(name.local.as_str(), "sp" | "pic" | "cxnSp") =>
            {
                if let Some(end) = element_end(document, index) {
                    parsed.shapes.push(parse_shape(
                        document,
                        index,
                        end,
                        groups.iter().map(|(_, transform)| *transform).collect(),
                        theme,
                        &mut parsed.diagnostics,
                    ));
                }
            }
            TokenKind::Start { name, .. } if name.local == "graphicFrame" => {
                if let Some(end) = element_end(document, index) {
                    parsed.shapes.push(parse_graphic_frame(
                        document,
                        index,
                        end,
                        groups.iter().map(|(_, transform)| *transform).collect(),
                        theme,
                        &mut parsed.diagnostics,
                    ));
                }
            }
            TokenKind::End { name }
                if name.local == "grpSp"
                    && groups
                        .last()
                        .is_some_and(|(depth, _)| *depth == token.depth) =>
            {
                groups.pop();
            }
            _ => {}
        }
    }
    for token in document.tokens() {
        let TokenKind::Start { name, .. } = &token.kind else {
            continue;
        };
        let diagnostic = match name.local.as_str() {
            "timing" | "anim" | "animMotion" | "animEffect" => Some((
                ResolveDiagnosticCode::UnsupportedAnimation,
                "animation timing is preserved but not executed",
            )),
            "transition" => Some((
                ResolveDiagnosticCode::UnsupportedTransition,
                "slide transition is preserved but not executed",
            )),
            "scene3d" | "sp3d" => Some((
                ResolveDiagnosticCode::UnsupportedThreeD,
                "3D properties are preserved but not rendered",
            )),
            _ => None,
        };
        if let Some((code, message)) = diagnostic {
            if !parsed.diagnostics.iter().any(|existing| existing.0 == code) {
                parsed.diagnostics.push((code, None, message.to_owned()));
            }
        }
    }
    parsed
}

fn parse_graphic_frame(
    document: &XmlDocument,
    start: usize,
    end: usize,
    groups: Vec<GroupTransform>,
    theme: &Theme,
    diagnostics: &mut Vec<(ResolveDiagnosticCode, Option<u32>, String)>,
) -> RawShape {
    let mut shape = RawShape {
        groups,
        ..RawShape::default()
    };
    for index in start..=end {
        let TokenKind::Start {
            name, attributes, ..
        } = &document.tokens()[index].kind
        else {
            continue;
        };
        match name.local.as_str() {
            "cNvPr" => {
                shape.id = plain_u32(attributes, "id").unwrap_or(0);
                shape.name = plain(attributes, "name").unwrap_or_default().to_owned();
                shape.alternative_text = plain(attributes, "descr")
                    .or_else(|| plain(attributes, "title"))
                    .map(str::to_owned);
            }
            "xfrm" if shape.transform.is_none() => {
                if let Some(transform_end) = element_end(document, index) {
                    shape.transform = Some(parse_transform(document, index, transform_end));
                }
            }
            "tbl" if shape.table.is_none() => {
                if let Some(table_end) = element_end(document, index) {
                    shape.table = Some(parse_table(document, index, table_end, theme));
                }
            }
            "chart" => {
                shape.chart_relationship_id = attributes
                    .iter()
                    .find(|attribute| attribute.name.local == "id")
                    .map(|attribute| attribute.value.clone());
            }
            "relIds" => {
                shape.preserved_graphic = Some(PreservedFeature::SmartArt);
                diagnostics.push((
                    ResolveDiagnosticCode::UnsupportedSmartArt,
                    Some(shape.id),
                    "SmartArt data and fallback relationships are preserved but not rendered"
                        .to_owned(),
                ));
            }
            "oleObj" => {
                shape.preserved_graphic = Some(PreservedFeature::OleObject);
                diagnostics.push((
                    ResolveDiagnosticCode::UnsupportedActiveContent,
                    Some(shape.id),
                    "embedded OLE content is preserved but never activated".to_owned(),
                ));
            }
            _ => {}
        }
    }
    if shape.table.is_none()
        && shape.chart_relationship_id.is_none()
        && shape.preserved_graphic.is_none()
    {
        shape.preserved_graphic = Some(PreservedFeature::UnknownGraphicFrame);
        diagnostics.push((
            ResolveDiagnosticCode::UnsupportedGraphicFrame,
            Some(shape.id),
            "unknown graphic frame is preserved but not rendered".to_owned(),
        ));
    }
    shape
}

fn parse_table(document: &XmlDocument, start: usize, end: usize, theme: &Theme) -> ResolvedTable {
    let mut column_widths = Vec::new();
    let mut rows = Vec::new();
    for index in start..=end {
        let TokenKind::Start {
            name, attributes, ..
        } = &document.tokens()[index].kind
        else {
            continue;
        };
        if name.local == "gridCol" {
            column_widths.push(plain_i64(attributes, "w").unwrap_or(0));
        } else if name.local == "tr" {
            let Some(row_end) = element_end(document, index) else {
                continue;
            };
            let mut cells = Vec::new();
            for cell_index in index + 1..row_end {
                let TokenKind::Start {
                    name: cell_name,
                    attributes: cell_attributes,
                    ..
                } = &document.tokens()[cell_index].kind
                else {
                    continue;
                };
                if cell_name.local != "tc"
                    || document.tokens()[cell_index].depth != document.tokens()[index].depth + 1
                {
                    continue;
                }
                let cell_end = element_end(document, cell_index).unwrap_or(cell_index);
                let text = collect_text(document, cell_index, cell_end);
                let fill = (cell_index..=cell_end)
                    .find_map(|fill_index| {
                        matches!(
                            &document.tokens()[fill_index].kind,
                            TokenKind::Start { name, .. } if name.local == "solidFill"
                        )
                        .then(|| {
                            parse_color(
                                document,
                                fill_index,
                                element_end(document, fill_index).unwrap_or(fill_index),
                                theme,
                            )
                        })
                        .flatten()
                    })
                    .unwrap_or(WHITE);
                cells.push(ResolvedTableCell {
                    text,
                    row_span: plain_u32(cell_attributes, "rowSpan").unwrap_or(1),
                    column_span: plain_u32(cell_attributes, "gridSpan").unwrap_or(1),
                    fill,
                });
            }
            rows.push(ResolvedTableRow {
                height: plain_i64(attributes, "h").unwrap_or(0),
                cells,
            });
        }
    }
    ResolvedTable {
        column_widths,
        rows,
    }
}

fn parse_chart(document: &XmlDocument) -> ResolvedChart {
    let mut kind = ChartKind::Other;
    for token in document.tokens() {
        let TokenKind::Start { name, .. } = &token.kind else {
            continue;
        };
        kind = match name.local.as_str() {
            "lineChart" => ChartKind::Line,
            "pieChart" | "pie3DChart" | "doughnutChart" => ChartKind::Pie,
            "areaChart" | "area3DChart" => ChartKind::Area,
            "scatterChart" => ChartKind::Scatter,
            "barChart" | "bar3DChart" => {
                let bar_direction = document.tokens().iter().find_map(|candidate| {
                    let TokenKind::Start {
                        name, attributes, ..
                    } = &candidate.kind
                    else {
                        return None;
                    };
                    (name.local == "barDir")
                        .then(|| plain(attributes, "val"))
                        .flatten()
                });
                if bar_direction == Some("bar") {
                    ChartKind::Bar
                } else {
                    ChartKind::Column
                }
            }
            _ => continue,
        };
        break;
    }
    let palette = [
        RgbaColor {
            red: 68,
            green: 114,
            blue: 196,
            alpha: 255,
        },
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
    let mut series = Vec::new();
    for (index, token) in document.tokens().iter().enumerate() {
        let TokenKind::Start { name, .. } = &token.kind else {
            continue;
        };
        if name.local != "ser" {
            continue;
        }
        let Some(end) = element_end(document, index) else {
            continue;
        };
        let name = child_cache_values(document, index, end, &["tx"])
            .into_iter()
            .next()
            .unwrap_or_else(|| format!("Series {}", series.len() + 1));
        let categories = child_cache_values(document, index, end, &["cat", "xVal"]);
        let values = child_cache_values(document, index, end, &["val", "yVal"])
            .into_iter()
            .filter_map(|value| value.parse::<f64>().ok())
            .collect();
        series.push(ChartSeries {
            name,
            categories,
            values,
            color: palette[series.len() % palette.len()],
        });
    }
    ResolvedChart {
        kind,
        series,
        embedded_workbook: None,
    }
}

fn child_cache_values(
    document: &XmlDocument,
    start: usize,
    end: usize,
    container_names: &[&str],
) -> Vec<String> {
    for index in start..=end {
        let TokenKind::Start { name, .. } = &document.tokens()[index].kind else {
            continue;
        };
        if !container_names.contains(&name.local.as_str()) {
            continue;
        }
        let container_end = element_end(document, index).unwrap_or(index);
        let values = cache_values(document, index, container_end);
        if !values.is_empty() {
            return values;
        }
    }
    Vec::new()
}

fn cache_values(document: &XmlDocument, start: usize, end: usize) -> Vec<String> {
    let mut indexed = Vec::<(u32, String)>::new();
    for index in start..=end {
        let TokenKind::Start {
            name, attributes, ..
        } = &document.tokens()[index].kind
        else {
            continue;
        };
        if name.local != "pt" {
            continue;
        }
        let point_end = element_end(document, index).unwrap_or(index);
        let value = (index..=point_end).find_map(|value_index| {
            let TokenKind::Start { name, .. } = &document.tokens()[value_index].kind else {
                return None;
            };
            (name.local == "v").then(|| {
                element_end(document, value_index)
                    .map(|value_end| collect_raw_text(document, value_index, value_end))
                    .unwrap_or_default()
            })
        });
        if let Some(value) = value {
            indexed.push((plain_u32(attributes, "idx").unwrap_or(index as u32), value));
        }
    }
    if indexed.is_empty() {
        let direct = collect_raw_text(document, start, end);
        return (!direct.is_empty()).then_some(direct).into_iter().collect();
    }
    indexed.sort_unstable_by_key(|(index, _)| *index);
    indexed.into_iter().map(|(_, value)| value).collect()
}

fn collect_text(document: &XmlDocument, start: usize, end: usize) -> String {
    let mut output = String::new();
    for index in start..=end {
        let TokenKind::Start { name, .. } = &document.tokens()[index].kind else {
            continue;
        };
        if name.local == "t" {
            let text_end = element_end(document, index).unwrap_or(index);
            output.push_str(&collect_raw_text(document, index, text_end));
        }
    }
    output
}

fn collect_raw_text(document: &XmlDocument, start: usize, end: usize) -> String {
    let mut output = String::new();
    for token in &document.tokens()[start..=end] {
        if !matches!(token.kind, TokenKind::Text | TokenKind::Cdata) {
            continue;
        }
        let range = if matches!(token.kind, TokenKind::Cdata) {
            token.range.start + 9..token.range.end - 3
        } else {
            token.range.clone()
        };
        let raw = std::str::from_utf8(document.source_range(range)).unwrap_or_default();
        if matches!(token.kind, TokenKind::Cdata) {
            output.push_str(raw);
        } else if let Ok(decoded) = decode_entities(raw, token.range.start) {
            output.push_str(&decoded);
        }
    }
    output
}

fn parse_shape(
    document: &XmlDocument,
    start: usize,
    end: usize,
    groups: Vec<GroupTransform>,
    theme: &Theme,
    diagnostics: &mut Vec<(ResolveDiagnosticCode, Option<u32>, String)>,
) -> RawShape {
    let mut shape = RawShape {
        groups,
        ..RawShape::default()
    };
    let mut stack = Vec::<String>::new();
    let mut in_text = false;
    let mut text = String::new();
    for index in start..=end {
        let token = &document.tokens()[index];
        match &token.kind {
            TokenKind::Start {
                name,
                attributes,
                empty,
            } => {
                let inside_line = stack.iter().any(|local| local == "ln");
                match name.local.as_str() {
                    "cNvPr" => {
                        shape.id = plain_u32(attributes, "id").unwrap_or(0);
                        shape.name = plain(attributes, "name").unwrap_or_default().to_owned();
                        shape.alternative_text = plain(attributes, "descr")
                            .or_else(|| plain(attributes, "title"))
                            .map(str::to_owned);
                    }
                    "hlinkClick" => {
                        shape.hyperlink_relationship_id = attributes
                            .iter()
                            .find(|attribute| attribute.name.local == "id")
                            .map(|attribute| attribute.value.clone());
                    }
                    "ph" => {
                        shape.placeholder = Some(Placeholder {
                            kind: plain(attributes, "type").unwrap_or("body").to_owned(),
                            index: plain_u32(attributes, "idx").unwrap_or(0),
                        });
                    }
                    "xfrm" if shape.transform.is_none() => {
                        if let Some(xfrm_end) = element_end(document, index) {
                            shape.transform = Some(parse_transform(document, index, xfrm_end));
                        }
                    }
                    "prstGeom" => {
                        shape.geometry = plain(attributes, "prst").and_then(preset_geometry);
                        if shape.geometry.is_none() {
                            diagnostics.push((
                                ResolveDiagnosticCode::UnsupportedCustomGeometry,
                                Some(shape.id),
                                format!(
                                    "unsupported preset geometry {:?}; rectangle fallback used",
                                    plain(attributes, "prst")
                                ),
                            ));
                        }
                    }
                    "custGeom" => diagnostics.push((
                        ResolveDiagnosticCode::UnsupportedCustomGeometry,
                        Some(shape.id),
                        "custom geometry is retained in source but not lowered yet".to_owned(),
                    )),
                    "solidFill" => {
                        if let Some(fill_end) = element_end(document, index) {
                            let color =
                                parse_color(document, index, fill_end, theme).unwrap_or(BLACK);
                            if inside_line {
                                let width =
                                    nearest_line_width(document, start, index).unwrap_or(12_700);
                                shape.stroke = Some(Stroke {
                                    color,
                                    width,
                                    dash: nearest_dash(document, index, fill_end),
                                });
                            } else if shape.fill.is_none() {
                                shape.fill = Some(Fill::Solid(color));
                            }
                        }
                    }
                    "noFill" if inside_line => shape.stroke = None,
                    "noFill" => shape.fill = Some(Fill::None),
                    "gradFill" | "pattFill" | "blipFill" if name.local != "blipFill" => {
                        diagnostics.push((
                            ResolveDiagnosticCode::UnsupportedFill,
                            Some(shape.id),
                            format!("{} requires a renderer fallback", name.local),
                        ));
                    }
                    "blip" => {
                        shape.image_relationship_id = attributes
                            .iter()
                            .find(|attribute| attribute.name.local == "embed")
                            .map(|attribute| attribute.value.clone());
                    }
                    "srcRect" => {
                        shape.crop = ImageCrop {
                            left: plain_i32(attributes, "l").unwrap_or(0),
                            top: plain_i32(attributes, "t").unwrap_or(0),
                            right: plain_i32(attributes, "r").unwrap_or(0),
                            bottom: plain_i32(attributes, "b").unwrap_or(0),
                        };
                    }
                    "t" => in_text = true,
                    "effectLst" | "effectDag" => diagnostics.push((
                        ResolveDiagnosticCode::UnsupportedEffect,
                        Some(shape.id),
                        format!("{} is retained but not rendered", name.local),
                    )),
                    _ => {}
                }
                if !empty {
                    stack.push(name.local.clone());
                }
            }
            TokenKind::Text | TokenKind::Cdata if in_text => {
                let range = if matches!(&token.kind, TokenKind::Cdata) {
                    token.range.start + 9..token.range.end - 3
                } else {
                    token.range.clone()
                };
                let raw = std::str::from_utf8(document.source_range(range)).unwrap_or_default();
                if matches!(&token.kind, TokenKind::Cdata) {
                    text.push_str(raw);
                } else if let Ok(decoded) = decode_entities(raw, token.range.start) {
                    text.push_str(&decoded);
                }
            }
            TokenKind::End { name } => {
                if name.local == "t" {
                    in_text = false;
                }
                if stack.last().is_some_and(|local| *local == name.local) {
                    stack.pop();
                }
            }
            _ => {}
        }
    }
    if !text.is_empty() {
        shape.text = Some(text);
    }
    shape
}

fn parse_group_transform(document: &XmlDocument, start: usize, end: usize) -> GroupTransform {
    let cutoff = (start + 1..=end)
        .find(|index| {
            matches!(
                &document.tokens()[*index].kind,
                TokenKind::Start { name, .. }
                    if matches!(name.local.as_str(), "sp" | "pic" | "cxnSp" | "grpSp" | "graphicFrame")
            )
        })
        .unwrap_or(end);
    let outer = (start..=cutoff)
        .find(|index| {
            matches!(&document.tokens()[*index].kind, TokenKind::Start { name, .. } if name.local == "xfrm")
        })
        .map(|xfrm| {
            parse_transform(
                document,
                xfrm,
                element_end(document, xfrm).unwrap_or(cutoff),
            )
        })
        .unwrap_or_default();
    let mut child_origin = EmuPoint::default();
    let mut child_size = EmuSize::default();
    for token in &document.tokens()[start..=cutoff] {
        let TokenKind::Start {
            name, attributes, ..
        } = &token.kind
        else {
            continue;
        };
        match name.local.as_str() {
            "chOff" => {
                child_origin.x = plain_i64(attributes, "x").unwrap_or(0);
                child_origin.y = plain_i64(attributes, "y").unwrap_or(0);
            }
            "chExt" => {
                child_size.width = plain_i64(attributes, "cx").unwrap_or(0);
                child_size.height = plain_i64(attributes, "cy").unwrap_or(0);
            }
            _ => {}
        }
    }
    GroupTransform {
        outer,
        child_origin,
        child_size,
    }
}

fn parse_transform(document: &XmlDocument, start: usize, end: usize) -> Transform {
    let mut transform = Transform::default();
    if let TokenKind::Start { attributes, .. } = &document.tokens()[start].kind {
        transform.rotation = plain_i32(attributes, "rot").unwrap_or(0);
        transform.flip_horizontal = plain(attributes, "flipH") == Some("1");
        transform.flip_vertical = plain(attributes, "flipV") == Some("1");
    }
    for token in &document.tokens()[start..=end] {
        let TokenKind::Start {
            name, attributes, ..
        } = &token.kind
        else {
            continue;
        };
        match name.local.as_str() {
            "off" => {
                transform.bounds.origin.x = plain_i64(attributes, "x").unwrap_or(0);
                transform.bounds.origin.y = plain_i64(attributes, "y").unwrap_or(0);
            }
            "ext" => {
                transform.bounds.size.width = plain_i64(attributes, "cx").unwrap_or(0);
                transform.bounds.size.height = plain_i64(attributes, "cy").unwrap_or(0);
            }
            _ => {}
        }
    }
    transform
}

fn parse_theme(document: &XmlDocument) -> Result<Theme, String> {
    let mut theme = Theme::default();
    let mut scheme_depth = None;
    let mut slot: Option<(usize, String)> = None;
    for token in document.tokens() {
        match &token.kind {
            TokenKind::Start { name, .. } if name.local == "clrScheme" => {
                scheme_depth = Some(token.depth)
            }
            TokenKind::Start {
                name, attributes, ..
            } if scheme_depth.is_some_and(|depth| token.depth == depth + 1) => {
                slot = Some((token.depth, name.local.clone()));
                if let Some(value) =
                    color_attribute(name.local.as_str(), attributes).and_then(parse_hex_color)
                {
                    theme.colors.insert(name.local.clone(), value);
                }
            }
            TokenKind::Start {
                name, attributes, ..
            } if slot.is_some() && matches!(name.local.as_str(), "srgbClr" | "sysClr") => {
                let value = if name.local == "sysClr" {
                    plain(attributes, "lastClr").or_else(|| plain(attributes, "val"))
                } else {
                    plain(attributes, "val")
                };
                if let (Some((_, slot_name)), Some(color)) =
                    (&slot, value.and_then(parse_hex_color))
                {
                    theme.colors.insert(slot_name.clone(), color);
                }
            }
            TokenKind::End { name } if name.local == "clrScheme" => scheme_depth = None,
            TokenKind::End { .. }
                if slot
                    .as_ref()
                    .is_some_and(|(depth, _)| *depth == token.depth) =>
            {
                slot = None
            }
            _ => {}
        }
    }
    Ok(theme)
}

fn apply_color_map(document: &XmlDocument, theme: &mut Theme) {
    for token in document.tokens() {
        let TokenKind::Start {
            name, attributes, ..
        } = &token.kind
        else {
            continue;
        };
        if matches!(name.local.as_str(), "clrMap" | "overrideClrMapping") {
            for attribute in attributes
                .iter()
                .filter(|attribute| attribute.name.namespace.is_none())
            {
                theme
                    .mapping
                    .insert(attribute.name.local.clone(), attribute.value.clone());
            }
        }
    }
}

fn parse_background(document: &XmlDocument, theme: &Theme) -> Option<RgbaColor> {
    let background = document.tokens().iter().position(
        |token| matches!(&token.kind, TokenKind::Start { name, .. } if name.local == "bg"),
    )?;
    let end = element_end(document, background)?;
    let fill = (background..=end).find(|index| {
        matches!(&document.tokens()[*index].kind, TokenKind::Start { name, .. } if name.local == "solidFill")
    })?;
    parse_color(document, fill, element_end(document, fill)?, theme)
}

fn parse_color(
    document: &XmlDocument,
    start: usize,
    end: usize,
    theme: &Theme,
) -> Option<RgbaColor> {
    let mut color = None;
    let mut transforms = Vec::<(String, i32)>::new();
    for token in &document.tokens()[start..=end] {
        let TokenKind::Start {
            name, attributes, ..
        } = &token.kind
        else {
            continue;
        };
        match name.local.as_str() {
            "srgbClr" => color = plain(attributes, "val").and_then(parse_hex_color),
            "sysClr" => {
                color = plain(attributes, "lastClr")
                    .or_else(|| plain(attributes, "val"))
                    .and_then(parse_hex_color)
            }
            "schemeClr" => {
                if let Some(slot) = plain(attributes, "val") {
                    let mapped = theme.mapping.get(slot).map_or(slot, String::as_str);
                    color = theme.colors.get(mapped).copied();
                }
            }
            "tint" | "shade" | "lumMod" | "lumOff" | "alpha" => {
                if let Some(value) = plain_i32(attributes, "val") {
                    transforms.push((name.local.clone(), value));
                }
            }
            _ => {}
        }
    }
    let mut color = color?;
    for (kind, value) in transforms {
        color = apply_color_transform(color, &kind, value);
    }
    Some(color)
}

fn apply_color_transform(mut color: RgbaColor, kind: &str, value: i32) -> RgbaColor {
    let scale = value.clamp(0, 100_000) as i64;
    let channel = |component: u8, operation: &str| -> u8 {
        let component = component as i64;
        let output = match operation {
            "tint" => component + (255 - component) * scale / 100_000,
            "shade" | "lumMod" => component * scale / 100_000,
            "lumOff" => component + 255 * scale / 100_000,
            _ => component,
        };
        output.clamp(0, 255) as u8
    };
    if kind == "alpha" {
        color.alpha = ((255_i64 * scale) / 100_000) as u8;
    } else {
        color.red = channel(color.red, kind);
        color.green = channel(color.green, kind);
        color.blue = channel(color.blue, kind);
    }
    color
}

fn append_diagnostics(output: &mut Vec<ResolveDiagnostic>, part_name: &str, parsed: &ParsedPart) {
    output.extend(
        parsed
            .diagnostics
            .iter()
            .map(|(code, shape_id, message)| ResolveDiagnostic {
                code: *code,
                part_name: part_name.to_owned(),
                shape_id: *shape_id,
                message: message.clone(),
            }),
    );
}

fn element_end(document: &XmlDocument, start: usize) -> Option<usize> {
    let token = document.tokens().get(start)?;
    let TokenKind::Start { name, empty, .. } = &token.kind else {
        return None;
    };
    if *empty {
        return Some(start);
    }
    document
        .tokens()
        .iter()
        .enumerate()
        .skip(start + 1)
        .find_map(|(index, candidate)| {
            matches!(&candidate.kind, TokenKind::End { name: end_name }
                if candidate.depth == token.depth && end_name.local == name.local)
            .then_some(index)
        })
}

fn preset_geometry(value: &str) -> Option<PresetGeometry> {
    match value {
        "rect" => Some(PresetGeometry::Rect),
        "roundRect" => Some(PresetGeometry::RoundRect),
        "ellipse" => Some(PresetGeometry::Ellipse),
        "line" => Some(PresetGeometry::Line),
        "triangle" => Some(PresetGeometry::Triangle),
        "rtTriangle" => Some(PresetGeometry::RightTriangle),
        "diamond" => Some(PresetGeometry::Diamond),
        "parallelogram" => Some(PresetGeometry::Parallelogram),
        "hexagon" => Some(PresetGeometry::Hexagon),
        _ => None,
    }
}

fn nearest_line_width(document: &XmlDocument, start: usize, before: usize) -> Option<i64> {
    document.tokens()[start..=before]
        .iter()
        .rev()
        .find_map(|token| {
            let TokenKind::Start {
                name, attributes, ..
            } = &token.kind
            else {
                return None;
            };
            (name.local == "ln")
                .then(|| plain_i64(attributes, "w"))
                .flatten()
        })
}

fn nearest_dash(document: &XmlDocument, start: usize, end: usize) -> Option<String> {
    document.tokens()[start..=end].iter().find_map(|token| {
        let TokenKind::Start {
            name, attributes, ..
        } = &token.kind
        else {
            return None;
        };
        (name.local == "prstDash")
            .then(|| plain(attributes, "val").map(str::to_owned))
            .flatten()
    })
}

fn color_attribute<'a>(local: &str, attributes: &'a [Attribute]) -> Option<&'a str> {
    matches!(local, "srgbClr" | "sysClr")
        .then(|| plain(attributes, "val"))
        .flatten()
}

fn parse_hex_color(value: &str) -> Option<RgbaColor> {
    if value.len() != 6 {
        return None;
    }
    Some(RgbaColor {
        red: u8::from_str_radix(&value[0..2], 16).ok()?,
        green: u8::from_str_radix(&value[2..4], 16).ok()?,
        blue: u8::from_str_radix(&value[4..6], 16).ok()?,
        alpha: 255,
    })
}

fn plain<'a>(attributes: &'a [Attribute], local: &str) -> Option<&'a str> {
    attributes
        .iter()
        .find(|attribute| attribute.name.namespace.is_none() && attribute.name.local == local)
        .map(|attribute| attribute.value.as_str())
}

fn plain_u32(attributes: &[Attribute], local: &str) -> Option<u32> {
    plain(attributes, local)?.parse().ok()
}

fn plain_i32(attributes: &[Attribute], local: &str) -> Option<i32> {
    plain(attributes, local)?.parse().ok()
}

fn deduplicate_trace(trace: &mut ResolutionTrace) {
    let mut visited = HashSet::new();
    trace
        .visited_parts
        .retain(|part| visited.insert(part.clone()));
    let mut parsed = HashSet::new();
    trace
        .parsed_xml_parts
        .retain(|part| parsed.insert(part.clone()));
}
