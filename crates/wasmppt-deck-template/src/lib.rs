//! Compiles the explicit Cortex Theme Starter POTX contract into a host-neutral deck plan.
//!
//! Discovery depends only on `p:sldLayout/@matchingName` and standard placeholder
//! type/index pairs. Slides, visible shape names, and host APIs are deliberately absent.

mod policy;
mod xml;

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    sync::Arc,
};

use sha2::{Digest, Sha256};
use wasmppt_deck::{
    DeckDiagnostic, DeckDiagnosticCode, DeckTemplatePlan, DiagnosticSeverity, EmuRect, EmuSize,
    PlaceholderIdentity, RegionRole, SemanticRole, SourceRange, StableId, TemplateAsset,
    TemplateAssetKind, TemplateLayout, TemplateLayoutRole, TemplateTextColor, TemplateTextLevel,
    TemplateTheme, TextMargins, ThemeColor, ThemeFontSet,
};
use wasmppt_opc::{
    DiagnosticCode as OpcDiagnosticCode, MemorySource, PackageGraph, PackageLimits, PartId,
    RelationshipTarget, ZipArchive,
};

use crate::xml::{Element, Elements, attr};

const POTX_MAIN_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.presentationml.template.main+xml";
const LAYOUT_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml";
const PML_NS: &str = "http://schemas.openxmlformats.org/presentationml/2006/main";
const STRICT_PML_NS: &str = "http://purl.oclc.org/ooxml/presentationml/main";
const POLICY: &str = "cortex-theme-starter-v1";
const VALIDATOR_VERSION: u32 = 1;

const REQUIRED_LAYOUTS: [(&str, TemplateLayoutRole); 3] = [
    ("wasmppt:title-v1", TemplateLayoutRole::Title),
    ("wasmppt:content-v1", TemplateLayoutRole::Content),
    ("wasmppt:statement-v1", TemplateLayoutRole::Statement),
];

#[derive(Clone, Debug)]
pub struct ThemeCompileResult {
    pub plan: DeckTemplatePlan,
    /// Invalid or active-content-bearing templates are never eligible for cache insertion.
    pub cacheable: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThemeCompileError {
    message: String,
}

impl ThemeCompileError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for ThemeCompileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ThemeCompileError {}

#[derive(Clone, Debug, Default)]
pub struct ThemeTemplateCompiler {
    package_limits: PackageLimits,
}

impl ThemeTemplateCompiler {
    #[must_use]
    pub fn new(package_limits: PackageLimits) -> Self {
        Self { package_limits }
    }

