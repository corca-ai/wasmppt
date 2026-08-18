use std::collections::HashSet;

use wasmppt_opc::{PackageGraph, PackagePartSource, PartId, RelationshipTarget};
use wasmppt_pml::PresentationView;
use wasmppt_xml::{Attribute, TokenKind, XmlDocument, decode_entities};

use crate::{
    ChartGrouping, ChartKind, ChartSeries, CustomPath, ElementKind, EmbeddedFontResource,
    EmbeddedFontStyle, EmuPoint, EmuSize, Fill, GradientStop, GroupTransform, ImageCrop,
    LayoutError, LineEnd, OuterShadow, PathCommand, Placeholder, PreservedFeature, PresetGeometry,
    PropertyProvenance, ResolutionTrace, ResolveDiagnostic, ResolveDiagnosticCode, ResolveOutput,
    ResolvedBulletImage, ResolvedChart, ResolvedElement, ResolvedParagraph, ResolvedSlide,
    ResolvedTable, ResolvedTableCell, ResolvedTableRow, ResolvedTextFrame, ResolvedTextRun,
    ResolvedTextStyle, ResolvedTextTab, RgbaColor, SourceLevel, Stroke, TableCellBorders,
    TextAlignment, TextAutofit, TextDirection, TextFlow, TextFontAlignment, TextGlow, TextSpacing,
    TextTabAlignment, TextVerticalAlignment, TextWarp, Transform, plain_i64,
};

mod color;

#[cfg(test)]
use color::parse_hex_color;
use color::{
    BLACK, Theme, WHITE, apply_color_map, apply_color_transform, parse_background, parse_color,
    parse_theme,
};

const MARKUP_COMPATIBILITY: &str = "http://schemas.openxmlformats.org/markup-compatibility/2006";

#[derive(Clone, Debug, Default)]
struct RawShape {
    id: u32,
    name: String,
    placeholder: Option<Placeholder>,
    transform: Option<Transform>,
    groups: Vec<GroupTransform>,
    fill: Option<Fill>,
    stroke: Option<Stroke>,
    custom_path: Option<CustomPath>,
    outer_shadow: Option<OuterShadow>,
    text: Option<String>,
    text_style: PartialTextStyle,
    text_frame: Option<RawTextFrame>,
    alternative_text: Option<String>,
    hyperlink_relationship_id: Option<String>,
    table: Option<ResolvedTable>,
    chart_relationship_id: Option<String>,
    preserved_graphic: Option<PreservedFeature>,
    geometry: Option<PresetGeometry>,
    image_relationship_id: Option<String>,
    crop: ImageCrop,
    provenance: Vec<PropertyProvenance>,
}

#[derive(Clone, Debug, Default)]
struct RawTextRun {
    text: String,
    field_type: Option<String>,
    style: PartialTextStyle,
    east_asian_font_family: Option<String>,
    complex_script_font_family: Option<String>,
}

#[derive(Clone, Debug, Default)]
struct RawParagraph {
    runs: Vec<RawTextRun>,
    style: PartialTextStyle,
    level: u8,
    margin_left: Option<i64>,
    indent: Option<i64>,
    line_spacing: Option<TextSpacing>,
    space_before: Option<TextSpacing>,
    space_after: Option<TextSpacing>,
    direction: TextDirection,
    tabs: Vec<ResolvedTextTab>,
    font_alignment: TextFontAlignment,
    auto_number_scheme: Option<String>,
    auto_number_start: u32,
    bullet_image_relationship_id: Option<String>,
    bullet_font_family: Option<String>,
    bullet_color: Option<RgbaColor>,
    bullet_size: Option<TextSpacing>,
}

#[derive(Clone, Debug)]
struct RawTextFrame {
    paragraphs: Vec<RawParagraph>,
    wrap: bool,
    autofit: TextAutofit,
    autofit_font_scale: Option<i32>,
    autofit_line_spacing_reduction: Option<i32>,
    flow: TextFlow,
    column_count: u8,
    column_spacing: i64,
    default_tab_size: i64,
    warp: Option<TextWarp>,
    unsupported_warp: Option<String>,
    invalid_autofit_hint: bool,
}

#[derive(Debug, Default)]
struct ParsedPart {
    shapes: Vec<RawShape>,
    background: Option<RgbaColor>,
    diagnostics: Vec<(ResolveDiagnosticCode, Option<u32>, String)>,
    text_styles: MasterTextStyles,
    show_master_shapes: Option<bool>,
    show_header: Option<bool>,
    show_footer: Option<bool>,
    show_date_time: Option<bool>,
    show_slide_number: Option<bool>,
}

#[derive(Clone, Debug, Default)]
struct PartialTextStyle {
    font_size: Option<i32>,
    color: Option<RgbaColor>,
    font_family: Option<String>,
    bold: Option<bool>,
    italic: Option<bool>,
    underline: Option<bool>,
    strike: Option<bool>,
    character_spacing: Option<i32>,
    baseline: Option<i32>,
    outline: Option<Option<Stroke>>,
    shadow: Option<Option<OuterShadow>>,
    inner_shadow: Option<Option<OuterShadow>>,
    text_fill: Option<Fill>,
    glow: Option<Option<TextGlow>>,
    blur_radius: Option<i64>,
    soft_edge_radius: Option<i64>,
    reflection: Option<bool>,
    alignment: Option<TextAlignment>,
    vertical_alignment: Option<TextVerticalAlignment>,
    margin_left: Option<i64>,
    margin_top: Option<i64>,
    margin_right: Option<i64>,
    margin_bottom: Option<i64>,
    bullet: Option<Option<String>>,
    bullet_font_family: Option<Option<String>>,
    bullet_color: Option<Option<RgbaColor>>,
    bullet_size: Option<Option<TextSpacing>>,
    auto_number_scheme: Option<Option<String>>,
    auto_number_start: Option<u32>,
}

impl PartialTextStyle {
    fn has_values(&self) -> bool {
        self.font_size.is_some()
            || self.color.is_some()
            || self.font_family.is_some()
            || self.bold.is_some()
            || self.italic.is_some()
            || self.underline.is_some()
            || self.strike.is_some()
            || self.character_spacing.is_some()
            || self.baseline.is_some()
            || self.outline.is_some()
            || self.shadow.is_some()
            || self.inner_shadow.is_some()
            || self.text_fill.is_some()
            || self.glow.is_some()
            || self.blur_radius.is_some()
            || self.soft_edge_radius.is_some()
            || self.reflection.is_some()
            || self.alignment.is_some()
            || self.vertical_alignment.is_some()
            || self.margin_left.is_some()
            || self.margin_top.is_some()
            || self.margin_right.is_some()
            || self.margin_bottom.is_some()
            || self.bullet.is_some()
            || self.bullet_font_family.is_some()
            || self.bullet_color.is_some()
            || self.bullet_size.is_some()
            || self.auto_number_scheme.is_some()
            || self.auto_number_start.is_some()
    }

    fn overlay(&mut self, other: &Self) {
        macro_rules! replace_some {
            ($field:ident) => {
                if other.$field.is_some() {
                    self.$field = other.$field.clone();
                }
            };
        }
        replace_some!(font_size);
        replace_some!(color);
        replace_some!(font_family);
        replace_some!(bold);
        replace_some!(italic);
        replace_some!(underline);
        replace_some!(strike);
        replace_some!(character_spacing);
        replace_some!(baseline);
        replace_some!(outline);
        replace_some!(shadow);
        replace_some!(inner_shadow);
        replace_some!(text_fill);
        replace_some!(glow);
        replace_some!(blur_radius);
        replace_some!(soft_edge_radius);
        replace_some!(reflection);
        replace_some!(alignment);
        replace_some!(vertical_alignment);
        replace_some!(margin_left);
        replace_some!(margin_top);
        replace_some!(margin_right);
        replace_some!(margin_bottom);
        replace_some!(bullet);
        replace_some!(bullet_font_family);
        replace_some!(bullet_color);
        replace_some!(bullet_size);
        replace_some!(auto_number_scheme);
        replace_some!(auto_number_start);
    }

    fn fill_missing_from(&mut self, fallback: &Self) {
        macro_rules! fill_missing {
            ($field:ident) => {
                if self.$field.is_none() {
                    self.$field = fallback.$field.clone();
                }
            };
        }
        fill_missing!(font_size);
        fill_missing!(color);
        fill_missing!(font_family);
        fill_missing!(bold);
        fill_missing!(italic);
        fill_missing!(underline);
        fill_missing!(strike);
        fill_missing!(character_spacing);
        fill_missing!(baseline);
        fill_missing!(outline);
        fill_missing!(shadow);
        fill_missing!(inner_shadow);
        fill_missing!(text_fill);
        fill_missing!(glow);
        fill_missing!(blur_radius);
        fill_missing!(soft_edge_radius);
        fill_missing!(reflection);
        fill_missing!(alignment);
        fill_missing!(vertical_alignment);
        fill_missing!(margin_left);
        fill_missing!(margin_top);
        fill_missing!(margin_right);
        fill_missing!(margin_bottom);
        fill_missing!(bullet);
        fill_missing!(bullet_font_family);
        fill_missing!(bullet_color);
        fill_missing!(bullet_size);
        fill_missing!(auto_number_scheme);
        fill_missing!(auto_number_start);
    }

    fn resolve(&self) -> ResolvedTextStyle {
        let defaults = ResolvedTextStyle::default();
        ResolvedTextStyle {
            font_size: self.font_size.unwrap_or(defaults.font_size),
            color: self.color.unwrap_or(defaults.color),
            font_family: self.font_family.clone(),
            bold: self.bold.unwrap_or(defaults.bold),
            italic: self.italic.unwrap_or(defaults.italic),
            underline: self.underline.unwrap_or(defaults.underline),
            strike: self.strike.unwrap_or(defaults.strike),
            character_spacing: self.character_spacing.unwrap_or(defaults.character_spacing),
            baseline: self.baseline.unwrap_or(defaults.baseline),
            outline: self.outline.clone().flatten(),
            shadow: self.shadow.flatten(),
            inner_shadow: self.inner_shadow.flatten(),
            fill: self.text_fill.clone(),
            glow: self.glow.flatten(),
            blur_radius: self.blur_radius.unwrap_or(defaults.blur_radius),
            soft_edge_radius: self.soft_edge_radius.unwrap_or(defaults.soft_edge_radius),
            reflection: self.reflection.unwrap_or(defaults.reflection),
            alignment: self.alignment.unwrap_or(defaults.alignment),
            vertical_alignment: self
                .vertical_alignment
                .unwrap_or(defaults.vertical_alignment),
            margin_left: self.margin_left.unwrap_or(defaults.margin_left),
            margin_top: self.margin_top.unwrap_or(defaults.margin_top),
            margin_right: self.margin_right.unwrap_or(defaults.margin_right),
            margin_bottom: self.margin_bottom.unwrap_or(defaults.margin_bottom),
        }
    }
}

#[derive(Clone, Debug)]
struct TextStyleLevels {
    levels: [PartialTextStyle; 9],
}

impl Default for TextStyleLevels {
    fn default() -> Self {
        Self {
            levels: std::array::from_fn(|_| PartialTextStyle::default()),
        }
    }
}

#[derive(Clone, Debug, Default)]
struct MasterTextStyles {
    title: TextStyleLevels,
    body: TextStyleLevels,
    other: TextStyleLevels,
}

pub fn resolve_slide_parts(
    source: &dyn PackagePartSource,
    graph: &PackageGraph,
    slide_id: PartId,
    slide_size: EmuSize,
) -> Result<ResolveOutput, LayoutError> {
    let embedded_fonts = discover_embedded_fonts(source, graph)?;
    let slide_number = presentation_slide_number(source, graph, slide_id);
    resolve_slide_parts_cached(
        source,
        graph,
        slide_id,
        slide_size,
        &embedded_fonts,
        slide_number,
    )
}

fn presentation_slide_number(
    source: &dyn PackagePartSource,
    graph: &PackageGraph,
    slide_id: PartId,
) -> Option<u32> {
    let presentation_id = graph
        .package_relationships()
        .iter()
        .find(|relationship| {
            graph
                .relationship_type(relationship)
                .ends_with("/officeDocument")
        })
        .and_then(|relationship| match relationship.target {
            RelationshipTarget::Internal(part) => Some(part),
            _ => None,
        })?;
    let presentation_name = graph.part_name(graph.part(presentation_id));
    let presentation = PresentationView::parse(source.read_part(presentation_name).ok()?).ok()?;
    presentation
        .slide_relationship_ids()
        .iter()
        .filter_map(|id| {
            graph
                .part(presentation_id)
                .relationships
                .iter()
                .find(|relationship| graph.relationship_id(relationship) == id)
                .and_then(|relationship| match relationship.target {
                    RelationshipTarget::Internal(part) => Some(part),
                    _ => None,
                })
        })
        .position(|part| part == slide_id)
        .and_then(|index| u32::try_from(index).ok())
        .and_then(|index| index.checked_add(1))
}