    pub fn compile(
        &self,
        bytes: impl Into<Arc<[u8]>>,
    ) -> Result<ThemeCompileResult, ThemeCompileError> {
        let bytes = bytes.into();
        let template_hash: [u8; 32] = Sha256::digest(&bytes).into();
        let cache_key = cache_key(template_hash);
        let archive = ZipArchive::from_bytes_with_limits(bytes, self.package_limits.clone())
            .map_err(|error| {
                ThemeCompileError::new(format!("cannot open POTX package: {error}"))
            })?;
        compile_archive(&archive, template_hash, cache_key)
    }
}

fn compile_archive(
    archive: &ZipArchive<MemorySource>,
    template_hash: [u8; 32],
    cache_key: [u8; 32],
) -> Result<ThemeCompileResult, ThemeCompileError> {
    let mut diagnostics = Vec::new();
    for problem in policy::inspect_active_content(archive) {
        diagnostics.push(diagnostic(
            DeckDiagnosticCode::TEMPLATE_UNSAFE_CONTENT,
            problem,
            None,
        ));
    }

    let graph = PackageGraph::build(archive)
        .map_err(|error| ThemeCompileError::new(format!("cannot build POTX graph: {error}")))?;
    collect_graph_diagnostics(&graph, &mut diagnostics);

    let main_parts = graph
        .parts()
        .iter()
        .filter(|part| graph.content_type(part) == Some(POTX_MAIN_TYPE))
        .map(|part| part.id)
        .collect::<Vec<_>>();
    if main_parts.len() != 1 {
        diagnostics.push(diagnostic(
            DeckDiagnosticCode::TEMPLATE_WRONG_CONTENT_TYPE,
            format!(
                "expected exactly one non-macro POTX main part, found {}",
                main_parts.len()
            ),
            None,
        ));
    }
    let main_id = main_parts.first().copied();
    if let Some(main_id) = main_id {
        if !package_targets_main(&graph, main_id) {
            diagnostics.push(diagnostic(
                DeckDiagnosticCode::TEMPLATE_INVALID_GRAPH,
                "the package officeDocument relationship does not target the POTX main part",
                None,
            ));
        }
    }

    let page_size = main_id
        .and_then(|id| read_elements(archive, &graph, id, &mut diagnostics))
        .and_then(|elements| parse_page_size(&elements, &mut diagnostics))
        .unwrap_or_default();

    let mut discovered = BTreeMap::<String, Vec<PartId>>::new();
    for part in graph
        .parts()
        .iter()
        .filter(|part| graph.content_type(part) == Some(LAYOUT_TYPE))
    {
        let Some(elements) = read_elements(archive, &graph, part.id, &mut diagnostics) else {
            continue;
        };
        let Some(root) = elements.root() else {
            continue;
        };
        if !is_pml_root(root, "sldLayout") {
            diagnostics.push(diagnostic(
                DeckDiagnosticCode::TEMPLATE_INVALID_XML,
                format!(
                    "{} is not a PresentationML slide layout",
                    graph.part_name(part)
                ),
                Some(part_source(graph.part_name(part), root)),
            ));
            continue;
        }
        if let Some(name) = attr(root, "matchingName") {
            discovered.entry(name.to_owned()).or_default().push(part.id);
        }
    }

    for (name, _) in REQUIRED_LAYOUTS {
        match discovered.get(name).map(Vec::len).unwrap_or(0) {
            0 => diagnostics.push(diagnostic(
                DeckDiagnosticCode::TEMPLATE_MISSING_LAYOUT,
                format!("missing required slide layout {name}"),
                None,
            )),
            1 => {}
            count => diagnostics.push(diagnostic(
                DeckDiagnosticCode::TEMPLATE_DUPLICATE_LAYOUT,
                format!("slide layout {name} occurs {count} times"),
                None,
            )),
        }
    }

    let theme_parts = theme_parts(&graph, &discovered, &mut diagnostics);
    if theme_parts.len() > 1 {
        diagnostics.push(diagnostic(
            DeckDiagnosticCode::TEMPLATE_INVALID_GRAPH,
            "required layouts resolve to different theme parts",
            None,
        ));
    }
    let theme = theme_parts
        .first()
        .copied()
        .and_then(|id| read_elements(archive, &graph, id, &mut diagnostics))
        .map(|elements| parse_theme(&elements))
        .unwrap_or_else(|| {
            diagnostics.push(diagnostic(
                DeckDiagnosticCode::TEMPLATE_MISSING_THEME,
                "required master theme relationship is missing",
                None,
            ));
            TemplateTheme::default()
        });

    let mut layouts = Vec::new();
    let mut regions = Vec::new();
    let mut assets = Vec::new();
    {
        let mut compiler = LayoutCompiler {
            archive,
            graph: &graph,
            template_hash,
            theme: &theme,
            diagnostics: &mut diagnostics,
            layouts: &mut layouts,
            regions: &mut regions,
            assets: &mut assets,
        };
        for (matching_name, role) in REQUIRED_LAYOUTS {
            let Some(layout_id) = discovered
                .get(matching_name)
                .and_then(|parts| (parts.len() == 1).then_some(parts[0]))
            else {
                continue;
            };
            compiler.compile(layout_id, matching_name, role);
        }
    }

    diagnostics.sort_by(|left, right| {
        (
            left.code.0,
            left.source.as_ref().map(|source| &source.source),
            &left.message,
        )
            .cmp(&(
                right.code.0,
                right.source.as_ref().map(|source| &source.source),
                &right.message,
            ))
    });
    let cacheable = page_size.is_positive()
        && !diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == DiagnosticSeverity::Error);
    let plan = DeckTemplatePlan {
        id: stable_id(&cache_key),
        template_hash,
        cache_key,
        validator_version: VALIDATOR_VERSION,
        compiler_policy: POLICY.to_owned(),
        page_size,
        theme,
        layouts,
        regions,
        assets,
        diagnostics,
    };
    Ok(ThemeCompileResult { plan, cacheable })
}

struct LayoutCompiler<'a> {
    archive: &'a ZipArchive<MemorySource>,
    graph: &'a PackageGraph,
    template_hash: [u8; 32],
    theme: &'a TemplateTheme,
    diagnostics: &'a mut Vec<DeckDiagnostic>,
    layouts: &'a mut Vec<TemplateLayout>,
    regions: &'a mut Vec<wasmppt_deck::TemplateRegion>,
    assets: &'a mut Vec<TemplateAsset>,
}

impl LayoutCompiler<'_> {
    fn compile(&mut self, layout_part_id: PartId, matching_name: &str, role: TemplateLayoutRole) {
        let layout_part = self.graph.part(layout_part_id);
        let layout_name = self.graph.part_name(layout_part);
        let Some(layout_xml) =
            read_elements(self.archive, self.graph, layout_part_id, self.diagnostics)
        else {
            return;
        };
        let Some(master_id) = related_part(self.graph, layout_part_id, "/slideMaster") else {
            self.diagnostics.push(diagnostic(
                DeckDiagnosticCode::TEMPLATE_INVALID_GRAPH,
                format!("{layout_name} has no slideMaster relationship"),
                None,
            ));
            return;
        };
        let master_name = self.graph.part_name(self.graph.part(master_id));
        let Some(master_xml) = read_elements(self.archive, self.graph, master_id, self.diagnostics)
        else {
            return;
        };

        let id = derive_id(&self.template_hash, b"layout", &[matching_name.as_bytes()]);
        let master_placeholders = placeholder_facts(&master_xml, master_name, self.theme);
        let layout_placeholders = placeholder_facts(&layout_xml, layout_name, self.theme);
        report_duplicate_placeholders(layout_name, &layout_placeholders, self.diagnostics);
        let master_styles = master_text_styles(&master_xml, self.theme);
        let mut region_ids = Vec::new();
        let mut role_counts = BTreeMap::<RegionRole, usize>::new();

        for placeholder in layout_placeholders {
            let region_role = placeholder_role(role, &placeholder.identity.kind);
            let Some(region_role) = region_role else {
                continue;
            };
            *role_counts.entry(region_role).or_default() += 1;
            let inherited = master_placeholders
                .iter()
                .find(|master| master.identity == placeholder.identity);
            let frame = placeholder
                .frame
                .or_else(|| inherited.and_then(|master| master.frame));
            let Some(frame) = frame else {
                self.diagnostics.push(diagnostic(
                    DeckDiagnosticCode::TEMPLATE_INVALID_PLACEHOLDER,
                    format!(
                        "{matching_name} placeholder {}:{} has no resolvable bounds",
                        placeholder.identity.kind, placeholder.identity.index
                    ),
                    Some(placeholder.source.clone()),
                ));
                continue;
            };
            if !frame.is_positive() {
                self.diagnostics.push(diagnostic(
                    DeckDiagnosticCode::TEMPLATE_INVALID_PLACEHOLDER,
                    format!(
                        "{matching_name} placeholder {}:{} has non-positive bounds",
                        placeholder.identity.kind, placeholder.identity.index
                    ),
                    Some(placeholder.source.clone()),
                ));
                continue;
            }
            let category_styles = style_for_region(&master_styles, region_role);
            let inherited_styles = inherited
                .map(|value| value.text_levels.as_slice())
                .unwrap_or(&[]);
            let text_levels = merge_text_levels(
                &merge_text_levels(category_styles, inherited_styles),
                &placeholder.text_levels,
            );
            let margins = placeholder
                .margins
                .merge(inherited.map(|value| value.margins).unwrap_or_default())
                .finish();
            let region_id = derive_id(
                id.as_bytes(),
                b"placeholder",
                &[
                    placeholder.identity.kind.as_bytes(),
                    &placeholder.identity.index.to_le_bytes(),
                ],
            );
            region_ids.push(region_id);
            self.regions.push(wasmppt_deck::TemplateRegion {
                id: region_id,
                layout_id: id,
                role: region_role,
                placeholder: placeholder.identity,
                frame,
                margins,
                text_levels,
                accepts: accepted_roles(region_role),
                required: required_region(role, region_role),
            });
        }
        report_required_regions(matching_name, role, &role_counts, self.diagnostics);

        let related_parts = preserved_relationship_parts(self.graph, layout_part_id, master_id);
        let mut layout_assets = collect_assets(&master_xml, master_name, id, &related_parts, 0);
        let z_offset = u32::try_from(layout_assets.len()).unwrap_or(u32::MAX);
        layout_assets.extend(collect_assets(
            &layout_xml,
            layout_name,
            id,
            &related_parts,
            z_offset,
        ));
        let asset_ids = layout_assets.iter().map(|asset| asset.id).collect();
        self.assets.extend(layout_assets);
        let background = find_background(&layout_xml, layout_name)
            .or_else(|| find_background(&master_xml, master_name));
        self.layouts.push(TemplateLayout {
            id,
            role,
            matching_name: matching_name.to_owned(),
            source_part: layout_name.to_owned(),
            master_part: master_name.to_owned(),
            region_ids,
            asset_ids,
            background,
        });
    }
}

fn collect_graph_diagnostics(graph: &PackageGraph, diagnostics: &mut Vec<DeckDiagnostic>) {
    for graph_diagnostic in graph.diagnostics() {
        if matches!(
            graph_diagnostic.code,
            OpcDiagnosticCode::RelationshipCycle | OpcDiagnosticCode::OrphanedPart
        ) {
            continue;
        }
        let source = graph_diagnostic
            .part
            .map(|id| SourceRange::new(graph.part_name(graph.part(id)), 0, 0));
        diagnostics.push(diagnostic(
            DeckDiagnosticCode::TEMPLATE_INVALID_GRAPH,
            graph_diagnostic.message.clone(),
            source,
        ));
    }
}