pub(crate) fn resolve_slide_parts_cached(
    source: &dyn PackagePartSource,
    graph: &PackageGraph,
    slide_id: PartId,
    slide_size: EmuSize,
    embedded_fonts: &[EmbeddedFontResource],
    slide_number: Option<u32>,
) -> Result<ResolveOutput, LayoutError> {
    let layout_id = related(graph, slide_id, "/slideLayout");
    let master_id = layout_id.and_then(|part| related(graph, part, "/slideMaster"));
    let theme_id = master_id.and_then(|part| related(graph, part, "/theme"));
    let mut trace = ResolutionTrace::default();
    let mut diagnostics = Vec::new();

    let theme = if let Some(part) = theme_id {
        let (name, document) = parse_part(source, graph, part, &mut trace)?;
        parse_theme(&document).map_err(|message| {
            LayoutError::new(format!("cannot parse theme part {name}: {message}"))
                .with_part_name(name)
        })?
    } else {
        Theme::default()
    };
    let master = if let Some(part) = master_id {
        let (name, document) = parse_part(source, graph, part, &mut trace)?;
        let mut adjusted = theme.clone();
        apply_color_map(&document, &mut adjusted);
        let parsed = parse_drawing_part(&document, &adjusted);
        append_diagnostics(&mut diagnostics, &name, &parsed);
        Some((part, parsed, adjusted))
    } else {
        None
    };
    let layout = if let Some(part) = layout_id {
        let (name, document) = parse_part(source, graph, part, &mut trace)?;
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
    let (slide_name, slide_document) = parse_part(source, graph, slide_id, &mut trace)?;
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
            source,
            graph,
            diagnostics: &mut diagnostics,
            trace: &mut trace,
            slide_number,
        };
        let show_master_shapes = slide.show_master_shapes.unwrap_or(true)
            && layout
                .as_ref()
                .and_then(|(_, part, _)| part.show_master_shapes)
                .unwrap_or(true);
        if show_master_shapes {
            if let Some((part, parsed, _)) = &master {
                append_non_placeholder(
                    &mut elements,
                    &mut element_resolver,
                    parsed,
                    SourceLevel::Master,
                    *part,
                );
            }
        }
        if let Some((part, parsed, _)) = &layout {
            append_non_placeholder(
                &mut elements,
                &mut element_resolver,
                parsed,
                SourceLevel::Layout,
                *part,
            );
            append_unmaterialized_placeholders(
                &mut elements,
                &mut element_resolver,
                parsed,
                SourceLevel::Layout,
                *part,
                &slide,
                None,
            );
        }
        if show_master_shapes {
            if let Some((part, parsed, _)) = &master {
                append_unmaterialized_placeholders(
                    &mut elements,
                    &mut element_resolver,
                    parsed,
                    SourceLevel::Master,
                    *part,
                    &slide,
                    layout.as_ref().map(|(_, part, _)| part),
                );
            }
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
            let text_relationship_part = if raw.text_frame.is_some() {
                slide_id
            } else if inherited_layout.is_some_and(|shape| shape.text_frame.is_some()) {
                layout_id.unwrap_or(slide_id)
            } else if inherited_master.is_some_and(|shape| shape.text_frame.is_some()) {
                master_id.unwrap_or(slide_id)
            } else {
                slide_id
            };
            let mut merged = merge_shape(raw, inherited_layout, inherited_master);
            if let Some((_, master_part, _)) = &master {
                let master_styles =
                    master_text_style_for(&master_part.text_styles, merged.placeholder.as_ref());
                apply_master_text_styles(&mut merged, master_styles);
            }
            elements.push(resolve_element(
                &merged,
                &mut element_resolver,
                SourceLevel::Slide,
                slide_id,
                text_relationship_part,
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
        embedded_fonts: embedded_fonts.to_vec(),
        diagnostics,
        trace,
    })
}

pub(crate) fn discover_embedded_fonts(
    source: &dyn PackagePartSource,
    graph: &PackageGraph,
) -> Result<Vec<EmbeddedFontResource>, LayoutError> {
    let Some(presentation) = graph
        .package_relationships()
        .iter()
        .find_map(|relationship| {
            (graph
                .relationship_type(relationship)
                .ends_with("/officeDocument"))
            .then_some(&relationship.target)
            .and_then(|target| match target {
                RelationshipTarget::Internal(part) => Some(*part),
                _ => None,
            })
        })
    else {
        return Ok(Vec::new());
    };
    let name = graph.part_name(graph.part(presentation));
    let bytes = source
        .read_part(name)
        .map_err(|error| super::package_error(error).with_part_name(name))?;
    let document = XmlDocument::parse(bytes).map_err(|error| LayoutError::xml(error, name))?;
    let mut fonts = Vec::new();
    for (index, token) in document.tokens().iter().enumerate() {
        let TokenKind::Start { name, .. } = &token.kind else {
            continue;
        };
        if name.local != "embeddedFont" {
            continue;
        }
        let end = element_end(&document, index).unwrap_or(index);
        let family = (index..=end).find_map(|candidate| match &document.tokens()[candidate].kind {
            TokenKind::Start {
                name, attributes, ..
            } if name.local == "font" => plain(attributes, "typeface").map(str::to_owned),
            _ => None,
        });
        let Some(family) = family else { continue };
        for candidate in index..=end {
            let TokenKind::Start {
                name, attributes, ..
            } = &document.tokens()[candidate].kind
            else {
                continue;
            };
            let style = match name.local.as_str() {
                "regular" => EmbeddedFontStyle::Regular,
                "bold" => EmbeddedFontStyle::Bold,
                "italic" => EmbeddedFontStyle::Italic,
                "boldItalic" => EmbeddedFontStyle::BoldItalic,
                _ => continue,
            };
            let Some(id) = attributes
                .iter()
                .find(|attribute| {
                    attribute.name.local == "id" && attribute.name.namespace.is_some()
                })
                .map(|attribute| attribute.value.as_str())
            else {
                continue;
            };
            let Some(part) = relationship_target(graph, presentation, id) else {
                continue;
            };
            fonts.push(EmbeddedFontResource {
                family: family.clone(),
                style,
                part_name: graph.part_name(graph.part(part)).to_owned(),
            });
        }
    }
    fonts.sort_by(|left, right| {
        left.family
            .cmp(&right.family)
            .then(left.part_name.cmp(&right.part_name))
    });
    fonts.dedup();
    Ok(fonts)
}

fn parse_part(
    source: &dyn PackagePartSource,
    graph: &PackageGraph,
    part: PartId,
    trace: &mut ResolutionTrace,
) -> Result<(String, XmlDocument), LayoutError> {
    let name = graph.part_name(graph.part(part)).to_owned();
    trace.visited_parts.push(name.clone());
    let bytes = source
        .read_part(&name)
        .map_err(|error| super::package_error(error).with_part_name(&name))?;
    let document = XmlDocument::parse(bytes).map_err(|error| LayoutError::xml(error, &name))?;
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
            shape, resolver, source, part_id, part_id, &part_name,
        ));
    }
}

fn append_unmaterialized_placeholders(
    output: &mut Vec<ResolvedElement>,
    resolver: &mut ElementResolver<'_>,
    part: &ParsedPart,
    source: SourceLevel,
    part_id: PartId,
    slide: &ParsedPart,
    nearer: Option<&ParsedPart>,
) {
    let part_name = resolver
        .graph
        .part_name(resolver.graph.part(part_id))
        .to_owned();
    for shape in &part.shapes {
        let Some(placeholder) = &shape.placeholder else {
            continue;
        };
        let enabled = match placeholder.kind.as_str() {
            "hdr" => slide.show_header.or(part.show_header).unwrap_or(true),
            "ftr" => slide.show_footer.or(part.show_footer).unwrap_or(true),
            "dt" => slide.show_date_time.or(part.show_date_time).unwrap_or(true),
            "sldNum" => slide
                .show_slide_number
                .or(part.show_slide_number)
                .unwrap_or(true),
            _ => false,
        };
        if !enabled
            || slide
                .shapes
                .iter()
                .any(|candidate| placeholder_matches(placeholder, candidate.placeholder.as_ref()))
            || nearer.is_some_and(|nearer| {
                nearer.shapes.iter().any(|candidate| {
                    placeholder_matches(placeholder, candidate.placeholder.as_ref())
                })
            })
        {
            continue;
        }
        output.push(resolve_element(
            shape, resolver, source, part_id, part_id, &part_name,
        ));
    }
}

struct ElementResolver<'a> {
    source: &'a dyn PackagePartSource,
    graph: &'a PackageGraph,
    diagnostics: &'a mut Vec<ResolveDiagnostic>,
    trace: &'a mut ResolutionTrace,
    slide_number: Option<u32>,
}

fn resolve_element(
    shape: &RawShape,
    resolver: &mut ElementResolver<'_>,
    source: SourceLevel,
    source_part: PartId,
    text_relationship_part: PartId,
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
                    parse_part(resolver.source, graph, part, resolver.trace).ok()?;
                let mut chart = parse_chart(&document);
                chart.embedded_workbook = related(graph, part, "/package").map(|workbook| {
                    let name = graph.part_name(graph.part(workbook)).to_owned();
                    resolver.trace.visited_parts.push(name.clone());
                    name
                });
                if chart.kind == ChartKind::Other {
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
                    grouping: ChartGrouping::Standard,
                    series: Vec::new(),
                    title: None,
                    show_legend: false,
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
    let source_modified = resolver.source.is_modified(source_name);
    resolved_element(
        shape,
        source,
        kind,
        &ResolvedElementContext {
            graph,
            source_part,
            text_relationship_part,
            source_modified,
            slide_number: resolver.slide_number,
        },
    )
}

struct ResolvedElementContext<'a> {
    graph: &'a PackageGraph,
    source_part: PartId,
    text_relationship_part: PartId,
    source_modified: bool,
    slide_number: Option<u32>,
}

fn resolved_element(
    shape: &RawShape,
    source: SourceLevel,
    kind: ElementKind,
    context: &ResolvedElementContext<'_>,
) -> ResolvedElement {
    let text_style = shape.text_style.resolve();
    let mut text = apply_paragraph_markers(
        shape.text.as_deref().unwrap_or_default(),
        shape
            .text_style
            .bullet
            .as_ref()
            .and_then(|value| value.as_deref()),
    );
    let materializes_slide_number = context.slide_number.is_some()
        && shape.text_frame.as_ref().is_some_and(|frame| {
            frame.paragraphs.iter().any(|paragraph| {
                paragraph.runs.iter().any(|run| {
                    run.field_type
                        .as_deref()
                        .is_some_and(|field_type| field_type.eq_ignore_ascii_case("slidenum"))
                })
            })
        });
    let text_frame = shape.text_frame.as_ref().map(|frame| {
        let mut resolved = resolve_text_frame(
            frame,
            &shape.text_style,
            context.source_modified,
            context.slide_number,
        );
        for (raw, paragraph) in frame.paragraphs.iter().zip(&mut resolved.paragraphs) {
            if let Some(id) = &raw.bullet_image_relationship_id {
                paragraph.bullet_image = Some(ResolvedBulletImage {
                    relationship_id: id.clone(),
                    part_name: relationship_target(
                        context.graph,
                        context.text_relationship_part,
                        id,
                    )
                    .map(|part| context.graph.part_name(context.graph.part(part)).to_owned()),
                });
            }
        }
        resolved
    });
    if materializes_slide_number {
        text = text_frame
            .as_ref()
            .map(resolved_text_frame_plain_text)
            .unwrap_or_default();
    }
    ResolvedElement {
        id: shape.id,
        name: shape.name.clone(),
        source,
        provenance: if shape.provenance.is_empty() {
            [
                "transform",
                "geometry",
                "fill",
                "stroke",
                "text",
                "text-style",
            ]
            .into_iter()
            .map(|property| PropertyProvenance { property, source })
            .collect()
        } else {
            shape.provenance.clone()
        },
        z_order: 0,
        placeholder: shape.placeholder.clone(),
        transform: shape.transform.unwrap_or_default(),
        group_transforms: shape.groups.clone(),
        fill: shape.fill.clone().unwrap_or(Fill::None),
        stroke: shape.stroke.clone(),
        custom_path: shape.custom_path.clone(),
        outer_shadow: shape.outer_shadow,
        text,
        text_style,
        text_frame,
        alternative_text: shape.alternative_text.clone(),
        hyperlink: shape.hyperlink_relationship_id.as_ref().and_then(|id| {
            context
                .graph
                .part(context.source_part)
                .relationships
                .iter()
                .find(|relationship| context.graph.relationship_id(relationship) == id)
                .and_then(|relationship| match &relationship.target {
                    RelationshipTarget::External(target) => Some(target.clone()),
                    RelationshipTarget::Internal(part) => Some(
                        context
                            .graph
                            .part_name(context.graph.part(*part))
                            .to_owned(),
                    ),
                    RelationshipTarget::Missing(_) => None,
                })
        }),
        kind,
    }
}

fn resolve_text_frame(
    frame: &RawTextFrame,
    inherited: &PartialTextStyle,
    autofit_recompute: bool,
    slide_number: Option<u32>,
) -> ResolvedTextFrame {
    let base = inherited.resolve();
    let mut numbering = [0_u32; 9];
    ResolvedTextFrame {
        paragraphs: frame
            .paragraphs
            .iter()
            .map(|paragraph| {
                let paragraph_style = paragraph.style.clone();
                let resolved_paragraph = paragraph_style.resolve();
                let inherited_auto_number = paragraph
                    .style
                    .auto_number_scheme
                    .as_ref()
                    .and_then(|scheme| scheme.as_deref());
                let auto_number_scheme = paragraph
                    .auto_number_scheme
                    .as_deref()
                    .or(inherited_auto_number);
                let bullet = if let Some(scheme) = auto_number_scheme {
                    let level = paragraph.level as usize;
                    let next = if numbering[level] == 0 {
                        if paragraph.auto_number_scheme.is_some() {
                            paragraph.auto_number_start.max(1)
                        } else {
                            paragraph.style.auto_number_start.unwrap_or(1).max(1)
                        }
                    } else {
                        numbering[level].saturating_add(1)
                    };
                    numbering[level] = next;
                    for nested in &mut numbering[level + 1..] {
                        *nested = 0;
                    }
                    Some(format_auto_number(scheme, next))
                } else {
                    paragraph
                        .style
                        .bullet
                        .as_ref()
                        .and_then(|bullet| bullet.clone())
                };
                let runs = paragraph
                    .runs
                    .iter()
                    .map(|run| {
                        let mut style = paragraph_style.clone();
                        style.overlay(&run.style);
                        ResolvedTextRun {
                            text: if run.field_type.as_deref().is_some_and(|field_type| {
                                field_type.eq_ignore_ascii_case("slidenum")
                            }) {
                                slide_number
                                    .map_or_else(|| run.text.clone(), |number| number.to_string())
                            } else {
                                run.text.clone()
                            },
                            style: style.resolve(),
                            east_asian_font_family: run.east_asian_font_family.clone(),
                            complex_script_font_family: run.complex_script_font_family.clone(),
                        }
                    })
                    .collect::<Vec<_>>();
                let bullet_style = (bullet.is_some()
                    || paragraph.bullet_image_relationship_id.is_some())
                .then(|| {
                    let mut style = runs
                        .first()
                        .map(|run| run.style.clone())
                        .unwrap_or_else(|| resolved_paragraph.clone());
                    let bullet_font_family = paragraph
                        .style
                        .bullet_font_family
                        .as_ref()
                        .map(|value| value.as_ref())
                        .unwrap_or(paragraph.bullet_font_family.as_ref());
                    if let Some(family) = bullet_font_family {
                        style.font_family = Some(family.clone());
                    }
                    let bullet_color = paragraph
                        .style
                        .bullet_color
                        .as_ref()
                        .copied()
                        .unwrap_or(paragraph.bullet_color);
                    if let Some(color) = bullet_color {
                        style.color = color;
                        style.fill = Some(Fill::Solid(color));
                    }
                    let bullet_size = paragraph
                        .style
                        .bullet_size
                        .as_ref()
                        .copied()
                        .unwrap_or(paragraph.bullet_size);
                    if let Some(size) = bullet_size {
                        style.font_size = match size {
                            TextSpacing::Percent(value) => {
                                ((i64::from(style.font_size) * i64::from(value)) / 100_000)
                                    .clamp(100, 400_000) as i32
                            }
                            TextSpacing::Points(value) => value.clamp(100, 400_000),
                        };
                    }
                    style
                });
                ResolvedParagraph {
                    runs,
                    alignment: resolved_paragraph.alignment,
                    bullet,
                    bullet_image: None,
                    bullet_style,
                    level: paragraph.level,
                    margin_left: paragraph.margin_left.unwrap_or(0),
                    indent: paragraph.indent.unwrap_or(0),
                    line_spacing: paragraph.line_spacing,
                    space_before: paragraph.space_before,
                    space_after: paragraph.space_after,
                    direction: paragraph.direction,
                    tabs: paragraph.tabs.clone(),
                    font_alignment: paragraph.font_alignment,
                }
            })
            .collect(),
        vertical_alignment: base.vertical_alignment,
        margin_left: base.margin_left,
        margin_top: base.margin_top,
        margin_right: base.margin_right,
        margin_bottom: base.margin_bottom,
        wrap: frame.wrap,
        autofit: frame.autofit,
        autofit_font_scale: frame.autofit_font_scale,
        autofit_line_spacing_reduction: frame.autofit_line_spacing_reduction,
        autofit_recompute: autofit_recompute
            && (frame.autofit_font_scale.is_some()
                || frame.autofit_line_spacing_reduction.is_some()),
        flow: frame.flow,
        column_count: frame.column_count,
        column_spacing: frame.column_spacing,
        default_tab_size: frame.default_tab_size,
        warp: frame.warp.clone(),
    }
}