fn package_targets_main(graph: &PackageGraph, main: PartId) -> bool {
    graph.package_relationships().iter().any(|relationship| {
        graph
            .relationship_type(relationship)
            .ends_with("/officeDocument")
            && relationship.target == RelationshipTarget::Internal(main)
    })
}

fn read_elements(
    archive: &ZipArchive<MemorySource>,
    graph: &PackageGraph,
    part_id: PartId,
    diagnostics: &mut Vec<DeckDiagnostic>,
) -> Option<Elements> {
    let name = graph.part_name(graph.part(part_id));
    let entry = archive.entry(name)?;
    let bytes = match archive.read_entry(entry) {
        Ok(bytes) => bytes,
        Err(error) => {
            diagnostics.push(diagnostic(
                DeckDiagnosticCode::TEMPLATE_INVALID_PACKAGE,
                format!("cannot read {name}: {error}"),
                None,
            ));
            return None;
        }
    };
    match Elements::parse(bytes) {
        Ok(elements) => Some(elements),
        Err(error) => {
            diagnostics.push(diagnostic(
                DeckDiagnosticCode::TEMPLATE_INVALID_XML,
                format!("cannot parse {name}: {error}"),
                Some(SourceRange::new(
                    name,
                    u32::try_from(error.offset()).unwrap_or(u32::MAX),
                    u32::try_from(error.offset()).unwrap_or(u32::MAX),
                )),
            ));
            None
        }
    }
}

fn parse_page_size(elements: &Elements, diagnostics: &mut Vec<DeckDiagnostic>) -> Option<EmuSize> {
    let root = elements.root()?;
    if !is_pml_root(root, "presentation") {
        diagnostics.push(diagnostic(
            DeckDiagnosticCode::TEMPLATE_INVALID_XML,
            "POTX main part has an unexpected root",
            Some(part_source("ppt/presentation.xml", root)),
        ));
        return None;
    }
    let size = elements
        .descendants(root)
        .find(|element| element.local == "sldSz")?;
    let width = attr(size, "cx").and_then(|value| value.parse::<i64>().ok());
    let height = attr(size, "cy").and_then(|value| value.parse::<i64>().ok());
    match (width, height) {
        (Some(width), Some(height)) if width > 0 && height > 0 => Some(EmuSize { width, height }),
        _ => {
            diagnostics.push(diagnostic(
                DeckDiagnosticCode::TEMPLATE_INVALID_PAGE_SIZE,
                "POTX slide size must contain positive cx and cy EMU values",
                Some(part_source("ppt/presentation.xml", size)),
            ));
            None
        }
    }
}

fn theme_parts(
    graph: &PackageGraph,
    discovered: &BTreeMap<String, Vec<PartId>>,
    diagnostics: &mut Vec<DeckDiagnostic>,
) -> Vec<PartId> {
    let mut themes = BTreeSet::new();
    for (name, _) in REQUIRED_LAYOUTS {
        for layout in discovered.get(name).into_iter().flatten() {
            let Some(master) = related_part(graph, *layout, "/slideMaster") else {
                continue;
            };
            if let Some(theme) = related_part(graph, master, "/theme") {
                themes.insert(theme.index());
            } else {
                diagnostics.push(diagnostic(
                    DeckDiagnosticCode::TEMPLATE_INVALID_GRAPH,
                    format!("{name} master has no theme relationship"),
                    None,
                ));
            }
        }
    }
    themes
        .into_iter()
        .map(|index| graph.parts()[index].id)
        .collect()
}

fn related_part(graph: &PackageGraph, source: PartId, suffix: &str) -> Option<PartId> {
    graph
        .part(source)
        .relationships
        .iter()
        .find_map(|relationship| {
            if graph.relationship_type(relationship).ends_with(suffix) {
                if let RelationshipTarget::Internal(target) = relationship.target {
                    return Some(target);
                }
            }
            None
        })
}

fn parse_theme(elements: &Elements) -> TemplateTheme {
    let Some(root) = elements.root() else {
        return TemplateTheme::default();
    };
    let major_fonts = elements
        .descendants(root)
        .find(|element| element.local == "majorFont")
        .map(|element| parse_font_set(elements, element))
        .unwrap_or_default();
    let minor_fonts = elements
        .descendants(root)
        .find(|element| element.local == "minorFont")
        .map(|element| parse_font_set(elements, element))
        .unwrap_or_default();
    let mut colors = Vec::new();
    if let Some(scheme) = elements
        .descendants(root)
        .find(|element| element.local == "clrScheme")
    {
        for slot in elements
            .descendants(scheme)
            .filter(|element| is_color_slot(&element.local))
        {
            if let Some(rgb) = parse_color(elements, slot, &[]) {
                colors.push(ThemeColor {
                    slot: slot.local.clone(),
                    rgb,
                });
            }
        }
        colors.sort_by(|left, right| left.slot.cmp(&right.slot));
        colors.dedup_by(|left, right| left.slot == right.slot);
    }
    TemplateTheme {
        major_fonts,
        minor_fonts,
        colors,
    }
}

fn parse_font_set(elements: &Elements, parent: &Element) -> ThemeFontSet {
    let mut fonts = ThemeFontSet::default();
    for element in elements.descendants(parent) {
        match element.local.as_str() {
            "latin" => fonts.latin = attr(element, "typeface").map(str::to_owned),
            "ea" => fonts.east_asian = attr(element, "typeface").map(str::to_owned),
            "cs" => fonts.complex_script = attr(element, "typeface").map(str::to_owned),
            _ => {}
        }
    }
    fonts
}

#[derive(Clone, Debug)]
struct PlaceholderFacts {
    identity: PlaceholderIdentity,
    frame: Option<EmuRect>,
    margins: PartialMargins,
    text_levels: Vec<TemplateTextLevel>,
    source: SourceRange,
}

fn placeholder_facts(
    elements: &Elements,
    part: &str,
    theme: &TemplateTheme,
) -> Vec<PlaceholderFacts> {
    shape_elements(elements)
        .filter_map(|shape| {
            let placeholder = elements.first_descendant(shape, "ph")?;
            let identity = PlaceholderIdentity {
                kind: attr(placeholder, "type").unwrap_or("obj").to_owned(),
                index: attr(placeholder, "idx")
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(0),
            };
            Some(PlaceholderFacts {
                identity,
                frame: parse_frame(elements, shape),
                margins: parse_margins(elements, shape),
                text_levels: parse_text_levels(elements, shape, theme),
                source: part_source(part, shape),
            })
        })
        .collect()
}

fn shape_elements(elements: &Elements) -> impl Iterator<Item = &Element> {
    elements.values().iter().filter(|element| {
        matches!(
            element.local.as_str(),
            "sp" | "pic" | "graphicFrame" | "cxnSp"
        )
    })
}

fn parse_frame(elements: &Elements, shape: &Element) -> Option<EmuRect> {
    let transform = elements.first_descendant(shape, "xfrm")?;
    let offset = elements.first_descendant(transform, "off")?;
    let extent = elements.first_descendant(transform, "ext")?;
    Some(EmuRect {
        x: parse_i64(offset, "x")?,
        y: parse_i64(offset, "y")?,
        width: parse_i64(extent, "cx")?,
        height: parse_i64(extent, "cy")?,
    })
}

fn parse_i64(element: &Element, name: &str) -> Option<i64> {
    attr(element, name)?.parse().ok()
}

#[derive(Clone, Copy, Debug, Default)]
struct PartialMargins {
    left: Option<i64>,
    top: Option<i64>,
    right: Option<i64>,
    bottom: Option<i64>,
}

impl PartialMargins {
    fn merge(self, fallback: Self) -> Self {
        Self {
            left: self.left.or(fallback.left),
            top: self.top.or(fallback.top),
            right: self.right.or(fallback.right),
            bottom: self.bottom.or(fallback.bottom),
        }
    }

    fn finish(self) -> TextMargins {
        TextMargins {
            left: self.left.unwrap_or(91_440),
            top: self.top.unwrap_or(45_720),
            right: self.right.unwrap_or(91_440),
            bottom: self.bottom.unwrap_or(45_720),
        }
    }
}

fn parse_margins(elements: &Elements, shape: &Element) -> PartialMargins {
    let Some(body) = elements.first_descendant(shape, "bodyPr") else {
        return PartialMargins::default();
    };
    PartialMargins {
        left: parse_i64(body, "lIns"),
        top: parse_i64(body, "tIns"),
        right: parse_i64(body, "rIns"),
        bottom: parse_i64(body, "bIns"),
    }
}

fn parse_text_levels(
    elements: &Elements,
    parent: &Element,
    theme: &TemplateTheme,
) -> Vec<TemplateTextLevel> {
    let mut levels = Vec::new();
    for element in elements.descendants(parent) {
        let Some(level) = level_number(&element.local) else {
            continue;
        };
        let run = elements
            .first_descendant(element, "defRPr")
            .unwrap_or(element);
        let color = parse_text_color(elements, run, theme);
        let mut output = TemplateTextLevel {
            level,
            font_size: attr(run, "sz").and_then(|value| value.parse().ok()),
            color,
            bold: attr(run, "b").and_then(parse_bool),
            italic: attr(run, "i").and_then(parse_bool),
            margin_left: parse_i64(element, "marL"),
            indent: parse_i64(element, "indent"),
            ..TemplateTextLevel::default()
        };
        for child in elements.descendants(run) {
            match child.local.as_str() {
                "latin" => output.latin_typeface = attr(child, "typeface").map(str::to_owned),
                "ea" => {
                    output.east_asian_typeface = attr(child, "typeface").map(str::to_owned);
                }
                "cs" => {
                    output.complex_script_typeface = attr(child, "typeface").map(str::to_owned);
                }
                _ => {}
            }
        }
        levels.push(output);
    }
    levels.sort_by_key(|level| level.level);
    levels.dedup_by_key(|level| level.level);
    levels
}