fn resolved_text_frame_plain_text(frame: &ResolvedTextFrame) -> String {
    frame
        .paragraphs
        .iter()
        .map(|paragraph| {
            paragraph
                .runs
                .iter()
                .map(|run| run.text.as_str())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn merge_shape(local: &RawShape, layout: Option<&RawShape>, master: Option<&RawShape>) -> RawShape {
    let fallback = |select: fn(&RawShape) -> Option<Transform>| {
        select(local)
            .or_else(|| layout.and_then(select))
            .or_else(|| master.and_then(select))
    };
    let mut text_style = local.text_style.clone();
    if let Some(layout) = layout {
        text_style.fill_missing_from(&layout.text_style);
    }
    if let Some(master) = master {
        text_style.fill_missing_from(&master.text_style);
    }
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
        custom_path: local
            .custom_path
            .clone()
            .or_else(|| layout.and_then(|shape| shape.custom_path.clone()))
            .or_else(|| master.and_then(|shape| shape.custom_path.clone())),
        outer_shadow: local
            .outer_shadow
            .or_else(|| layout.and_then(|shape| shape.outer_shadow))
            .or_else(|| master.and_then(|shape| shape.outer_shadow)),
        text: local
            .text
            .clone()
            .or_else(|| layout.and_then(|shape| shape.text.clone()))
            .or_else(|| master.and_then(|shape| shape.text.clone())),
        text_style,
        text_frame: local
            .text_frame
            .clone()
            .or_else(|| layout.and_then(|shape| shape.text_frame.clone()))
            .or_else(|| master.and_then(|shape| shape.text_frame.clone())),
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
        provenance: [
            (
                "transform",
                local.transform.is_some(),
                layout.is_some_and(|shape| shape.transform.is_some()),
                master.is_some_and(|shape| shape.transform.is_some()),
            ),
            (
                "geometry",
                local.geometry.is_some(),
                layout.is_some_and(|shape| shape.geometry.is_some()),
                master.is_some_and(|shape| shape.geometry.is_some()),
            ),
            (
                "fill",
                local.fill.is_some(),
                layout.is_some_and(|shape| shape.fill.is_some()),
                master.is_some_and(|shape| shape.fill.is_some()),
            ),
            (
                "stroke",
                local.stroke.is_some(),
                layout.is_some_and(|shape| shape.stroke.is_some()),
                master.is_some_and(|shape| shape.stroke.is_some()),
            ),
            (
                "text",
                local.text.is_some(),
                layout.is_some_and(|shape| shape.text.is_some()),
                master.is_some_and(|shape| shape.text.is_some()),
            ),
            (
                "text-style",
                local.text_style.has_values(),
                layout.is_some_and(|shape| shape.text_style.has_values()),
                master.is_some_and(|shape| shape.text_style.has_values()),
            ),
        ]
        .into_iter()
        .filter_map(|(property, local_value, layout_value, master_value)| {
            let source = if local_value {
                Some(SourceLevel::Slide)
            } else if layout_value {
                Some(SourceLevel::Layout)
            } else if master_value {
                Some(SourceLevel::Master)
            } else {
                None
            }?;
            Some(PropertyProvenance { property, source })
        })
        .collect(),
    }
}

fn master_text_style_for<'a>(
    styles: &'a MasterTextStyles,
    placeholder: Option<&Placeholder>,
) -> &'a TextStyleLevels {
    match placeholder.map(|placeholder| placeholder.kind.as_str()) {
        Some("title" | "ctrTitle") => &styles.title,
        Some("body" | "obj" | "subTitle") | None => &styles.body,
        _ => &styles.other,
    }
}

fn apply_master_text_styles(shape: &mut RawShape, styles: &TextStyleLevels) {
    shape.text_style.fill_missing_from(&styles.levels[0]);
    if let Some(frame) = &mut shape.text_frame {
        for paragraph in &mut frame.paragraphs {
            paragraph
                .style
                .fill_missing_from(&styles.levels[paragraph.level as usize]);
        }
    }
}