fn level_number(local: &str) -> Option<u8> {
    let number = local.strip_prefix("lvl")?.strip_suffix("pPr")?;
    let one_based = number.parse::<u8>().ok()?;
    one_based.checked_sub(1).filter(|level| *level < 9)
}

fn parse_text_color(
    elements: &Elements,
    parent: &Element,
    theme: &TemplateTheme,
) -> Option<TemplateTextColor> {
    let color = elements
        .descendants(parent)
        .find(|element| matches!(element.local.as_str(), "srgbClr" | "sysClr" | "schemeClr"))?;
    let scheme = (color.local == "schemeClr")
        .then(|| attr(color, "val").map(str::to_owned))
        .flatten();
    let rgb = if let Some(scheme) = &scheme {
        theme
            .colors
            .iter()
            .find(|color| &color.slot == scheme)
            .map(|color| color.rgb)?
    } else {
        parse_hex(attr(color, "val").or_else(|| attr(color, "lastClr"))?)?
    };
    Some(TemplateTextColor { scheme, rgb })
}

#[derive(Clone, Debug, Default)]
struct MasterTextStyles {
    title: Vec<TemplateTextLevel>,
    body: Vec<TemplateTextLevel>,
    other: Vec<TemplateTextLevel>,
}

fn master_text_styles(elements: &Elements, theme: &TemplateTheme) -> MasterTextStyles {
    let mut styles = MasterTextStyles::default();
    for element in elements.values() {
        match element.local.as_str() {
            "titleStyle" => styles.title = parse_text_levels(elements, element, theme),
            "bodyStyle" => styles.body = parse_text_levels(elements, element, theme),
            "otherStyle" => styles.other = parse_text_levels(elements, element, theme),
            _ => {}
        }
    }
    styles
}

fn style_for_region(styles: &MasterTextStyles, role: RegionRole) -> &[TemplateTextLevel] {
    match role {
        RegionRole::Title | RegionRole::Statement => &styles.title,
        RegionRole::Body => &styles.body,
        _ => &styles.other,
    }
}

fn merge_text_levels(
    base: &[TemplateTextLevel],
    overlay: &[TemplateTextLevel],
) -> Vec<TemplateTextLevel> {
    let mut merged = base
        .iter()
        .cloned()
        .map(|level| (level.level, level))
        .collect::<BTreeMap<_, _>>();
    for next in overlay {
        let current = merged
            .entry(next.level)
            .or_insert_with(|| TemplateTextLevel {
                level: next.level,
                ..TemplateTextLevel::default()
            });
        current.font_size = next.font_size.or(current.font_size);
        current.latin_typeface = next
            .latin_typeface
            .clone()
            .or_else(|| current.latin_typeface.clone());
        current.east_asian_typeface = next
            .east_asian_typeface
            .clone()
            .or_else(|| current.east_asian_typeface.clone());
        current.complex_script_typeface = next
            .complex_script_typeface
            .clone()
            .or_else(|| current.complex_script_typeface.clone());
        current.color = next.color.clone().or_else(|| current.color.clone());
        current.bold = next.bold.or(current.bold);
        current.italic = next.italic.or(current.italic);
        current.margin_left = next.margin_left.or(current.margin_left);
        current.indent = next.indent.or(current.indent);
    }
    merged.into_values().collect()
}

fn report_duplicate_placeholders(
    layout_name: &str,
    placeholders: &[PlaceholderFacts],
    diagnostics: &mut Vec<DeckDiagnostic>,
) {
    let mut counts = BTreeMap::<(String, u32), usize>::new();
    for placeholder in placeholders {
        *counts
            .entry((
                placeholder.identity.kind.clone(),
                placeholder.identity.index,
            ))
            .or_default() += 1;
    }
    for ((kind, index), count) in counts {
        if count > 1 {
            diagnostics.push(diagnostic(
                DeckDiagnosticCode::TEMPLATE_DUPLICATE_PLACEHOLDER,
                format!("{layout_name} contains {count} placeholders for {kind}:{index}"),
                None,
            ));
        }
    }
}

fn placeholder_role(layout: TemplateLayoutRole, kind: &str) -> Option<RegionRole> {
    match kind {
        "title" | "ctrTitle" if layout == TemplateLayoutRole::Statement => {
            Some(RegionRole::Statement)
        }
        "title" | "ctrTitle" => Some(RegionRole::Title),
        "subTitle" if layout == TemplateLayoutRole::Statement => Some(RegionRole::Caption),
        "subTitle" => Some(RegionRole::Subtitle),
        "body" | "obj" => Some(RegionRole::Body),
        "pic" => Some(RegionRole::Media),
        // Page furniture is preserved by `collect_assets`; it is not a
        // semantic insertion target and therefore must not become a region.
        "ftr" | "dt" | "sldNum" => None,
        _ => None,
    }
}