fn apply_paragraph_markers(text: &str, bullet: Option<&str>) -> String {
    let Some(bullet) = bullet else {
        return text.to_owned();
    };
    text.split('\n')
        .map(|paragraph| {
            if paragraph.is_empty() {
                String::new()
            } else {
                format!("{bullet} {paragraph}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
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
        text_styles: parse_master_text_styles(document, theme),
        ..ParsedPart::default()
    };
    for token in document.tokens() {
        let TokenKind::Start {
            name, attributes, ..
        } = &token.kind
        else {
            continue;
        };
        if matches!(name.local.as_str(), "sld" | "sldLayout") {
            parsed.show_master_shapes = plain(attributes, "showMasterSp").map(ooxml_bool);
        }
        if name.local == "hf" {
            parsed.show_header = plain(attributes, "hdr").map(ooxml_bool);
            parsed.show_footer = plain(attributes, "ftr").map(ooxml_bool);
            parsed.show_date_time = plain(attributes, "dt").map(ooxml_bool);
            parsed.show_slide_number = plain(attributes, "sldNum").map(ooxml_bool);
        }
    }
    let mut groups = Vec::<(usize, GroupTransform)>::new();
    let mut alternate_content_end = None;
    for (index, token) in document.tokens().iter().enumerate() {
        if alternate_content_end.is_some_and(|end| index <= end) {
            continue;
        }
        match &token.kind {
            TokenKind::Start { name, .. }
                if name.local == "AlternateContent"
                    && name.namespace.is_some_and(|namespace| {
                        document.namespace(namespace) == MARKUP_COMPATIBILITY
                    }) =>
            {
                if let Some(end) = element_end(document, index) {
                    if let Some(shape) = parse_alternate_content(
                        document,
                        index,
                        end,
                        groups.iter().map(|(_, transform)| *transform).collect(),
                        theme,
                        &mut parsed.diagnostics,
                    ) {
                        parsed.shapes.push(shape);
                        alternate_content_end = Some(end);
                    }
                }
            }
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

fn parse_alternate_content(
    document: &XmlDocument,
    start: usize,
    end: usize,
    groups: Vec<GroupTransform>,
    theme: &Theme,
    diagnostics: &mut Vec<(ResolveDiagnosticCode, Option<u32>, String)>,
) -> Option<RawShape> {
    let depth = document.tokens()[start].depth + 1;
    let mut choice = None;
    let mut fallback = None;
    for index in start + 1..=end {
        let token = &document.tokens()[index];
        let TokenKind::Start { name, .. } = &token.kind else {
            continue;
        };
        if token.depth != depth
            || name
                .namespace
                .is_none_or(|namespace| document.namespace(namespace) != MARKUP_COMPATIBILITY)
        {
            continue;
        }
        match name.local.as_str() {
            "Choice" if choice.is_none() => {
                choice = element_end(document, index).map(|end| (index, end))
            }
            "Fallback" if fallback.is_none() => {
                fallback = element_end(document, index).map(|end| (index, end));
            }
            _ => {}
        }
    }

    let (choice_start, choice_end) = choice?;
    let graphic_frames = child_elements(document, choice_start, choice_end, "graphicFrame");
    let mut smartart_candidates = graphic_frames
        .iter()
        .filter_map(|(start, end)| {
            let mut candidate_diagnostics = Vec::new();
            let shape = parse_graphic_frame(
                document,
                *start,
                *end,
                groups.clone(),
                theme,
                &mut candidate_diagnostics,
            );
            (shape.preserved_graphic == Some(PreservedFeature::SmartArt))
                .then_some((shape, candidate_diagnostics))
        })
        .collect::<Vec<_>>();
    if smartart_candidates.is_empty() {
        return None;
    }
    let (smartart, smartart_diagnostics) = smartart_candidates.remove(0);
    if graphic_frames.len() != 1 || !smartart_candidates.is_empty() {
        diagnostics.extend(smartart_diagnostics);
        return Some(smartart);
    }

    let Some((fallback_start, fallback_end)) = fallback else {
        diagnostics.extend(smartart_diagnostics);
        return Some(smartart);
    };
    let pictures = child_elements(document, fallback_start, fallback_end, "pic");
    if pictures.len() != 1 {
        diagnostics.extend(smartart_diagnostics);
        return Some(smartart);
    }
    let mut fallback_diagnostics = Vec::new();
    let mut picture = parse_shape(
        document,
        pictures[0].0,
        pictures[0].1,
        groups,
        theme,
        &mut fallback_diagnostics,
    );
    if picture.image_relationship_id.is_none() {
        diagnostics.extend(smartart_diagnostics);
        return Some(smartart);
    }
    picture.id = smartart.id;
    picture.name = smartart.name;
    picture.alternative_text = smartart.alternative_text.or(picture.alternative_text);
    picture.preserved_graphic = None;
    diagnostics.extend(fallback_diagnostics);
    Some(picture)
}

fn child_elements(
    document: &XmlDocument,
    start: usize,
    end: usize,
    local: &str,
) -> Vec<(usize, usize)> {
    let child_depth = document.tokens()[start].depth + 1;
    (start + 1..end)
        .filter_map(|index| {
            let token = &document.tokens()[index];
            matches!(
                &token.kind,
                TokenKind::Start { name, .. }
                    if name.local == local && token.depth == child_depth
            )
            .then(|| element_end(document, index).map(|end| (index, end)))
            .flatten()
        })
        .collect()
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
                    "SmartArt data is preserved but no provably associated fallback image is available"
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
    let mut first_row = false;
    let mut first_column = false;
    let mut banded_rows = false;
    let mut banded_columns = false;
    if let Some(properties) = direct_child_element(document, start, end, "tblPr") {
        if let TokenKind::Start { attributes, .. } = &document.tokens()[properties].kind {
            first_row = plain(attributes, "firstRow").is_some_and(ooxml_bool);
            first_column = plain(attributes, "firstCol").is_some_and(ooxml_bool);
            banded_rows = plain(attributes, "bandRow").is_some_and(ooxml_bool);
            banded_columns = plain(attributes, "bandCol").is_some_and(ooxml_bool);
        }
    }
    if let Some(grid) = direct_child_element(document, start, end, "tblGrid") {
        let grid_end = element_end(document, grid).unwrap_or(grid).min(end);
        let column_depth = document.tokens()[grid].depth + 1;
        for index in grid + 1..grid_end {
            let TokenKind::Start {
                name, attributes, ..
            } = &document.tokens()[index].kind
            else {
                continue;
            };
            if name.local == "gridCol" && document.tokens()[index].depth == column_depth {
                column_widths.push(plain_i64(attributes, "w").unwrap_or(0));
            }
        }
    }
    let row_depth = document.tokens()[start].depth + 1;
    for index in start + 1..end {
        let TokenKind::Start {
            name, attributes, ..
        } = &document.tokens()[index].kind
        else {
            continue;
        };
        if name.local == "tr" && document.tokens()[index].depth == row_depth {
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
                let direct_fill =
                    find_direct_table_cell_fill(document, cell_index, cell_end, theme);
                let row_number = rows.len();
                let column_number = cells.len();
                let fill = direct_fill.unwrap_or_else(|| {
                    let accent = theme.colors.get("accent1").copied().unwrap_or(WHITE);
                    if (first_row && row_number == 0) || (first_column && column_number == 0) {
                        accent
                    } else if (banded_rows && row_number % 2 == 1)
                        || (banded_columns && column_number % 2 == 1)
                    {
                        apply_color_transform(accent, "tint", 85_000)
                    } else {
                        WHITE
                    }
                });
                cells.push(ResolvedTableCell {
                    text,
                    text_frame: parse_text_frame(document, cell_index, cell_end, theme).map(
                        |frame| {
                            resolve_text_frame(&frame, &PartialTextStyle::default(), false, None)
                        },
                    ),
                    row_span: plain_u32(cell_attributes, "rowSpan").unwrap_or(1),
                    column_span: plain_u32(cell_attributes, "gridSpan").unwrap_or(1),
                    horizontal_merge: plain(cell_attributes, "hMerge").is_some_and(ooxml_bool),
                    vertical_merge: plain(cell_attributes, "vMerge").is_some_and(ooxml_bool),
                    fill,
                    borders: parse_table_cell_borders(document, cell_index, cell_end, theme),
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
        first_row,
        first_column,
        banded_rows,
        banded_columns,
    }
}

fn find_direct_table_cell_fill(
    document: &XmlDocument,
    start: usize,
    end: usize,
    theme: &Theme,
) -> Option<RgbaColor> {
    let properties = direct_child_element(document, start, end, "tcPr")?;
    let properties_end = element_end(document, properties)
        .unwrap_or(properties)
        .min(end);
    let fill_depth = document.tokens()[properties].depth + 1;
    (properties + 1..properties_end).find_map(|fill_index| {
        matches!(
            &document.tokens()[fill_index].kind,
            TokenKind::Start { name, .. }
                if name.local == "solidFill"
                    && document.tokens()[fill_index].depth == fill_depth
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
}

fn parse_table_cell_borders(
    document: &XmlDocument,
    start: usize,
    end: usize,
    theme: &Theme,
) -> TableCellBorders {
    let mut borders = TableCellBorders::default();
    let Some(properties) = direct_child_element(document, start, end, "tcPr") else {
        return borders;
    };
    let properties_end = element_end(document, properties)
        .unwrap_or(properties)
        .min(end);
    let border_depth = document.tokens()[properties].depth + 1;
    for index in properties + 1..properties_end {
        let TokenKind::Start {
            name, attributes, ..
        } = &document.tokens()[index].kind
        else {
            continue;
        };
        if document.tokens()[index].depth != border_depth {
            continue;
        }
        let target = match name.local.as_str() {
            "lnL" => &mut borders.left,
            "lnR" => &mut borders.right,
            "lnT" => &mut borders.top,
            "lnB" => &mut borders.bottom,
            _ => continue,
        };
        let border_end = element_end(document, index)
            .unwrap_or(index)
            .min(properties_end);
        let property_depth = document.tokens()[index].depth + 1;
        let mut no_fill = false;
        let mut color = None;
        let mut dash = None;
        for property_index in index + 1..border_end {
            let property_token = &document.tokens()[property_index];
            if property_token.depth != property_depth {
                continue;
            }
            let TokenKind::Start {
                name, attributes, ..
            } = &property_token.kind
            else {
                continue;
            };
            match name.local.as_str() {
                "noFill" => no_fill = true,
                "solidFill" | "gradFill" | "pattFill" if color.is_none() => {
                    let fill_end = element_end(document, property_index)
                        .unwrap_or(property_index)
                        .min(border_end);
                    color = parse_color(document, property_index, fill_end, theme);
                }
                "prstDash" if dash.is_none() => {
                    dash = plain(attributes, "val").map(str::to_owned);
                }
                _ => {}
            }
        }
        if no_fill {
            *target = None;
            continue;
        }
        *target = Some(Stroke {
            color: color.unwrap_or(BLACK),
            width: plain_i64(attributes, "w").unwrap_or(9_525),
            dash,
            head_end: None,
            tail_end: None,
        });
    }
    borders
}

fn parse_chart(document: &XmlDocument) -> ResolvedChart {
    let mut kinds = Vec::new();
    let mut chart_ranges = Vec::new();
    for index in 0..document.tokens().len() {
        let Some(kind) = chart_kind_at(document, index) else {
            continue;
        };
        chart_ranges.push((
            index,
            element_end(document, index).unwrap_or(index),
            document.tokens()[index].depth,
            kind,
        ));
        if !kinds.contains(&kind) {
            kinds.push(kind);
        }
    }
    let kind = match kinds.as_slice() {
        [] => ChartKind::Other,
        [kind] => *kind,
        _ => ChartKind::Combination,
    };
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
    let grouping = document
        .tokens()
        .iter()
        .find_map(|token| {
            let TokenKind::Start {
                name, attributes, ..
            } = &token.kind
            else {
                return None;
            };
            (name.local == "grouping").then(|| match plain(attributes, "val") {
                Some("stacked") => ChartGrouping::Stacked,
                Some("percentStacked") => ChartGrouping::PercentStacked,
                _ => ChartGrouping::Standard,
            })
        })
        .unwrap_or_default();
    let title = document
        .tokens()
        .iter()
        .enumerate()
        .find_map(|(index, token)| {
            let TokenKind::Start { name, .. } = &token.kind else {
                return None;
            };
            if name.local != "title" {
                return None;
            }
            let end = element_end(document, index)?;
            let text = collect_text(document, index, end);
            (!text.is_empty()).then_some(text)
        });
    let show_legend = document.tokens().iter().any(
        |token| matches!(&token.kind, TokenKind::Start { name, .. } if name.local == "legend"),
    );
    let mut series = Vec::new();
    for (index, token) in document.tokens().iter().enumerate() {
        let TokenKind::Start { name, .. } = &token.kind else {
            continue;
        };
        if name.local != "ser" {
            continue;
        }
        let Some(series_kind) = chart_ranges
            .iter()
            .find(|(start, end, depth, _)| {
                *start < index && index <= *end && *depth + 1 == token.depth
            })
            .map(|(_, _, _, kind)| *kind)
        else {
            continue;
        };
        let Some(end) = element_end(document, index) else {
            continue;
        };
        let name = child_cache_values(document, index, end, &["tx"])
            .into_iter()
            .next()
            .unwrap_or_else(|| format!("Series {}", series.len() + 1));
        let categories = child_cache_values(document, index, end, &["cat"]);
        let x_values = child_cache_values(document, index, end, &["xVal"])
            .into_iter()
            .filter_map(|value| value.parse::<f64>().ok())
            .collect();
        let values = child_cache_values(document, index, end, &["val", "yVal"])
            .into_iter()
            .filter_map(|value| value.parse::<f64>().ok())
            .collect();
        let bubble_sizes = child_cache_values(document, index, end, &["bubbleSize"])
            .into_iter()
            .filter_map(|value| value.parse::<f64>().ok())
            .collect();
        series.push(ChartSeries {
            kind: series_kind,
            name,
            categories,
            x_values,
            values,
            bubble_sizes,
            color: palette[series.len() % palette.len()],
        });
    }
    ResolvedChart {
        kind,
        grouping,
        series,
        title,
        show_legend,
        embedded_workbook: None,
    }
}

fn chart_kind_at(document: &XmlDocument, index: usize) -> Option<ChartKind> {
    let token = document.tokens().get(index)?;
    let TokenKind::Start { name, .. } = &token.kind else {
        return None;
    };
    Some(match name.local.as_str() {
        "lineChart" => ChartKind::Line,
        "pieChart" => ChartKind::Pie,
        "doughnutChart" => ChartKind::Doughnut,
        "areaChart" => ChartKind::Area,
        "scatterChart" => ChartKind::Scatter,
        "bubbleChart" => ChartKind::Bubble,
        "barChart" => {
            let end = element_end(document, index).unwrap_or(index);
            let bar_direction = document.tokens()[index..=end].iter().find_map(|candidate| {
                let TokenKind::Start {
                    name, attributes, ..
                } = &candidate.kind
                else {
                    return None;
                };
                (candidate.depth == token.depth + 1 && name.local == "barDir")
                    .then(|| plain(attributes, "val"))
                    .flatten()
            });
            if bar_direction == Some("bar") {
                ChartKind::Bar
            } else {
                ChartKind::Column
            }
        }
        _ => return None,
    })
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
    let mut paragraph_text = String::new();
    let mut paragraphs = Vec::new();
    let mut defaults = PartialTextStyle::default();
    let mut paragraph_style = PartialTextStyle::default();
    let mut run_style = PartialTextStyle::default();
    let mut saw_paragraph_style = false;
    let mut saw_run_style = false;
    for index in start..=end {
        let token = &document.tokens()[index];
        match &token.kind {
            TokenKind::Start {
                name,
                attributes,
                empty,
            } => {
                let direct_parent = stack.last().map(String::as_str);
                let inside_text_properties = stack
                    .iter()
                    .any(|local| matches!(local.as_str(), "rPr" | "defRPr" | "endParaRPr"));
                match name.local.as_str() {
                    "cNvPr" => {
                        shape.id = plain_u32(attributes, "id").unwrap_or(0);
                        shape.name = plain(attributes, "name").unwrap_or_default().to_owned();
                        shape.alternative_text = plain(attributes, "descr")
                            .or_else(|| plain(attributes, "title"))
                            .map(str::to_owned);
                    }
                    "hlinkClick" if direct_parent == Some("cNvPr") => {
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
                            let mut transform = parse_transform(document, index, xfrm_end);
                            const MAX_DIMENSION: i64 = 91_440_000;
                            let has_explicit_extent = document.tokens()[index..=xfrm_end]
                                .iter()
                                .any(|token| {
                                    matches!(
                                        &token.kind,
                                        TokenKind::Start { name, .. } if name.local == "ext"
                                    )
                                });
                            if has_explicit_extent
                                && (!(1..=MAX_DIMENSION).contains(&transform.bounds.size.width)
                                    || !(1..=MAX_DIMENSION)
                                        .contains(&transform.bounds.size.height))
                            {
                                diagnostics.push((
                                    ResolveDiagnosticCode::InvalidValue,
                                    Some(shape.id),
                                    "shape dimensions are outside the supported range and were clamped"
                                        .to_owned(),
                                ));
                                transform.bounds.size.width =
                                    transform.bounds.size.width.clamp(1, MAX_DIMENSION);
                                transform.bounds.size.height =
                                    transform.bounds.size.height.clamp(1, MAX_DIMENSION);
                            }
                            shape.transform = Some(transform);
                        }
                    }
                    "bodyPr" => {
                        defaults.overlay(&parse_body_properties(attributes));
                    }
                    "lvl1pPr" if stack.iter().any(|local| local == "lstStyle") => {
                        if let Some(style_end) = element_end(document, index) {
                            defaults.overlay(&parse_text_style_range(
                                document, index, style_end, theme,
                            ));
                        }
                    }
                    "pPr" if !saw_paragraph_style => {
                        if let Some(style_end) = element_end(document, index) {
                            paragraph_style.overlay(&parse_text_style_range(
                                document, index, style_end, theme,
                            ));
                        }
                        saw_paragraph_style = true;
                    }
                    "rPr"
                        if !saw_run_style
                            && stack
                                .iter()
                                .any(|local| matches!(local.as_str(), "r" | "fld")) =>
                    {
                        if let Some(style_end) = element_end(document, index) {
                            run_style.overlay(&parse_text_style_range(
                                document, index, style_end, theme,
                            ));
                        }
                        saw_run_style = true;
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
                    "custGeom" => {
                        let geometry_end = element_end(document, index).unwrap_or(index).min(end);
                        shape.custom_path = parse_custom_path(document, index, geometry_end);
                        if shape.custom_path.is_none() {
                            diagnostics.push((
                                ResolveDiagnosticCode::UnsupportedCustomGeometry,
                                Some(shape.id),
                                "custom geometry contains unsupported path commands".to_owned(),
                            ));
                        }
                    }
                    "solidFill" if direct_parent == Some("ln") => {
                        if let Some(fill_end) = element_end(document, index) {
                            let color =
                                parse_color(document, index, fill_end, theme).unwrap_or(BLACK);
                            let line = direct_parent_element(document, start, index, "ln");
                            let width = line
                                .and_then(|line| match &document.tokens()[line].kind {
                                    TokenKind::Start { attributes, .. } => {
                                        plain_i64(attributes, "w")
                                    }
                                    _ => None,
                                })
                                .unwrap_or(12_700);
                            let dash = line.and_then(|line| {
                                nearest_dash(
                                    document,
                                    line,
                                    element_end(document, line).unwrap_or(line).min(end),
                                )
                            });
                            shape.stroke = Some(Stroke {
                                color,
                                width,
                                dash,
                                head_end: None,
                                tail_end: None,
                            });
                        }
                    }
                    "solidFill" if direct_parent == Some("spPr") && shape.fill.is_none() => {
                        if let Some(fill_end) = element_end(document, index) {
                            let color =
                                parse_color(document, index, fill_end, theme).unwrap_or(BLACK);
                            shape.fill = Some(Fill::Solid(color));
                        }
                    }
                    "noFill" if direct_parent == Some("ln") => shape.stroke = None,
                    "noFill" if direct_parent == Some("spPr") => shape.fill = Some(Fill::None),
                    "gradFill" if direct_parent == Some("spPr") => {
                        let fill_end = element_end(document, index).unwrap_or(index).min(end);
                        shape.fill = parse_gradient_fill(document, index, fill_end, theme);
                        if shape.fill.is_none() {
                            diagnostics.push((
                                ResolveDiagnosticCode::UnsupportedFill,
                                Some(shape.id),
                                "non-linear or invalid gradient requires a renderer fallback"
                                    .to_owned(),
                            ));
                        }
                    }
                    "pattFill" if direct_parent == Some("spPr") => {
                        let fill_end = element_end(document, index).unwrap_or(index).min(end);
                        shape.fill = parse_pattern_fill(document, index, fill_end, theme);
                        if shape.fill.is_none() {
                            diagnostics.push((
                                ResolveDiagnosticCode::UnsupportedFill,
                                Some(shape.id),
                                "invalid pattern fill requires a renderer fallback".to_owned(),
                            ));
                        }
                    }
                    "headEnd" if direct_parent == Some("ln") => {
                        if let Some(stroke) = &mut shape.stroke {
                            stroke.head_end = plain(attributes, "type").and_then(line_end);
                        }
                    }
                    "tailEnd" if direct_parent == Some("ln") => {
                        if let Some(stroke) = &mut shape.stroke {
                            stroke.tail_end = plain(attributes, "type").and_then(line_end);
                        }
                    }
                    "blip" if direct_parent == Some("blipFill") => {
                        shape.image_relationship_id = attributes
                            .iter()
                            .find(|attribute| attribute.name.local == "embed")
                            .map(|attribute| attribute.value.clone());
                    }
                    "audioFile" | "videoFile" | "media" => diagnostics.push((
                        ResolveDiagnosticCode::UnsupportedActiveContent,
                        Some(shape.id),
                        "media is preserved and its poster may render, but playback is never activated"
                            .to_owned(),
                    )),
                    "srcRect" if direct_parent == Some("blipFill") => {
                        shape.crop = ImageCrop {
                            left: plain_i32(attributes, "l").unwrap_or(0),
                            top: plain_i32(attributes, "t").unwrap_or(0),
                            right: plain_i32(attributes, "r").unwrap_or(0),
                            bottom: plain_i32(attributes, "b").unwrap_or(0),
                        };
                    }
                    "t" => in_text = true,
                    "outerShdw" => {
                        let shadow_end = element_end(document, index).unwrap_or(index).min(end);
                        if !inside_text_properties {
                            shape.outer_shadow = parse_outer_shadow(document, index, shadow_end, theme);
                        }
                    }
                    "effectDag" => diagnostics.push((
                        ResolveDiagnosticCode::UnsupportedEffect,
                        Some(shape.id),
                        "effectDag is retained but not rendered".to_owned(),
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
                    paragraph_text.push_str(raw);
                } else if let Ok(decoded) = decode_entities(raw, token.range.start) {
                    paragraph_text.push_str(&decoded);
                }
            }
            TokenKind::End { name } => {
                if name.local == "t" {
                    in_text = false;
                }
                if name.local == "p" {
                    paragraphs.push(std::mem::take(&mut paragraph_text));
                }
                if stack.last().is_some_and(|local| *local == name.local) {
                    stack.pop();
                }
            }
            _ => {}
        }
    }
    if !paragraph_text.is_empty() {
        paragraphs.push(paragraph_text);
    }
    if !paragraphs.is_empty() {
        shape.text = Some(paragraphs.join("\n"));
    }
    defaults.overlay(&paragraph_style);
    defaults.overlay(&run_style);
    shape.text_style = defaults;
    shape.text_frame = parse_text_frame(document, start, end, theme);
    if let Some(preset) = shape
        .text_frame
        .as_ref()
        .and_then(|frame| frame.unsupported_warp.as_deref())
    {
        diagnostics.push((
            ResolveDiagnosticCode::UnsupportedEffect,
            Some(shape.id),
            format!("text warp preset {preset} is retained and rendered without warping"),
        ));
    }
    if shape
        .text_frame
        .as_ref()
        .is_some_and(|frame| frame.invalid_autofit_hint)
    {
        diagnostics.push((
            ResolveDiagnosticCode::InvalidValue,
            Some(shape.id),
            "normal AutoFit hints are malformed or outside the supported range and were clamped"
                .to_owned(),
        ));
    }
    shape
}

fn parse_gradient_fill(
    document: &XmlDocument,
    start: usize,
    end: usize,
    theme: &Theme,
) -> Option<Fill> {
    let mut angle = 0;
    let mut stops = Vec::new();
    for index in start..=end {
        let TokenKind::Start {
            name, attributes, ..
        } = &document.tokens()[index].kind
        else {
            continue;
        };
        if name.local == "lin" {
            angle = plain_i32(attributes, "ang").unwrap_or(0);
        } else if name.local == "gs" {
            let stop_end = element_end(document, index).unwrap_or(index).min(end);
            if let Some(color) = parse_color(document, index, stop_end, theme) {
                stops.push(GradientStop {
                    position: plain_i32(attributes, "pos").unwrap_or(0).clamp(0, 100_000),
                    color,
                });
            }
        }
    }
    stops.sort_unstable_by_key(|stop| stop.position);
    (stops.len() >= 2).then(|| {
        let radial = (start..=end).any(|index| {
            matches!(
                &document.tokens()[index].kind,
                TokenKind::Start { name, .. } if name.local == "path"
            )
        });
        if radial {
            Fill::RadialGradient { stops }
        } else {
            Fill::LinearGradient { angle, stops }
        }
    })
}

fn parse_pattern_fill(
    document: &XmlDocument,
    start: usize,
    end: usize,
    theme: &Theme,
) -> Option<Fill> {
    let TokenKind::Start { attributes, .. } = &document.tokens()[start].kind else {
        return None;
    };
    let color = |container: &str| {
        (start..=end).find_map(|index| {
            let TokenKind::Start { name, .. } = &document.tokens()[index].kind else {
                return None;
            };
            (name.local == container).then(|| {
                parse_color(
                    document,
                    index,
                    element_end(document, index).unwrap_or(index).min(end),
                    theme,
                )
            })?
        })
    };
    Some(Fill::Pattern {
        preset: plain(attributes, "prst").unwrap_or("pct5").to_owned(),
        foreground: color("fgClr").unwrap_or(BLACK),
        background: color("bgClr").unwrap_or(WHITE),
    })
}

fn parse_custom_path(document: &XmlDocument, start: usize, end: usize) -> Option<CustomPath> {
    let path_start = (start..=end).find(|index| {
        matches!(
            &document.tokens()[*index].kind,
            TokenKind::Start { name, .. } if name.local == "path"
        )
    })?;
    let TokenKind::Start { attributes, .. } = &document.tokens()[path_start].kind else {
        return None;
    };
    let size = EmuSize {
        width: plain_i64(attributes, "w").unwrap_or(1).max(1),
        height: plain_i64(attributes, "h").unwrap_or(1).max(1),
    };
    let path_end = element_end(document, path_start)
        .unwrap_or(path_start)
        .min(end);
    let mut commands = Vec::new();
    for index in path_start..=path_end {
        let TokenKind::Start { name, .. } = &document.tokens()[index].kind else {
            continue;
        };
        match name.local.as_str() {
            "moveTo" | "lnTo" => {
                let command_end = element_end(document, index).unwrap_or(index).min(path_end);
                let point = (index..=command_end).find_map(|candidate| {
                    let TokenKind::Start {
                        name, attributes, ..
                    } = &document.tokens()[candidate].kind
                    else {
                        return None;
                    };
                    (name.local == "pt").then(|| EmuPoint {
                        x: plain_i64(attributes, "x").unwrap_or(0),
                        y: plain_i64(attributes, "y").unwrap_or(0),
                    })
                });
                if let Some(point) = point {
                    commands.push(if name.local == "moveTo" {
                        PathCommand::MoveTo(point)
                    } else {
                        PathCommand::LineTo(point)
                    });
                }
            }
            "quadBezTo" | "cubicBezTo" => {
                let command_end = element_end(document, index).unwrap_or(index).min(path_end);
                let points = (index..=command_end)
                    .filter_map(|candidate| {
                        let TokenKind::Start {
                            name, attributes, ..
                        } = &document.tokens()[candidate].kind
                        else {
                            return None;
                        };
                        (name.local == "pt").then(|| EmuPoint {
                            x: plain_i64(attributes, "x").unwrap_or(0),
                            y: plain_i64(attributes, "y").unwrap_or(0),
                        })
                    })
                    .collect::<Vec<_>>();
                if name.local == "quadBezTo" && points.len() >= 2 {
                    commands.push(PathCommand::QuadraticTo {
                        control: points[0],
                        end: points[1],
                    });
                } else if name.local == "cubicBezTo" && points.len() >= 3 {
                    commands.push(PathCommand::CubicTo {
                        control1: points[0],
                        control2: points[1],
                        end: points[2],
                    });
                } else {
                    return None;
                }
            }
            "arcTo" => {
                let TokenKind::Start { attributes, .. } = &document.tokens()[index].kind else {
                    continue;
                };
                commands.push(PathCommand::ArcTo {
                    width_radius: plain_i64(attributes, "wR").unwrap_or(0).abs(),
                    height_radius: plain_i64(attributes, "hR").unwrap_or(0).abs(),
                    start_angle: plain_i32(attributes, "stAng").unwrap_or(0),
                    sweep_angle: plain_i32(attributes, "swAng").unwrap_or(0),
                });
            }
            "close" => commands.push(PathCommand::Close),
            _ => {}
        }
    }
    (!commands.is_empty()).then_some(CustomPath { size, commands })
}

fn parse_outer_shadow(
    document: &XmlDocument,
    start: usize,
    end: usize,
    theme: &Theme,
) -> Option<OuterShadow> {
    let TokenKind::Start { attributes, .. } = &document.tokens()[start].kind else {
        return None;
    };
    Some(OuterShadow {
        color: parse_color(document, start, end, theme)?,
        blur_radius: plain_i64(attributes, "blurRad").unwrap_or(0),
        distance: plain_i64(attributes, "dist").unwrap_or(0),
        direction: plain_i32(attributes, "dir").unwrap_or(0),
    })
}

fn line_end(value: &str) -> Option<LineEnd> {
    match value {
        "triangle" => Some(LineEnd::Triangle),
        "stealth" => Some(LineEnd::Stealth),
        "diamond" => Some(LineEnd::Diamond),
        "oval" => Some(LineEnd::Oval),
        "arrow" => Some(LineEnd::Arrow),
        _ => None,
    }
}

fn parse_text_frame(
    document: &XmlDocument,
    start: usize,
    end: usize,
    theme: &Theme,
) -> Option<RawTextFrame> {
    let body = (start..=end).find(|index| {
        matches!(
            &document.tokens()[*index].kind,
            TokenKind::Start { name, .. } if name.local == "txBody"
        )
    })?;
    let body_end = element_end(document, body).unwrap_or(end).min(end);
    let mut wrap = true;
    let mut autofit = TextAutofit::None;
    let mut autofit_font_scale = None;
    let mut autofit_line_spacing_reduction = None;
    let mut flow = TextFlow::Horizontal;
    let mut column_count = 1;
    let mut column_spacing = 0;
    let mut default_tab_size = 457_200;
    let mut warp = None;
    let mut unsupported_warp = None;
    let mut invalid_autofit_hint = false;
    let mut local_list_styles = TextStyleLevels::default();
    for index in body..=body_end {
        let TokenKind::Start {
            name, attributes, ..
        } = &document.tokens()[index].kind
        else {
            continue;
        };
        match name.local.as_str() {
            "bodyPr" => {
                wrap = plain(attributes, "wrap") != Some("none");
                flow = match plain(attributes, "vert") {
                    Some("vert" | "wordArtVert" | "eaVert") => TextFlow::Vertical,
                    Some("vert270" | "wordArtVertRtl") => TextFlow::Vertical270,
                    _ => TextFlow::Horizontal,
                };
                column_count = plain_u32(attributes, "numCol").unwrap_or(1).clamp(1, 16) as u8;
                column_spacing = plain_i64(attributes, "spcCol").unwrap_or(0).max(0);
                default_tab_size = plain_i64(attributes, "defTabSz")
                    .unwrap_or(457_200)
                    .clamp(1, 91_440_000);
            }
            "normAutofit" => {
                autofit = TextAutofit::ShrinkText;
                let font_scale = plain_percentage(attributes, "fontScale");
                let line_spacing_reduction = plain_percentage(attributes, "lnSpcReduction");
                invalid_autofit_hint |= plain(attributes, "fontScale").is_some()
                    && font_scale.is_none_or(|value| !(1_000..=100_000).contains(&value));
                invalid_autofit_hint |= plain(attributes, "lnSpcReduction").is_some()
                    && line_spacing_reduction.is_none_or(|value| !(0..=100_000).contains(&value));
                autofit_font_scale = font_scale.map(|value| value.clamp(1_000, 100_000));
                autofit_line_spacing_reduction =
                    line_spacing_reduction.map(|value| value.clamp(0, 100_000));
            }
            "spAutoFit" => autofit = TextAutofit::ResizeShape,
            "prstTxWarp" => {
                let preset = plain(attributes, "prst").unwrap_or_default();
                if matches!(
                    preset,
                    "textArchUp"
                        | "textArchDown"
                        | "textArchUpPour"
                        | "textArchDownPour"
                        | "textWave1"
                        | "textWave2"
                        | "textInflate"
                        | "textDeflate"
                        // Decode aliases emitted by pre-v7 development snapshots.
                        | "archUp"
                        | "archDown"
                        | "archUpPour"
                        | "archDownPour"
                        | "wave1"
                        | "wave2"
                        | "inflate"
                        | "deflate"
                ) {
                    let warp_end = element_end(document, index).unwrap_or(index).min(body_end);
                    let adjustment = (index..=warp_end)
                        .find_map(|candidate| {
                            let TokenKind::Start {
                                name, attributes, ..
                            } = &document.tokens()[candidate].kind
                            else {
                                return None;
                            };
                            if name.local != "gd" {
                                return None;
                            }
                            plain(attributes, "fmla")?
                                .strip_prefix("val ")?
                                .parse::<i32>()
                                .ok()
                        })
                        .unwrap_or(25_000)
                        .clamp(0, 100_000);
                    warp = Some(TextWarp {
                        preset: preset.to_owned(),
                        adjustment,
                    });
                } else if !preset.is_empty() {
                    unsupported_warp = Some(preset.to_owned());
                }
            }
            "noAutofit" => autofit = TextAutofit::None,
            _ => {}
        }
    }
    if let Some(list_start) = (body..=body_end).find(|index| {
        matches!(
            &document.tokens()[*index].kind,
            TokenKind::Start { name, .. } if name.local == "lstStyle"
        )
    }) {
        let list_end = element_end(document, list_start)
            .unwrap_or(list_start)
            .min(body_end);
        for level in 0..9 {
            let local = format!("lvl{}pPr", level + 1);
            if let Some(level_start) = (list_start..=list_end).find(|candidate| {
                matches!(
                    &document.tokens()[*candidate].kind,
                    TokenKind::Start { name, .. } if name.local == local
                )
            }) {
                let level_end = element_end(document, level_start)
                    .unwrap_or(level_start)
                    .min(list_end);
                local_list_styles.levels[level] =
                    parse_text_style_range(document, level_start, level_end, theme);
            } else if level > 0 {
                local_list_styles.levels[level] = local_list_styles.levels[level - 1].clone();
            }
        }
    }
    let mut paragraphs = Vec::new();
    let mut index = body + 1;
    while index <= body_end {
        let is_paragraph = matches!(
            &document.tokens()[index].kind,
            TokenKind::Start { name, .. } if name.local == "p"
        );
        if !is_paragraph {
            index += 1;
            continue;
        }
        let paragraph_end = element_end(document, index).unwrap_or(index).min(body_end);
        let mut paragraph = parse_rich_paragraph(document, index, paragraph_end, theme);
        let mut effective_style = local_list_styles.levels[paragraph.level as usize].clone();
        effective_style.overlay(&paragraph.style);
        paragraph.style = effective_style;
        paragraphs.push(paragraph);
        index = paragraph_end + 1;
    }
    (!paragraphs.is_empty()).then_some(RawTextFrame {
        paragraphs,
        wrap,
        autofit,
        autofit_font_scale,
        autofit_line_spacing_reduction,
        flow,
        column_count,
        column_spacing,
        default_tab_size,
        warp,
        unsupported_warp,
        invalid_autofit_hint,
    })
}

fn parse_rich_paragraph(
    document: &XmlDocument,
    start: usize,
    end: usize,
    theme: &Theme,
) -> RawParagraph {
    let mut paragraph = RawParagraph::default();
    let mut paragraph_mark_style = None;
    for index in start..=end {
        let TokenKind::Start {
            name, attributes, ..
        } = &document.tokens()[index].kind
        else {
            continue;
        };
        if name.local == "pPr" {
            let property_end = element_end(document, index).unwrap_or(index).min(end);
            paragraph.style = parse_text_style_range(document, index, property_end, theme);
            paragraph.level = plain_u32(attributes, "lvl").unwrap_or(0).min(8) as u8;
            paragraph.margin_left = plain_i64(attributes, "marL");
            paragraph.indent = plain_i64(attributes, "indent");
            paragraph.line_spacing = spacing_value(document, index, property_end, "lnSpc");
            paragraph.space_before = spacing_value(document, index, property_end, "spcBef");
            paragraph.space_after = spacing_value(document, index, property_end, "spcAft");
            paragraph.direction = if plain(attributes, "rtl").is_some_and(ooxml_bool) {
                TextDirection::RightToLeft
            } else {
                TextDirection::LeftToRight
            };
            paragraph.tabs = parse_text_tabs(document, index, property_end);
            paragraph.font_alignment = match plain(attributes, "fontAlgn") {
                Some("t") => TextFontAlignment::Top,
                Some("ctr") => TextFontAlignment::Center,
                Some("base") => TextFontAlignment::Baseline,
                Some("b") => TextFontAlignment::Bottom,
                _ => TextFontAlignment::Automatic,
            };
            for candidate in index..=property_end {
                let TokenKind::Start {
                    name, attributes, ..
                } = &document.tokens()[candidate].kind
                else {
                    continue;
                };
                if name.local == "buAutoNum" {
                    paragraph.auto_number_scheme = plain(attributes, "type").map(str::to_owned);
                    paragraph.auto_number_start = plain_u32(attributes, "startAt")
                        .unwrap_or(1)
                        .clamp(1, 32_767);
                }
                if name.local == "blip" {
                    paragraph.bullet_image_relationship_id = attributes
                        .iter()
                        .find(|attribute| matches!(attribute.name.local.as_str(), "embed" | "link"))
                        .map(|attribute| attribute.value.clone());
                }
                if name.local == "buFont" {
                    paragraph.bullet_font_family = plain(attributes, "typeface")
                        .map(|family| resolve_theme_font(family, theme));
                }
                if name.local == "buSzPct" {
                    paragraph.bullet_size =
                        plain_percentage(attributes, "val").map(TextSpacing::Percent);
                }
                if name.local == "buSzPts" {
                    paragraph.bullet_size = plain_i32(attributes, "val").map(TextSpacing::Points);
                }
                if name.local == "buClr" {
                    let color_end = element_end(document, candidate)
                        .unwrap_or(candidate)
                        .min(property_end);
                    paragraph.bullet_color = parse_color(document, candidate, color_end, theme);
                }
            }
            break;
        }
    }
    for index in start..=end {
        let TokenKind::Start { name, .. } = &document.tokens()[index].kind else {
            continue;
        };
        if name.local == "endParaRPr" {
            let property_end = element_end(document, index).unwrap_or(index).min(end);
            paragraph_mark_style =
                Some(parse_text_style_range(document, index, property_end, theme));
            break;
        }
    }
    let mut index = start + 1;
    while index < end {
        let is_break = matches!(
            &document.tokens()[index].kind,
            TokenKind::Start { name, .. } if name.local == "br"
        );
        let run_end = match &document.tokens()[index].kind {
            TokenKind::Start { name, .. } if matches!(name.local.as_str(), "r" | "fld" | "br") => {
                element_end(document, index).unwrap_or(index).min(end)
            }
            _ => {
                index += 1;
                continue;
            }
        };
        let text = if is_break {
            "\n".to_owned()
        } else {
            collect_text(document, index, run_end)
        };
        if !text.is_empty() {
            let mut style = PartialTextStyle::default();
            let mut east_asian_font_family = None;
            let mut complex_script_font_family = None;
            for candidate in index..=run_end {
                let TokenKind::Start {
                    name, attributes, ..
                } = &document.tokens()[candidate].kind
                else {
                    continue;
                };
                if matches!(name.local.as_str(), "rPr" | "br") {
                    let style_end = element_end(document, candidate)
                        .unwrap_or(candidate)
                        .min(run_end);
                    style = parse_text_style_range(document, candidate, style_end, theme);
                } else if name.local == "ea" {
                    east_asian_font_family = plain(attributes, "typeface")
                        .map(|family| resolve_theme_font(family, theme));
                } else if name.local == "cs" {
                    complex_script_font_family = plain(attributes, "typeface")
                        .map(|family| resolve_theme_font(family, theme));
                }
            }
            paragraph.runs.push(RawTextRun {
                text,
                field_type: match &document.tokens()[index].kind {
                    TokenKind::Start {
                        name, attributes, ..
                    } if name.local == "fld" => plain(attributes, "type").map(str::to_owned),
                    _ => None,
                },
                style,
                east_asian_font_family,
                complex_script_font_family,
            });
        }
        index = run_end + 1;
    }
    if paragraph.runs.is_empty() {
        let text = collect_text(document, start, end);
        if !text.is_empty() {
            paragraph.runs.push(RawTextRun {
                text,
                ..RawTextRun::default()
            });
        }
    }
    // The paragraph mark participates in PowerPoint line metrics even when the
    // paragraph contains no visible run. Keeping it as a zero-length run also
    // preserves endParaRPr inheritance without inventing visible text.
    paragraph.runs.push(RawTextRun {
        text: String::new(),
        style: paragraph_mark_style.unwrap_or_default(),
        ..RawTextRun::default()
    });
    paragraph
}

fn format_auto_number(scheme: &str, value: u32) -> String {
    let alpha = |uppercase: bool| {
        let mut value = value.max(1);
        let mut output = String::new();
        while value > 0 {
            value -= 1;
            output.insert(
                0,
                char::from_u32((if uppercase { b'A' } else { b'a' }) as u32 + value % 26)
                    .unwrap_or('?'),
            );
            value /= 26;
        }
        output
    };
    let roman = |uppercase: bool| {
        let mut value = value.min(3_999);
        let mut output = String::new();
        for (number, digits) in [
            (1000, "M"),
            (900, "CM"),
            (500, "D"),
            (400, "CD"),
            (100, "C"),
            (90, "XC"),
            (50, "L"),
            (40, "XL"),
            (10, "X"),
            (9, "IX"),
            (5, "V"),
            (4, "IV"),
            (1, "I"),
        ] {
            while value >= number {
                output.push_str(digits);
                value -= number;
            }
        }
        if uppercase {
            output
        } else {
            output.to_lowercase()
        }
    };
    let body = if scheme.starts_with("alphaLc") {
        alpha(false)
    } else if scheme.starts_with("alphaUc") {
        alpha(true)
    } else if scheme.starts_with("romanLc") {
        roman(false)
    } else if scheme.starts_with("romanUc") {
        roman(true)
    } else if scheme.starts_with("ordinal") {
        let suffix = if (11..=13).contains(&(value % 100)) {
            "th"
        } else {
            match value % 10 {
                1 => "st",
                2 => "nd",
                3 => "rd",
                _ => "th",
            }
        };
        format!("{value}{suffix}")
    } else if scheme.starts_with("circleNum") && value <= 20 {
        char::from_u32(0x245f + value).unwrap_or('•').to_string()
    } else {
        value.to_string()
    };
    if scheme.ends_with("ParenBoth") {
        format!("({body})")
    } else if scheme.ends_with("ParenR") {
        format!("{body})")
    } else if scheme.ends_with("Period") {
        format!("{body}.")
    } else {
        body
    }
}

fn parse_text_tabs(document: &XmlDocument, start: usize, end: usize) -> Vec<ResolvedTextTab> {
    (start..=end)
        .filter_map(|index| {
            let TokenKind::Start {
                name, attributes, ..
            } = &document.tokens()[index].kind
            else {
                return None;
            };
            (name.local == "tab").then_some(ResolvedTextTab {
                position: plain_i64(attributes, "pos").unwrap_or(0),
                alignment: match plain(attributes, "algn") {
                    Some("ctr") => TextTabAlignment::Center,
                    Some("r") => TextTabAlignment::Right,
                    Some("dec") => TextTabAlignment::Decimal,
                    _ => TextTabAlignment::Left,
                },
            })
        })
        .collect()
}

fn spacing_value(
    document: &XmlDocument,
    start: usize,
    end: usize,
    container: &str,
) -> Option<TextSpacing> {
    let container_start = (start..=end).find(|index| {
        matches!(
            &document.tokens()[*index].kind,
            TokenKind::Start { name, .. } if name.local == container
        )
    })?;
    let container_end = element_end(document, container_start)
        .unwrap_or(container_start)
        .min(end);
    for token in &document.tokens()[container_start..=container_end] {
        let TokenKind::Start {
            name, attributes, ..
        } = &token.kind
        else {
            continue;
        };
        if name.local == "spcPct" {
            return plain_percentage(attributes, "val").map(TextSpacing::Percent);
        }
        if name.local == "spcPts" {
            return plain_i32(attributes, "val").map(TextSpacing::Points);
        }
    }
    None
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

fn parse_master_text_styles(document: &XmlDocument, theme: &Theme) -> MasterTextStyles {
    let mut styles = MasterTextStyles::default();
    for (index, token) in document.tokens().iter().enumerate() {
        let TokenKind::Start { name, .. } = &token.kind else {
            continue;
        };
        let target = match name.local.as_str() {
            "titleStyle" => &mut styles.title,
            "bodyStyle" => &mut styles.body,
            "otherStyle" => &mut styles.other,
            _ => continue,
        };
        let Some(style_end) = element_end(document, index) else {
            continue;
        };
        for level in 0..9 {
            let local = format!("lvl{}pPr", level + 1);
            let level_start = (index + 1..=style_end).find(|candidate| {
                matches!(
                    &document.tokens()[*candidate].kind,
                    TokenKind::Start { name, .. } if name.local == local
                )
            });
            if let Some(level_start) = level_start {
                let level_end = element_end(document, level_start).unwrap_or(level_start);
                target.levels[level].overlay(&parse_text_style_range(
                    document,
                    level_start,
                    level_end,
                    theme,
                ));
            } else if level > 0 {
                target.levels[level] = target.levels[level - 1].clone();
            }
        }
    }
    styles
}

fn parse_body_properties(attributes: &[Attribute]) -> PartialTextStyle {
    PartialTextStyle {
        vertical_alignment: plain(attributes, "anchor").and_then(text_vertical_alignment),
        margin_left: plain_i64(attributes, "lIns"),
        margin_top: plain_i64(attributes, "tIns"),
        margin_right: plain_i64(attributes, "rIns"),
        margin_bottom: plain_i64(attributes, "bIns"),
        ..PartialTextStyle::default()
    }
}

fn parse_text_style_range(
    document: &XmlDocument,
    start: usize,
    end: usize,
    theme: &Theme,
) -> PartialTextStyle {
    let mut style = PartialTextStyle::default();
    for index in start..=end {
        let TokenKind::Start {
            name, attributes, ..
        } = &document.tokens()[index].kind
        else {
            continue;
        };
        match name.local.as_str() {
            "bodyPr" => style.overlay(&parse_body_properties(attributes)),
            "pPr" | "lvl1pPr" | "lvl2pPr" | "lvl3pPr" | "lvl4pPr" | "lvl5pPr" | "lvl6pPr"
            | "lvl7pPr" | "lvl8pPr" | "lvl9pPr" => {
                style.alignment = plain(attributes, "algn").and_then(text_alignment);
            }
            "defRPr" | "rPr" | "endParaRPr" => {
                if let Some(value) = plain_i32(attributes, "sz") {
                    style.font_size = Some(value);
                }
                if let Some(value) = plain(attributes, "b") {
                    style.bold = Some(ooxml_bool(value));
                }
                if let Some(value) = plain(attributes, "i") {
                    style.italic = Some(ooxml_bool(value));
                }
                if let Some(value) = plain(attributes, "u") {
                    style.underline = Some(value != "none");
                }
                if let Some(value) = plain(attributes, "strike") {
                    style.strike = Some(!matches!(value, "noStrike" | "none"));
                }
                if let Some(value) = plain_i32(attributes, "spc") {
                    style.character_spacing = Some(value);
                }
                if let Some(value) = plain_i32(attributes, "baseline") {
                    style.baseline = Some(value.clamp(-100_000, 100_000));
                }
            }
            "latin" if style.font_family.is_none() => {
                style.font_family =
                    plain(attributes, "typeface").map(|family| resolve_theme_font(family, theme));
            }
            "solidFill"
                if style.color.is_none() && is_direct_text_paint(document, start, index) =>
            {
                let fill_end = element_end(document, index).unwrap_or(index);
                style.color = parse_color(document, index, fill_end, theme);
                style.text_fill = style.color.map(Fill::Solid);
            }
            "gradFill" if is_direct_text_paint(document, start, index) => {
                let fill_end = element_end(document, index).unwrap_or(index).min(end);
                style.text_fill = parse_gradient_fill(document, index, fill_end, theme);
            }
            "pattFill" if is_direct_text_paint(document, start, index) => {
                let fill_end = element_end(document, index).unwrap_or(index).min(end);
                style.text_fill = parse_pattern_fill(document, index, fill_end, theme);
            }
            "noFill" if is_direct_text_paint(document, start, index) => {
                style.text_fill = Some(Fill::None)
            }
            "ln" => {
                let line_end = element_end(document, index).unwrap_or(index).min(end);
                let no_fill = document.tokens()[index..=line_end].iter().any(|token| {
                    matches!(&token.kind, TokenKind::Start { name, .. } if name.local == "noFill")
                });
                style.outline = Some((!no_fill).then(|| Stroke {
                    color: parse_color(document, index, line_end, theme).unwrap_or(BLACK),
                    width: plain_i64(attributes, "w").unwrap_or(9_525),
                    dash: nearest_dash(document, index, line_end),
                    head_end: None,
                    tail_end: None,
                }));
            }
            "outerShdw" => {
                let shadow_end = element_end(document, index).unwrap_or(index).min(end);
                style.shadow = Some(parse_outer_shadow(document, index, shadow_end, theme));
            }
            "innerShdw" => {
                let shadow_end = element_end(document, index).unwrap_or(index).min(end);
                style.inner_shadow = Some(parse_outer_shadow(document, index, shadow_end, theme));
            }
            "glow" => {
                let glow_end = element_end(document, index).unwrap_or(index).min(end);
                style.glow = Some(parse_color(document, index, glow_end, theme).map(|color| {
                    TextGlow {
                        color,
                        radius: plain_i64(attributes, "rad")
                            .unwrap_or(0)
                            .clamp(0, 9_144_000),
                    }
                }));
            }
            "blur" => {
                style.blur_radius = Some(
                    plain_i64(attributes, "rad")
                        .unwrap_or(0)
                        .clamp(0, 9_144_000),
                );
            }
            "softEdge" => {
                style.soft_edge_radius = Some(
                    plain_i64(attributes, "rad")
                        .unwrap_or(0)
                        .clamp(0, 9_144_000),
                );
            }
            "reflection" => style.reflection = Some(true),
            "buChar" => {
                style.bullet = Some(Some(plain(attributes, "char").unwrap_or("•").to_owned()));
                style.auto_number_scheme = Some(None);
            }
            "buAutoNum" => {
                let start = plain_u32(attributes, "startAt").unwrap_or(1);
                style.bullet = Some(Some(format!("{start}.")));
                style.auto_number_scheme = Some(plain(attributes, "type").map(str::to_owned));
                style.auto_number_start = Some(start.clamp(1, 32_767));
            }
            "buBlip" => {
                style.bullet = Some(Some("◼".to_owned()));
                style.auto_number_scheme = Some(None);
            }
            "buNone" => {
                style.bullet = Some(None);
                style.auto_number_scheme = Some(None);
            }
            "buFont" => {
                style.bullet_font_family = Some(
                    plain(attributes, "typeface").map(|family| resolve_theme_font(family, theme)),
                );
            }
            "buFontTx" => style.bullet_font_family = Some(None),
            "buClr" => {
                let color_end = element_end(document, index).unwrap_or(index).min(end);
                style.bullet_color = Some(parse_color(document, index, color_end, theme));
            }
            "buClrTx" => style.bullet_color = Some(None),
            "buSzPct" => {
                style.bullet_size =
                    Some(plain_percentage(attributes, "val").map(TextSpacing::Percent));
            }
            "buSzPts" => {
                style.bullet_size = Some(plain_i32(attributes, "val").map(TextSpacing::Points));
            }
            "buSzTx" => style.bullet_size = Some(None),
            _ => {}
        }
    }
    style
}

fn is_direct_text_paint(document: &XmlDocument, start: usize, index: usize) -> bool {
    let depth = document.tokens()[index].depth;
    (start..index).rev().any(|candidate| {
        matches!(
            &document.tokens()[candidate].kind,
            TokenKind::Start { name, .. }
                if document.tokens()[candidate].depth + 1 == depth
                    && matches!(name.local.as_str(), "rPr" | "defRPr" | "endParaRPr")
                    && element_end(document, candidate).is_some_and(|end| end >= index)
        )
    })
}

fn resolve_theme_font(family: &str, theme: &Theme) -> String {
    match family {
        "+mj-lt" => theme.major_latin.clone(),
        "+mn-lt" => theme.minor_latin.clone(),
        "+mj-ea" => theme.major_east_asian.clone(),
        "+mn-ea" => theme.minor_east_asian.clone(),
        "+mj-cs" => theme.major_complex_script.clone(),
        "+mn-cs" => theme.minor_complex_script.clone(),
        _ => family.to_owned(),
    }
}

fn text_alignment(value: &str) -> Option<TextAlignment> {
    match value {
        "l" => Some(TextAlignment::Left),
        "ctr" => Some(TextAlignment::Center),
        "r" => Some(TextAlignment::Right),
        "just" | "justLow" => Some(TextAlignment::Justify),
        "dist" | "thaiDist" => Some(TextAlignment::Distributed),
        _ => None,
    }
}

fn plain_percentage(attributes: &[Attribute], local: &str) -> Option<i32> {
    let value = plain(attributes, local)?;
    if let Some(percent) = value.strip_suffix('%') {
        let (whole, fraction) = percent.split_once('.').unwrap_or((percent, ""));
        let whole = whole.parse::<i64>().ok()?;
        let mut fraction_digits = fraction.bytes().take(3).collect::<Vec<_>>();
        if !fraction_digits.iter().all(u8::is_ascii_digit) {
            return None;
        }
        while fraction_digits.len() < 3 {
            fraction_digits.push(b'0');
        }
        let fraction = std::str::from_utf8(&fraction_digits)
            .ok()?
            .parse::<i64>()
            .ok()?;
        return i32::try_from(whole.checked_mul(1_000)?.checked_add(fraction)?).ok();
    }
    value.parse().ok()
}

fn text_vertical_alignment(value: &str) -> Option<TextVerticalAlignment> {
    match value {
        "t" => Some(TextVerticalAlignment::Top),
        "ctr" => Some(TextVerticalAlignment::Center),
        "b" => Some(TextVerticalAlignment::Bottom),
        _ => None,
    }
}

fn ooxml_bool(value: &str) -> bool {
    matches!(value, "1" | "true" | "on")
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

fn direct_child_element(
    document: &XmlDocument,
    start: usize,
    end: usize,
    local: &str,
) -> Option<usize> {
    let child_depth = document.tokens().get(start)?.depth + 1;
    (start + 1..end).find(|index| {
        let token = &document.tokens()[*index];
        token.depth == child_depth
            && matches!(&token.kind, TokenKind::Start { name, .. } if name.local == local)
    })
}

fn direct_parent_element(
    document: &XmlDocument,
    start: usize,
    index: usize,
    local: &str,
) -> Option<usize> {
    let parent_depth = document.tokens().get(index)?.depth.checked_sub(1)?;
    (start..index).rev().find(|candidate| {
        let token = &document.tokens()[*candidate];
        token.depth == parent_depth
            && matches!(&token.kind, TokenKind::Start { name, .. } if name.local == local)
            && element_end(document, *candidate).is_some_and(|end| end >= index)
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
        "pentagon" => Some(PresetGeometry::Pentagon),
        "octagon" => Some(PresetGeometry::Octagon),
        "star5" => Some(PresetGeometry::Star5),
        "plus" => Some(PresetGeometry::Plus),
        "chevron" => Some(PresetGeometry::Chevron),
        "rightArrow" => Some(PresetGeometry::RightArrow),
        "leftArrow" => Some(PresetGeometry::LeftArrow),
        "upArrow" => Some(PresetGeometry::UpArrow),
        "downArrow" => Some(PresetGeometry::DownArrow),
        "trapezoid" => Some(PresetGeometry::Trapezoid),
        _ => None,
    }
}

fn nearest_dash(document: &XmlDocument, start: usize, end: usize) -> Option<String> {
    let property_depth = document.tokens()[start].depth + 1;
    document.tokens()[start..=end].iter().find_map(|token| {
        if token.depth != property_depth {
            return None;
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn smartart_alternate_content(fallback: &str) -> String {
        format!(
            r#"<p:sld xmlns:p="p" xmlns:a="a" xmlns:dgm="dgm" xmlns:r="r" xmlns:mc="{MARKUP_COMPATIBILITY}"><p:cSld><p:spTree><mc:AlternateContent><mc:Choice Requires="dgm"><p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id="8" name="Process" descr="Process diagram"/></p:nvGraphicFramePr><p:xfrm><a:off x="100" y="200"/><a:ext cx="300" cy="400"/></p:xfrm><a:graphic><a:graphicData><dgm:relIds r:dm="rDiagram"/></a:graphicData></a:graphic></p:graphicFrame></mc:Choice>{fallback}</mc:AlternateContent></p:spTree></p:cSld></p:sld>"#,
        )
    }

    #[test]
    fn smartart_uses_only_the_picture_in_its_alternate_content_fallback() {
        let source = smartart_alternate_content(
            r#"<mc:Fallback><p:pic><p:nvPicPr><p:cNvPr id="81" name="Preview"/></p:nvPicPr><p:blipFill><a:blip r:embed="rPreview"/><a:srcRect l="1000" t="2000" r="3000" b="4000"/></p:blipFill><p:spPr><a:xfrm><a:off x="500" y="600"/><a:ext cx="700" cy="800"/></a:xfrm></p:spPr></p:pic></mc:Fallback>"#,
        );
        let document = XmlDocument::parse(source.into_bytes()).unwrap();
        let parsed = parse_drawing_part(&document, &Theme::default());

        assert!(parsed.diagnostics.is_empty());
        assert_eq!(parsed.shapes.len(), 1);
        let shape = &parsed.shapes[0];
        assert_eq!(shape.id, 8);
        assert_eq!(shape.name, "Process");
        assert_eq!(shape.alternative_text.as_deref(), Some("Process diagram"));
        assert_eq!(shape.image_relationship_id.as_deref(), Some("rPreview"));
        assert_eq!(shape.preserved_graphic, None);
        assert_eq!(
            shape.transform.unwrap().bounds.origin,
            EmuPoint { x: 500, y: 600 }
        );
        assert_eq!(
            shape.transform.unwrap().bounds.size,
            EmuSize {
                width: 700,
                height: 800
            }
        );
        assert_eq!(
            shape.crop,
            ImageCrop {
                left: 1_000,
                top: 2_000,
                right: 3_000,
                bottom: 4_000,
            }
        );
    }

    #[test]
    fn smartart_without_one_provable_fallback_picture_stays_a_placeholder() {
        for fallback in [
            "",
            r#"<mc:Fallback><p:pic><p:nvPicPr><p:cNvPr id="81" name="No relationship"/></p:nvPicPr></p:pic></mc:Fallback>"#,
            r#"<mc:Fallback><p:pic><p:nvPicPr><p:cNvPr id="81" name="First"/></p:nvPicPr><p:blipFill><a:blip r:embed="rOne"/></p:blipFill></p:pic><p:pic><p:nvPicPr><p:cNvPr id="82" name="Second"/></p:nvPicPr><p:blipFill><a:blip r:embed="rTwo"/></p:blipFill></p:pic></mc:Fallback>"#,
            r#"<mc:Fallback><p:grpSp><p:nvGrpSpPr/><p:grpSpPr/><p:pic><p:nvPicPr><p:cNvPr id="81" name="Nested picture"/></p:nvPicPr><p:blipFill><a:blip r:embed="rNested"/></p:blipFill></p:pic></p:grpSp></mc:Fallback>"#,
        ] {
            let source = smartart_alternate_content(fallback);
            let document = XmlDocument::parse(source.into_bytes()).unwrap();
            let parsed = parse_drawing_part(&document, &Theme::default());

            assert_eq!(parsed.shapes.len(), 1);
            assert_eq!(
                parsed.shapes[0].preserved_graphic,
                Some(PreservedFeature::SmartArt)
            );
            assert_eq!(parsed.shapes[0].image_relationship_id, None);
            assert!(parsed.diagnostics.iter().any(|diagnostic| {
                diagnostic.0 == ResolveDiagnosticCode::UnsupportedSmartArt
                    && diagnostic.1 == Some(8)
            }));
        }
    }

    #[test]
    fn alternate_content_that_is_not_smartart_keeps_the_existing_parse_path() {
        let source = format!(
            r#"<p:sld xmlns:p="p" xmlns:a="a" xmlns:r="r" xmlns:mc="{MARKUP_COMPATIBILITY}"><p:cSld><p:spTree><mc:AlternateContent><mc:Choice><p:pic><p:nvPicPr><p:cNvPr id="1" name="Choice picture"/></p:nvPicPr><p:blipFill><a:blip r:embed="rChoice"/></p:blipFill></p:pic></mc:Choice><mc:Fallback><p:pic><p:nvPicPr><p:cNvPr id="2" name="Fallback picture"/></p:nvPicPr><p:blipFill><a:blip r:embed="rFallback"/></p:blipFill></p:pic></mc:Fallback></mc:AlternateContent></p:spTree></p:cSld></p:sld>"#,
        );
        let document = XmlDocument::parse(source.into_bytes()).unwrap();
        let parsed = parse_drawing_part(&document, &Theme::default());

        assert_eq!(parsed.shapes.len(), 2);
        assert_eq!(parsed.shapes[0].name, "Choice picture");
        assert_eq!(parsed.shapes[1].name, "Fallback picture");
    }

    #[test]
    fn ambiguous_smartart_choice_does_not_adopt_its_picture_fallback() {
        let source = smartart_alternate_content(
            r#"<mc:Fallback><p:pic><p:nvPicPr><p:cNvPr id="81" name="Preview"/></p:nvPicPr><p:blipFill><a:blip r:embed="rPreview"/></p:blipFill></p:pic></mc:Fallback>"#,
        )
        .replace(
            "</mc:Choice>",
            r#"<p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id="9" name="Other frame"/></p:nvGraphicFramePr></p:graphicFrame></mc:Choice>"#,
        );
        let document = XmlDocument::parse(source.into_bytes()).unwrap();
        let parsed = parse_drawing_part(&document, &Theme::default());

        assert_eq!(parsed.shapes.len(), 1);
        assert_eq!(
            parsed.shapes[0].preserved_graphic,
            Some(PreservedFeature::SmartArt)
        );
        assert_eq!(parsed.shapes[0].image_relationship_id, None);
        assert!(
            parsed
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.0 == ResolveDiagnosticCode::UnsupportedSmartArt)
        );
    }

    #[test]
    fn parses_every_supported_two_dimensional_chart_family() {
        let cases = [
            ("barChart", "<c:barDir val=\"col\"/>", ChartKind::Column),
            ("barChart", "<c:barDir val=\"bar\"/>", ChartKind::Bar),
            ("lineChart", "", ChartKind::Line),
            ("pieChart", "", ChartKind::Pie),
            ("doughnutChart", "", ChartKind::Doughnut),
            ("areaChart", "", ChartKind::Area),
            ("scatterChart", "", ChartKind::Scatter),
            ("bubbleChart", "", ChartKind::Bubble),
        ];
        for (element, properties, expected) in cases {
            let source = format!(
                r#"<c:chartSpace xmlns:c="c" xmlns:a="a"><c:chart><c:title><a:p><a:r><a:t>Revenue</a:t></a:r></a:p></c:title><c:legend/><c:plotArea><c:{element}>{properties}<c:grouping val="stacked"/><c:ser><c:tx><c:v>Actual</c:v></c:tx><c:xVal><c:numRef><c:numCache><c:pt idx="0"><c:v>1</c:v></c:pt></c:numCache></c:numRef></c:xVal><c:yVal><c:numRef><c:numCache><c:pt idx="0"><c:v>2</c:v></c:pt></c:numCache></c:numRef></c:yVal><c:bubbleSize><c:numRef><c:numCache><c:pt idx="0"><c:v>3</c:v></c:pt></c:numCache></c:numRef></c:bubbleSize></c:ser></c:{element}></c:plotArea></c:chart></c:chartSpace>"#,
            );
            let document = XmlDocument::parse(source.into_bytes()).unwrap();
            let chart = parse_chart(&document);
            assert_eq!(chart.kind, expected);
            assert_eq!(chart.grouping, ChartGrouping::Stacked);
            assert_eq!(chart.title.as_deref(), Some("Revenue"));
            assert!(chart.show_legend);
            assert_eq!(chart.series[0].x_values, [1.0]);
            assert_eq!(chart.series[0].values, [2.0]);
            assert_eq!(chart.series[0].bubble_sizes, [3.0]);
        }
    }

    #[test]
    fn leaves_three_dimensional_charts_explicitly_unsupported() {
        let document = XmlDocument::parse(
            br#"<c:chartSpace xmlns:c="c"><c:chart><c:plotArea><c:pie3DChart/></c:plotArea></c:chart></c:chartSpace>"#
                .as_slice(),
        )
        .unwrap();
        assert_eq!(parse_chart(&document).kind, ChartKind::Other);
    }

    #[test]
    fn recognizes_two_dimensional_combination_charts() {
        let document = XmlDocument::parse(
            br#"<c:chartSpace xmlns:c="c"><c:chart><c:plotArea><c:lineChart><c:ser><c:val><c:numLit><c:pt idx="0"><c:v>2</c:v></c:pt></c:numLit></c:val></c:ser></c:lineChart><c:barChart><c:barDir val="col"/><c:ser><c:val><c:numLit><c:pt idx="0"><c:v>10</c:v></c:pt></c:numLit></c:val></c:ser></c:barChart></c:plotArea></c:chart></c:chartSpace>"#.as_slice(),
        )
        .unwrap();
        let chart = parse_chart(&document);
        assert_eq!(chart.kind, ChartKind::Combination);
        assert_eq!(chart.series[0].kind, ChartKind::Line);
        assert_eq!(chart.series[1].kind, ChartKind::Column);
    }

    #[test]
    fn parses_table_formatting_merges_and_rich_text_without_leaking_text_color() {
        let source = br#"<a:tbl xmlns:a="a" xmlns:x="extension">
          <a:tblPr firstRow="1" firstCol="1" bandRow="1" bandCol="0"/>
          <a:tblGrid><a:gridCol w="100"/><a:gridCol w="200"/></a:tblGrid>
          <a:tr h="50">
            <a:tc gridSpan="2" rowSpan="2"><a:txBody><a:bodyPr/><a:p><a:r><a:rPr b="1"><a:solidFill><a:srgbClr val="FF0000"/></a:solidFill></a:rPr><a:t>Header</a:t></a:r></a:p></a:txBody><a:tcPr><a:lnB w="12700"><a:solidFill><a:srgbClr val="445566"/></a:solidFill><a:extLst><a:ext><x:noFill/><x:srgbClr val="ABCDEF"/><x:prstDash val="dot"/></a:ext></a:extLst></a:lnB><a:solidFill><a:srgbClr val="112233"/></a:solidFill></a:tcPr></a:tc>
            <a:tc hMerge="1"><a:txBody><a:bodyPr/><a:p/></a:txBody><a:tcPr><a:extLst><a:ext><x:solidFill><x:srgbClr val="000000"/></x:solidFill></a:ext></a:extLst></a:tcPr></a:tc>
          </a:tr>
          <a:tr h="60"><a:tc vMerge="1"><a:txBody><a:bodyPr/><a:p/></a:txBody><a:tcPr/></a:tc></a:tr>
          <a:extLst><a:ext><x:tblPr firstRow="0"/><x:gridCol w="999"/><x:tr h="999"><x:tc><x:tcPr><x:solidFill><x:srgbClr val="000000"/></x:solidFill></x:tcPr></x:tc></x:tr></a:ext></a:extLst>
        </a:tbl>"#;
        let document = XmlDocument::parse(source.as_slice()).unwrap();
        let table = parse_table(
            &document,
            0,
            element_end(&document, 0).unwrap(),
            &Theme::default(),
        );

        assert_eq!(table.column_widths, [100, 200]);
        assert_eq!(table.rows.len(), 2);
        assert!(table.first_row);
        assert!(table.first_column);
        assert!(table.banded_rows);
        assert!(!table.banded_columns);
        let header = &table.rows[0].cells[0];
        assert_eq!(header.column_span, 2);
        assert_eq!(header.row_span, 2);
        assert_eq!(header.fill, parse_hex_color("112233").unwrap());
        assert_eq!(header.borders.bottom.as_ref().unwrap().width, 12_700);
        assert_eq!(
            header.borders.bottom.as_ref().unwrap().color,
            parse_hex_color("445566").unwrap()
        );
        assert_eq!(header.borders.bottom.as_ref().unwrap().dash, None);
        assert!(
            header.text_frame.as_ref().unwrap().paragraphs[0].runs[0]
                .style
                .bold
        );
        assert!(table.rows[0].cells[1].horizontal_merge);
        assert_eq!(
            table.rows[0].cells[1].fill,
            parse_hex_color("4472C4").unwrap()
        );
        assert!(table.rows[1].cells[0].vertical_merge);
    }

    #[test]
    fn text_style_inheritance_keeps_text_fill_out_of_shape_fill() {
        let source = br#"<p:sp xmlns:p="p" xmlns:a="a">
          <p:spPr><a:solidFill><a:srgbClr val="112233"/></a:solidFill></p:spPr>
          <p:txBody>
            <a:bodyPr anchor="ctr" vert="vert270" lIns="100" tIns="200" rIns="300" bIns="400" numCol="3" spcCol="91440"><a:normAutofit fontScale="92.000%" lnSpcReduction="20.000%"/></a:bodyPr>
            <a:lstStyle><a:lvl1pPr algn="ctr"><a:buNone/><a:defRPr sz="3200">
              <a:solidFill><a:srgbClr val="445566"/></a:solidFill>
              <a:latin typeface="+mj-lt"/>
            </a:defRPr></a:lvl1pPr></a:lstStyle>
            <a:p><a:pPr rtl="1"><a:lnSpc><a:spcPct val="120000"/></a:lnSpc><a:spcBef><a:spcPts val="600"/></a:spcBef><a:tabLst><a:tab pos="457200" algn="r"/></a:tabLst></a:pPr><a:r><a:rPr i="1" u="sng" strike="sngStrike" spc="120" baseline="30000"><a:solidFill><a:srgbClr val="92D050"/></a:solidFill><a:effectLst><a:glow rad="1000"><a:srgbClr val="ABCDEF"/></a:glow><a:blur rad="2000"/><a:softEdge rad="3000"/><a:reflection/></a:effectLst></a:rPr><a:t>First</a:t></a:r></a:p>
            <a:p><a:r><a:t>Second</a:t></a:r></a:p>
          </p:txBody>
        </p:sp>"#;
        let document = XmlDocument::parse(source.as_slice()).unwrap();
        let end = element_end(&document, 0).unwrap();
        let theme = Theme {
            major_latin: "Calibri".to_owned(),
            ..Theme::default()
        };
        let shape = parse_shape(&document, 0, end, Vec::new(), &theme, &mut Vec::new());

        assert_eq!(shape.text.as_deref(), Some("First\nSecond"));
        let frame = shape.text_frame.as_ref().unwrap();
        assert_eq!(frame.paragraphs.len(), 2);
        assert_eq!(frame.flow, TextFlow::Vertical270);
        assert_eq!(frame.autofit, TextAutofit::ShrinkText);
        assert_eq!(frame.autofit_font_scale, Some(92_000));
        assert_eq!(frame.autofit_line_spacing_reduction, Some(20_000));
        assert_eq!(frame.column_count, 3);
        assert_eq!(frame.column_spacing, 91_440);
        assert_eq!(
            frame.paragraphs[0].line_spacing,
            Some(TextSpacing::Percent(120_000))
        );
        assert_eq!(
            frame.paragraphs[0].space_before,
            Some(TextSpacing::Points(600))
        );
        assert_eq!(frame.paragraphs[0].direction, TextDirection::RightToLeft);
        assert_eq!(frame.paragraphs[0].tabs[0].position, 457_200);
        assert_eq!(
            frame.paragraphs[0].tabs[0].alignment,
            TextTabAlignment::Right
        );
        assert_eq!(frame.paragraphs[0].runs[0].text, "First");
        assert_eq!(frame.paragraphs[0].runs[0].style.italic, Some(true));
        assert_eq!(frame.paragraphs[0].runs[0].style.underline, Some(true));
        assert_eq!(frame.paragraphs[0].runs[0].style.strike, Some(true));
        assert_eq!(
            frame.paragraphs[0].runs[0].style.character_spacing,
            Some(120)
        );
        assert_eq!(frame.paragraphs[0].runs[0].style.baseline, Some(30_000));
        assert_eq!(
            frame.paragraphs[0].runs[0].style.glow,
            Some(Some(TextGlow {
                color: RgbaColor {
                    red: 171,
                    green: 205,
                    blue: 239,
                    alpha: 255,
                },
                radius: 1_000,
            }))
        );
        assert_eq!(frame.paragraphs[0].runs[0].style.blur_radius, Some(2_000));
        assert_eq!(
            frame.paragraphs[0].runs[0].style.soft_edge_radius,
            Some(3_000)
        );
        assert_eq!(frame.paragraphs[0].runs[0].style.reflection, Some(true));
        assert_eq!(shape.text_style.font_size, Some(3_200));
        assert_eq!(shape.text_style.font_family.as_deref(), Some("Calibri"));
        assert_eq!(shape.text_style.alignment, Some(TextAlignment::Center));
        assert_eq!(
            shape.text_style.vertical_alignment,
            Some(TextVerticalAlignment::Center)
        );
        assert_eq!(shape.text_style.italic, Some(true));
        assert_eq!(shape.text_style.bullet, Some(None));
        assert_eq!(shape.text_style.margin_left, Some(100));
        assert_eq!(shape.text_style.margin_top, Some(200));
        assert_eq!(shape.text_style.margin_right, Some(300));
        assert_eq!(shape.text_style.margin_bottom, Some(400));
        assert_eq!(
            shape.text_style.color,
            Some(RgbaColor {
                red: 146,
                green: 208,
                blue: 80,
                alpha: 255,
            })
        );
        assert_eq!(
            shape.fill,
            Some(Fill::Solid(RgbaColor {
                red: 17,
                green: 34,
                blue: 51,
                alpha: 255,
            }))
        );
    }

    #[test]
    fn descendant_paint_does_not_override_shape_fill() {
        for descendant in [
            r#"<a:rPr><a:noFill/></a:rPr>"#,
            r#"<a:rPr><a:gradFill><a:gsLst><a:gs pos="0"><a:srgbClr val="FF0000"/></a:gs><a:gs pos="100000"><a:srgbClr val="0000FF"/></a:gs></a:gsLst></a:gradFill></a:rPr>"#,
            r#"<a:rPr><a:pattFill prst="cross"><a:fgClr><a:srgbClr val="FF0000"/></a:fgClr><a:bgClr><a:srgbClr val="00FF00"/></a:bgClr></a:pattFill></a:rPr>"#,
            r#"<a:ln><a:gradFill><a:gsLst><a:gs pos="0"><a:srgbClr val="FF0000"/></a:gs><a:gs pos="100000"><a:srgbClr val="0000FF"/></a:gs></a:gsLst></a:gradFill></a:ln>"#,
            r#"<a:ln><a:pattFill prst="cross"><a:fgClr><a:srgbClr val="FF0000"/></a:fgClr><a:bgClr><a:srgbClr val="00FF00"/></a:bgClr></a:pattFill></a:ln>"#,
        ] {
            let source = format!(
                r#"<p:sp xmlns:p="p" xmlns:a="a"><p:spPr><a:solidFill><a:srgbClr val="112233"/></a:solidFill>{line_paint}</p:spPr><p:txBody><a:bodyPr/><a:p><a:r>{run_paint}<a:t>Text</a:t></a:r></a:p></p:txBody></p:sp>"#,
                line_paint = if descendant.starts_with("<a:ln>") {
                    descendant
                } else {
                    ""
                },
                run_paint = if descendant.starts_with("<a:rPr>") {
                    descendant
                } else {
                    ""
                },
            );
            let document = XmlDocument::parse(source.into_bytes()).unwrap();
            let shape = parse_shape(
                &document,
                0,
                element_end(&document, 0).unwrap(),
                Vec::new(),
                &Theme::default(),
                &mut Vec::new(),
            );

            assert_eq!(
                shape.fill,
                Some(Fill::Solid(parse_hex_color("112233").unwrap())),
                "descendant paint must not become the shape body fill: {descendant}",
            );
        }
    }

    #[test]
    fn text_links_and_picture_bullets_do_not_become_shape_properties() {
        let source = br#"<p:sp xmlns:p="p" xmlns:a="a" xmlns:r="r">
          <p:nvSpPr><p:cNvPr id="1" name="Linked shape"><a:hlinkClick r:id="rShape"/></p:cNvPr><p:cNvSpPr/><p:nvPr/></p:nvSpPr>
          <p:spPr/>
          <p:txBody><a:bodyPr/><a:p><a:pPr><a:buBlip><a:blip r:embed="rBullet"/></a:buBlip></a:pPr><a:r><a:rPr><a:hlinkClick r:id="rRun"/></a:rPr><a:t>Item</a:t></a:r></a:p></p:txBody>
        </p:sp>"#;
        let document = XmlDocument::parse(source.as_slice()).unwrap();
        let shape = parse_shape(
            &document,
            0,
            element_end(&document, 0).unwrap(),
            Vec::new(),
            &Theme::default(),
            &mut Vec::new(),
        );

        assert_eq!(shape.hyperlink_relationship_id.as_deref(), Some("rShape"));
        assert_eq!(shape.image_relationship_id, None);
        assert_eq!(
            shape.text_frame.as_ref().unwrap().paragraphs[0]
                .bullet_image_relationship_id
                .as_deref(),
            Some("rBullet")
        );
    }

    #[test]
    fn lowers_linear_gradient_custom_path_shadow_and_line_ends() {
        let source = br#"<p:sp xmlns:p="p" xmlns:a="a" xmlns:x="extension">
          <p:spPr>
            <a:custGeom><a:pathLst><a:path w="100" h="100">
              <a:moveTo><a:pt x="0" y="0"/></a:moveTo>
              <a:lnTo><a:pt x="100" y="0"/></a:lnTo>
              <a:quadBezTo><a:pt x="100" y="50"/><a:pt x="50" y="100"/></a:quadBezTo>
              <a:cubicBezTo><a:pt x="40" y="90"/><a:pt x="10" y="60"/><a:pt x="0" y="50"/></a:cubicBezTo>
              <a:arcTo wR="50" hR="50" stAng="10800000" swAng="5400000"/><a:close/>
            </a:path></a:pathLst></a:custGeom>
            <a:gradFill><a:gsLst>
              <a:gs pos="0"><a:srgbClr val="FF0000"><a:alpha val="50000"/></a:srgbClr></a:gs>
              <a:gs pos="100000"><a:srgbClr val="0000FF"/></a:gs>
            </a:gsLst><a:lin ang="5400000"/></a:gradFill>
            <a:ln w="12700"><a:solidFill><a:srgbClr val="000000"/></a:solidFill>
              <a:prstDash val="dash"/><a:headEnd type="triangle"/><a:tailEnd type="diamond"/>
              <a:extLst><a:ext><x:prstDash val="dot"/></a:ext></a:extLst></a:ln>
            <a:effectLst><a:outerShdw blurRad="100" dist="200" dir="5400000">
              <a:srgbClr val="333333"/>
            </a:outerShdw></a:effectLst>
          </p:spPr>
        </p:sp>"#;
        let document = XmlDocument::parse(source.as_slice()).unwrap();
        let end = element_end(&document, 0).unwrap();
        let mut diagnostics = Vec::new();
        let shape = parse_shape(
            &document,
            0,
            end,
            Vec::new(),
            &Theme::default(),
            &mut diagnostics,
        );

        let Fill::LinearGradient { angle, stops } = shape.fill.unwrap() else {
            panic!("expected a linear gradient")
        };
        assert_eq!(angle, 5_400_000);
        assert_eq!(stops.len(), 2);
        assert_eq!(stops[0].color.alpha, 127);
        let commands = shape.custom_path.unwrap().commands;
        assert_eq!(commands.len(), 6);
        assert!(matches!(commands[2], PathCommand::QuadraticTo { .. }));
        assert!(matches!(commands[3], PathCommand::CubicTo { .. }));
        assert!(matches!(commands[4], PathCommand::ArcTo { .. }));
        let stroke = shape.stroke.unwrap();
        assert_eq!(stroke.dash.as_deref(), Some("dash"));
        assert_eq!(stroke.head_end, Some(LineEnd::Triangle));
        assert_eq!(stroke.tail_end, Some(LineEnd::Diamond));
        assert_eq!(shape.outer_shadow.unwrap().distance, 200);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn parses_radial_pattern_and_common_preset_geometry() {
        let radial = br#"<p:sp xmlns:p="p" xmlns:a="a"><p:spPr><a:prstGeom prst="star5"/><a:gradFill><a:gsLst><a:gs pos="0"><a:srgbClr val="FFFFFF"/></a:gs><a:gs pos="100000"><a:srgbClr val="000000"/></a:gs></a:gsLst><a:path path="circle"/></a:gradFill></p:spPr></p:sp>"#;
        let document = XmlDocument::parse(radial.as_slice()).unwrap();
        let shape = parse_shape(
            &document,
            0,
            element_end(&document, 0).unwrap(),
            Vec::new(),
            &Theme::default(),
            &mut Vec::new(),
        );
        assert_eq!(shape.geometry, Some(PresetGeometry::Star5));
        assert!(matches!(shape.fill, Some(Fill::RadialGradient { .. })));

        let pattern = br#"<p:sp xmlns:p="p" xmlns:a="a"><p:spPr><a:prstGeom prst="chevron"/><a:pattFill prst="cross"><a:fgClr><a:srgbClr val="FF0000"/></a:fgClr><a:bgClr><a:srgbClr val="00FF00"/></a:bgClr></a:pattFill></p:spPr></p:sp>"#;
        let document = XmlDocument::parse(pattern.as_slice()).unwrap();
        let shape = parse_shape(
            &document,
            0,
            element_end(&document, 0).unwrap(),
            Vec::new(),
            &Theme::default(),
            &mut Vec::new(),
        );
        assert_eq!(shape.geometry, Some(PresetGeometry::Chevron));
        assert!(matches!(shape.fill, Some(Fill::Pattern { ref preset, .. }) if preset == "cross"));
    }

    #[test]
    fn formats_supported_auto_number_families() {
        assert_eq!(format_auto_number("arabicPeriod", 12), "12.");
        assert_eq!(format_auto_number("alphaLcParenR", 27), "aa)");
        assert_eq!(format_auto_number("alphaUcParenBoth", 2), "(B)");
        assert_eq!(format_auto_number("romanLcPeriod", 14), "xiv.");
    }

    #[test]
    fn empty_paragraph_retains_end_paragraph_mark_metrics() {
        let source = br#"<a:p xmlns:a="a"><a:endParaRPr sz="2400" b="1"/></a:p>"#;
        let document = XmlDocument::parse(source.as_slice()).unwrap();
        let paragraph = parse_rich_paragraph(
            &document,
            0,
            element_end(&document, 0).unwrap(),
            &Theme::default(),
        );
        assert_eq!(paragraph.runs.len(), 1);
        assert!(paragraph.runs[0].text.is_empty());
        assert_eq!(paragraph.runs[0].style.font_size, Some(2_400));
        assert_eq!(paragraph.runs[0].style.bold, Some(true));
    }

    #[test]
    fn picture_bullet_retains_its_relationship_for_lazy_media_resolution() {
        let source = br#"<a:p xmlns:a="a" xmlns:r="r"><a:pPr><a:buFont typeface="Marker Font"/><a:buClr><a:srgbClr val="123456"/></a:buClr><a:buSzPct val="80.000%"/><a:buBlip><a:blip r:embed="rBullet"/></a:buBlip></a:pPr><a:r><a:t>item</a:t></a:r></a:p>"#;
        let document = XmlDocument::parse(source.as_slice()).unwrap();
        let paragraph = parse_rich_paragraph(
            &document,
            0,
            element_end(&document, 0).unwrap(),
            &Theme::default(),
        );
        assert_eq!(
            paragraph.bullet_image_relationship_id.as_deref(),
            Some("rBullet")
        );
        assert_eq!(
            paragraph.style.bullet.as_ref().unwrap().as_deref(),
            Some("◼")
        );
        assert_eq!(paragraph.bullet_font_family.as_deref(), Some("Marker Font"));
        assert_eq!(paragraph.bullet_size, Some(TextSpacing::Percent(80_000)));
        assert_eq!(paragraph.bullet_color.unwrap().red, 0x12);
    }

    #[test]
    fn inherited_bullet_paint_and_text_relative_resets_are_resolved() {
        let source = br#"<p:sp xmlns:p="p" xmlns:a="a"><p:txBody><a:bodyPr/><a:lstStyle><a:lvl1pPr><a:buChar char="*"/><a:buFont typeface="Marker Font"/><a:buClr><a:srgbClr val="123456"/></a:buClr><a:buSzPct val="50000"/></a:lvl1pPr></a:lstStyle><a:p><a:r><a:rPr sz="2000"><a:solidFill><a:srgbClr val="FF0000"/></a:solidFill><a:latin typeface="Arial"/></a:rPr><a:t>inherited</a:t></a:r></a:p><a:p><a:pPr><a:buFontTx/><a:buClrTx/><a:buSzTx/></a:pPr><a:r><a:rPr sz="1800"><a:solidFill><a:srgbClr val="00AA00"/></a:solidFill><a:latin typeface="Arial"/></a:rPr><a:t>text relative</a:t></a:r></a:p></p:txBody></p:sp>"#;
        let document = XmlDocument::parse(source.as_slice()).unwrap();
        let shape = parse_shape(
            &document,
            0,
            element_end(&document, 0).unwrap(),
            Vec::new(),
            &Theme::default(),
            &mut Vec::new(),
        );
        let resolved = resolve_text_frame(
            shape.text_frame.as_ref().unwrap(),
            &PartialTextStyle::default(),
            false,
            None,
        );
        let inherited = resolved.paragraphs[0].bullet_style.as_ref().unwrap();
        assert_eq!(resolved.paragraphs[0].bullet.as_deref(), Some("*"));
        assert_eq!(inherited.font_family.as_deref(), Some("Marker Font"));
        assert_eq!(inherited.font_size, 1_000);
        assert_eq!(inherited.color.red, 0x12);

        let text_relative = resolved.paragraphs[1].bullet_style.as_ref().unwrap();
        assert_eq!(text_relative.font_family.as_deref(), Some("Arial"));
        assert_eq!(text_relative.font_size, 1_800);
        assert_eq!(text_relative.color.green, 0xaa);
    }

    #[test]
    fn inherited_automatic_numbering_continues_and_can_be_reset() {
        let source = br#"<p:sp xmlns:p="p" xmlns:a="a"><p:txBody><a:bodyPr/><a:lstStyle><a:lvl1pPr><a:buAutoNum type="romanUcPeriod" startAt="4"/></a:lvl1pPr></a:lstStyle><a:p><a:r><a:t>four</a:t></a:r></a:p><a:p><a:r><a:t>five</a:t></a:r></a:p><a:p><a:pPr><a:buNone/></a:pPr><a:r><a:t>plain</a:t></a:r></a:p></p:txBody></p:sp>"#;
        let document = XmlDocument::parse(source.as_slice()).unwrap();
        let shape = parse_shape(
            &document,
            0,
            element_end(&document, 0).unwrap(),
            Vec::new(),
            &Theme::default(),
            &mut Vec::new(),
        );
        let resolved = resolve_text_frame(
            shape.text_frame.as_ref().unwrap(),
            &PartialTextStyle::default(),
            false,
            None,
        );
        assert_eq!(resolved.paragraphs[0].bullet.as_deref(), Some("IV."));
        assert_eq!(resolved.paragraphs[1].bullet.as_deref(), Some("V."));
        assert_eq!(resolved.paragraphs[2].bullet, None);
    }

    #[test]
    fn unsupported_text_warp_is_diagnosed_and_falls_back_to_readable_text() {
        let source = br#"<p:sp xmlns:p="p" xmlns:a="a"><p:nvSpPr><p:cNvPr id="9" name="warp"/></p:nvSpPr><p:txBody><a:bodyPr><a:prstTxWarp prst="textCanDown"/></a:bodyPr><a:p><a:r><a:t>readable</a:t></a:r></a:p></p:txBody></p:sp>"#;
        let document = XmlDocument::parse(source.as_slice()).unwrap();
        let mut diagnostics = Vec::new();
        let shape = parse_shape(
            &document,
            0,
            element_end(&document, 0).unwrap(),
            Vec::new(),
            &Theme::default(),
            &mut diagnostics,
        );
        assert_eq!(shape.text.as_deref(), Some("readable"));
        assert!(shape.text_frame.as_ref().unwrap().warp.is_none());
        assert!(diagnostics.iter().any(|(code, shape_id, message)| {
            *code == ResolveDiagnosticCode::UnsupportedEffect
                && *shape_id == Some(9)
                && message.contains("textCanDown")
        }));
    }

    #[test]
    fn pathological_shape_dimensions_are_diagnosed_and_bounded() {
        let source = br#"<p:sp xmlns:p="p" xmlns:a="a"><p:nvSpPr><p:cNvPr id="10" name="bad bounds"/></p:nvSpPr><p:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="-1" cy="999999999"/></a:xfrm></p:spPr><p:txBody><a:bodyPr><a:normAutofit fontScale="0" lnSpcReduction="garbage"/></a:bodyPr><a:p><a:r><a:t>bounded</a:t></a:r></a:p></p:txBody></p:sp>"#;
        let document = XmlDocument::parse(source.as_slice()).unwrap();
        let mut diagnostics = Vec::new();
        let shape = parse_shape(
            &document,
            0,
            element_end(&document, 0).unwrap(),
            Vec::new(),
            &Theme::default(),
            &mut diagnostics,
        );
        let bounds = shape.transform.unwrap().bounds.size;
        assert_eq!(bounds.width, 1);
        assert_eq!(bounds.height, 91_440_000);
        assert_eq!(
            diagnostics
                .iter()
                .filter(|(code, shape_id, _)| {
                    *code == ResolveDiagnosticCode::InvalidValue && *shape_id == Some(10)
                })
                .count(),
            2
        );
        let frame = shape.text_frame.as_ref().unwrap();
        assert_eq!(frame.autofit_font_scale, Some(1_000));
        assert_eq!(frame.autofit_line_spacing_reduction, None);
        assert!(
            resolve_text_frame(frame, &PartialTextStyle::default(), true, None).autofit_recompute
        );
    }
}