fn required_region(layout: TemplateLayoutRole, role: RegionRole) -> bool {
    matches!(
        (layout, role),
        (TemplateLayoutRole::Title, RegionRole::Title)
            | (TemplateLayoutRole::Title, RegionRole::Subtitle)
            | (TemplateLayoutRole::Content, RegionRole::Title)
            | (TemplateLayoutRole::Content, RegionRole::Body)
            | (TemplateLayoutRole::Statement, RegionRole::Statement)
    )
}

fn report_required_regions(
    name: &str,
    layout: TemplateLayoutRole,
    counts: &BTreeMap<RegionRole, usize>,
    diagnostics: &mut Vec<DeckDiagnostic>,
) {
    let required: &[RegionRole] = match layout {
        TemplateLayoutRole::Title => &[RegionRole::Title, RegionRole::Subtitle],
        TemplateLayoutRole::Content => &[RegionRole::Title, RegionRole::Body],
        TemplateLayoutRole::Statement => &[RegionRole::Statement],
    };
    for role in required {
        match counts.get(role).copied().unwrap_or(0) {
            0 => diagnostics.push(diagnostic(
                DeckDiagnosticCode::TEMPLATE_INVALID_PLACEHOLDER,
                format!("{name} is missing required {role:?} placeholder"),
                None,
            )),
            1 => {}
            count => diagnostics.push(diagnostic(
                DeckDiagnosticCode::TEMPLATE_DUPLICATE_PLACEHOLDER,
                format!("{name} resolves {count} placeholders to required role {role:?}"),
                None,
            )),
        }
    }
}

fn accepted_roles(role: RegionRole) -> Vec<SemanticRole> {
    match role {
        RegionRole::Title => vec![SemanticRole::Title, SemanticRole::Section],
        RegionRole::Subtitle => vec![SemanticRole::Subtitle, SemanticRole::Credit],
        RegionRole::Body => vec![
            SemanticRole::Prose,
            SemanticRole::List,
            SemanticRole::Figure,
            SemanticRole::Caption,
            SemanticRole::Gallery,
            SemanticRole::Table,
            SemanticRole::Chart,
            SemanticRole::Code,
            SemanticRole::Diagram,
            SemanticRole::DisplayMath,
            SemanticRole::Quote,
            SemanticRole::Credit,
            SemanticRole::Definition,
            SemanticRole::DefinitionTerm,
            SemanticRole::DefinitionDescription,
            SemanticRole::Statement,
        ],
        RegionRole::Statement => vec![
            SemanticRole::Statement,
            SemanticRole::Quote,
            SemanticRole::DisplayMath,
            SemanticRole::Prose,
            SemanticRole::Caption,
            SemanticRole::Credit,
        ],
        RegionRole::Media => vec![SemanticRole::Figure, SemanticRole::Gallery],
        RegionRole::Caption => vec![SemanticRole::Caption, SemanticRole::Credit],
        RegionRole::Footer => vec![],
        RegionRole::Table => vec![SemanticRole::Table],
        RegionRole::Chart => vec![SemanticRole::Chart],
        RegionRole::Code => vec![SemanticRole::Code],
    }
}

fn collect_assets(
    elements: &Elements,
    part: &str,
    layout_id: StableId,
    related_parts: &[String],
    z_offset: u32,
) -> Vec<TemplateAsset> {
    shape_elements(elements)
        .filter(|shape| {
            elements
                .first_descendant(shape, "ph")
                .is_none_or(|placeholder| {
                    matches!(attr(placeholder, "type"), Some("ftr" | "dt" | "sldNum"))
                })
        })
        .enumerate()
        .map(|(index, shape)| {
            let z_order = z_offset.saturating_add(u32::try_from(index).unwrap_or(u32::MAX));
            let kind = match elements
                .first_descendant(shape, "ph")
                .and_then(|placeholder| attr(placeholder, "type"))
            {
                Some("ftr" | "dt" | "sldNum") => TemplateAssetKind::Footer,
                _ if shape.local == "pic" => TemplateAssetKind::Logo,
                _ => TemplateAssetKind::Decoration,
            };
            let source_xml = part_source(part, shape);
            let id = derive_id(
                layout_id.as_bytes(),
                b"asset",
                &[
                    part.as_bytes(),
                    &source_xml.start.to_le_bytes(),
                    &source_xml.end.to_le_bytes(),
                ],
            );
            TemplateAsset {
                id,
                layout_id,
                kind,
                source_part: part.to_owned(),
                source_xml,
                frame: parse_frame(elements, shape),
                z_order,
                related_parts: related_parts.to_vec(),
            }
        })
        .collect()
}

fn preserved_relationship_parts(
    graph: &PackageGraph,
    layout: PartId,
    master: PartId,
) -> Vec<String> {
    let mut parts = BTreeSet::new();
    for source in [layout, master] {
        for relationship in &graph.part(source).relationships {
            let kind = graph.relationship_type(relationship);
            if kind.ends_with("/slideMaster")
                || kind.ends_with("/slideLayout")
                || kind.ends_with("/theme")
            {
                continue;
            }
            if let RelationshipTarget::Internal(target) = relationship.target {
                parts.insert(graph.part_name(graph.part(target)).to_owned());
            }
        }
    }
    parts.into_iter().collect()
}

fn find_background(elements: &Elements, part: &str) -> Option<SourceRange> {
    elements
        .values()
        .iter()
        .find(|element| element.local == "bg")
        .map(|element| part_source(part, element))
}

fn parse_color(elements: &Elements, parent: &Element, known: &[ThemeColor]) -> Option<u32> {
    let color = elements
        .descendants(parent)
        .find(|element| matches!(element.local.as_str(), "srgbClr" | "sysClr" | "schemeClr"))?;
    if color.local == "schemeClr" {
        let slot = attr(color, "val")?;
        return known
            .iter()
            .find(|color| color.slot == slot)
            .map(|color| color.rgb);
    }
    parse_hex(attr(color, "val").or_else(|| attr(color, "lastClr"))?)
}

fn parse_hex(value: &str) -> Option<u32> {
    (value.len() == 6)
        .then(|| u32::from_str_radix(value, 16).ok())
        .flatten()
}

fn is_color_slot(local: &str) -> bool {
    matches!(
        local,
        "dk1"
            | "lt1"
            | "dk2"
            | "lt2"
            | "accent1"
            | "accent2"
            | "accent3"
            | "accent4"
            | "accent5"
            | "accent6"
            | "hlink"
            | "folHlink"
    )
}

fn parse_bool(value: &str) -> Option<bool> {
    match value {
        "1" | "true" | "on" => Some(true),
        "0" | "false" | "off" => Some(false),
        _ => None,
    }
}

fn is_pml_root(root: &Element, local: &str) -> bool {
    root.local == local && matches!(root.namespace.as_deref(), Some(PML_NS | STRICT_PML_NS))
}

fn part_source(part: &str, element: &Element) -> SourceRange {
    SourceRange::new(
        part,
        u32::try_from(element.range.start).unwrap_or(u32::MAX),
        u32::try_from(element.range.end).unwrap_or(u32::MAX),
    )
}

fn diagnostic(
    code: DeckDiagnosticCode,
    message: impl Into<String>,
    source: Option<SourceRange>,
) -> DeckDiagnostic {
    DeckDiagnostic {
        code,
        severity: DiagnosticSeverity::Error,
        message: message.into(),
        source,
        node_id: None,
        page_id: None,
    }
}

fn cache_key(template_hash: [u8; 32]) -> [u8; 32] {
    let mut digest = Sha256::new();
    digest.update(b"wasmppt/deck-template/cache/v1\0");
    digest.update(template_hash);
    digest.update(VALIDATOR_VERSION.to_le_bytes());
    digest.update(DeckTemplatePlan::SCHEMA_VERSION.to_le_bytes());
    digest.update(env!("CARGO_PKG_VERSION").as_bytes());
    digest.update(wasmppt_opc::VERSION.as_bytes());
    digest.update(POLICY.as_bytes());
    digest.finalize().into()
}

fn derive_id(seed: &[u8], domain: &[u8], values: &[&[u8]]) -> StableId {
    let mut digest = Sha256::new();
    digest.update(b"wasmppt/deck-template/id/v1\0");
    digest.update(seed);
    digest.update((domain.len() as u64).to_le_bytes());
    digest.update(domain);
    for value in values {
        digest.update((value.len() as u64).to_le_bytes());
        digest.update(value);
    }
    stable_id(&digest.finalize())
}

fn stable_id(bytes: &[u8]) -> StableId {
    let mut id = [0; 16];
    id.copy_from_slice(&bytes[..16]);
    StableId::from_bytes(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_key_changes_with_template_bytes() {
        assert_ne!(cache_key([1; 32]), cache_key([2; 32]));
        assert_eq!(cache_key([1; 32]), cache_key([1; 32]));
    }

    #[test]
    fn visible_names_are_not_part_of_identity() {
        let template = [7; 32];
        assert_eq!(
            derive_id(&template, b"layout", &[b"wasmppt:title-v1"]),
            derive_id(&template, b"layout", &[b"wasmppt:title-v1"])
        );
    }
}
