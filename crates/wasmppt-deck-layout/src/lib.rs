//! Bounded, deterministic semantic layout and automatic pagination.
//!
//! The planner evaluates a small generic candidate family over template-owned frames. It never
//! depends on a DOM, host font APIs, or a product slide-count limit.

mod flow;
mod measure;

use std::{cmp::Ordering, collections::BTreeMap, fmt, sync::Arc};

use flow::{FlowError, FlowUnit, build_flow};
use measure::{MeasureError, Measurer};
use sha2::{Digest, Sha256};
use wasmppt_deck::{
    ContentFit, DeckDiagnostic, DeckDiagnosticCode, DeckLimits, DeckPlan, DeckSpec,
    DeckTemplatePlan, DiagnosticSeverity, Emu, EmuRect, FragmentSlice, LogicalSlide,
    LogicalSlideKind, PhysicalPage, PlannedFragment, PlannedRegion, RegionRole, SemanticContent,
    SemanticNode, SemanticRole, StableId, TemplateLayout, TemplateLayoutRole, TemplateRegion,
    TypeChoice, validate_deck_plan, validate_deck_spec,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FontFace {
    pub family: String,
    pub face_index: u32,
    pub bytes: Arc<[u8]>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FontCatalog {
    /// Host-provided identity over every exact face available to this planning revision.
    pub identity: [u8; 32],
    pub default_family: Option<String>,
    pub faces: Vec<FontFace>,
}

impl FontCatalog {
    fn font(&self, family: &str) -> Option<&FontFace> {
        self.faces
            .iter()
            .find(|font| font.family.eq_ignore_ascii_case(family))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlannerLimits {
    pub max_font_faces: usize,
    pub max_font_bytes: usize,
    pub max_flow_units: usize,
    pub max_candidate_pages: usize,
    pub max_candidates_per_position: usize,
    pub max_measurements: usize,
    pub max_dynamic_states: usize,
}

impl Default for PlannerLimits {
    fn default() -> Self {
        Self {
            max_font_faces: 256,
            max_font_bytes: 128 * 1024 * 1024,
            max_flow_units: 100_000,
            max_candidate_pages: 250_000,
            max_candidates_per_position: 64,
            max_measurements: 1_000_000,
            max_dynamic_states: 100_000,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlannerPolicy {
    pub readable_floor: u32,
    pub font_step: u32,
    pub gap: Emu,
    pub limits: PlannerLimits,
}

impl Default for PlannerPolicy {
    fn default() -> Self {
        Self {
            readable_floor: 1_400,
            font_step: 100,
            gap: 114_300,
            limits: PlannerLimits::default(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlanError {
    pub code: DeckDiagnosticCode,
    pub diagnostics: Vec<DeckDiagnostic>,
}

impl fmt::Display for PlanError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = self
            .diagnostics
            .first()
            .map(|diagnostic| diagnostic.message.as_str())
            .unwrap_or("deck planning failed");
        formatter.write_str(message)
    }
}

impl std::error::Error for PlanError {}

#[derive(Clone, Debug, Default)]
pub struct DeckPlanner {
    policy: PlannerPolicy,
}

impl DeckPlanner {
    #[must_use]
    pub fn new(policy: PlannerPolicy) -> Self {
        Self { policy }
    }

    pub fn plan(
        &self,
        spec: &DeckSpec,
        template: &DeckTemplatePlan,
        fonts: &FontCatalog,
        contract_limits: &DeckLimits,
    ) -> Result<DeckPlan, PlanError> {
        let font_bytes = fonts
            .faces
            .iter()
            .map(|face| face.bytes.len())
            .try_fold(0usize, usize::checked_add);
        if fonts.faces.len() > self.policy.limits.max_font_faces
            || font_bytes.is_none_or(|bytes| bytes > self.policy.limits.max_font_bytes)
        {
            return Err(error(
                DeckDiagnosticCode::PLAN_WORK_LIMIT,
                "font catalog exceeds the configured planning bound",
                None,
            ));
        }
        let spec_report = validate_deck_spec(spec, contract_limits);
        if !spec_report.is_valid() {
            return Err(PlanError {
                code: DeckDiagnosticCode::INVALID_SEMANTIC_CONTENT,
                diagnostics: spec_report.diagnostics,
            });
        }
        if template
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == DiagnosticSeverity::Error)
        {
            return Err(error(
                DeckDiagnosticCode::PLAN_MISSING_LAYOUT,
                "cannot plan with an invalid template profile",
                None,
            ));
        }

        let mut diagnostics = Vec::new();
        let mut pages = Vec::new();
        let mut measurer = Measurer::new(fonts, spec, &self.policy.limits);
        for slide in &spec.logical_slides {
            let layout = select_layout(slide, template).ok_or_else(|| {
                error(
                    DeckDiagnosticCode::PLAN_MISSING_LAYOUT,
                    "template has no compatible layout for a logical slide",
                    Some(slide.id),
                )
            })?;
            let mut planned =
                self.plan_slide(slide, layout, template, &mut measurer, &mut diagnostics)?;
            pages.append(&mut planned);
        }
        if pages.len() > contract_limits.max_physical_pages {
            return Err(error(
                DeckDiagnosticCode::PLAN_WORK_LIMIT,
                "planned pages exceed the configured contract bound",
                None,
            ));
        }
        let mut plan = DeckPlan {
            id: plan_id(spec, template, fonts, &self.policy, contract_limits),
            spec_id: spec.id,
            template_id: template.id,
            page_size: template.page_size,
            pages,
            diagnostics,
        };
        let report = validate_deck_plan(spec, template, &plan, contract_limits);
        if !report.is_valid() {
            return Err(PlanError {
                code: report
                    .diagnostics
                    .first()
                    .map(|diagnostic| diagnostic.code)
                    .unwrap_or(DeckDiagnosticCode::PLAN_TARGET_DRIFT),
                diagnostics: report.diagnostics,
            });
        }
        plan.diagnostics.sort_by(|left, right| {
            (left.code.0, left.node_id, &left.message).cmp(&(
                right.code.0,
                right.node_id,
                &right.message,
            ))
        });
        plan.diagnostics.dedup();
        Ok(plan)
    }

    fn plan_slide(
        &self,
        slide: &LogicalSlide,
        layout: &TemplateLayout,
        template: &DeckTemplatePlan,
        measurer: &mut Measurer<'_>,
        diagnostics: &mut Vec<DeckDiagnostic>,
    ) -> Result<Vec<PhysicalPage>, PlanError> {
        let regions = layout_regions(layout, template);
        let (header_nodes, body_nodes) = split_headers(&slide.nodes, layout.role);
        let heading = header_nodes
            .iter()
            .find(|node| matches!(node.role, SemanticRole::Title | SemanticRole::Section))
            .map(|node| node.id);
        let header_placements =
            self.place_headers(&header_nodes, &regions, measurer, diagnostics)?;
        let units =
            build_flow(body_nodes, self.policy.limits.max_flow_units).map_err(|failure| {
                match failure {
                    FlowError::UnitLimit => error(
                        DeckDiagnosticCode::PLAN_WORK_LIMIT,
                        "semantic flow unit count exceeds the configured planning bound",
                        Some(slide.id),
                    ),
                }
            })?;
        let groups = group_units(&units);
        let primary = primary_region(layout.role, &regions).ok_or_else(|| {
            error(
                DeckDiagnosticCode::PLAN_MISSING_LAYOUT,
                "selected template layout has no usable primary region",
                Some(slide.id),
            )
        })?;
        let mut candidates = 0usize;
        let body_pages = if groups.is_empty() {
            Vec::new()
        } else {
            self.solve_pages(
                PaginationRequest {
                    groups: &groups,
                    region: primary,
                    slide_id: slide.id,
                },
                measurer,
                diagnostics,
                &mut candidates,
            )?
        };
        let total = body_pages.len().max(1);
        let mut pages = Vec::with_capacity(total);
        for ordinal in 1..=total {
            let body = body_pages.get(ordinal - 1).cloned().unwrap_or_default();
            let mut placements = if ordinal == 1 {
                header_placements.clone()
            } else {
                Vec::new()
            };
            placements.extend(body);
            pages.push(PhysicalPage {
                id: slide.id.derive(b"physical-page", ordinal as u32),
                logical_slide_id: slide.id,
                template_layout_id: layout.id,
                hidden: slide.hidden,
                continuation: wasmppt_deck::Continuation {
                    ordinal: ordinal as u32,
                    total: total as u32,
                    repeated_heading_node_id: (ordinal > 1).then_some(heading).flatten(),
                    label: (total > 1).then(|| format!("{ordinal}/{total}")),
                },
                regions: placements,
            });
        }
        Ok(pages)
    }

    fn place_headers(
        &self,
        nodes: &[&SemanticNode],
        regions: &[&TemplateRegion],
        measurer: &mut Measurer<'_>,
        diagnostics: &mut Vec<DeckDiagnostic>,
    ) -> Result<Vec<PlannedRegion>, PlanError> {
        let mut placements = Vec::new();
        for node in nodes {
            let Some(region) = compatible_region(node, regions) else {
                return Err(error(
                    DeckDiagnosticCode::PLAN_MISSING_LAYOUT,
                    "template layout has no compatible header region",
                    Some(node.id),
                ));
            };
            let measured = measurer
                .measure(node, FragmentSlice::Whole, region, region.frame, 0, 0)
                .map_err(|failure| measure_error(failure, node.id))?;
            if measured.height > region.frame.height {
                return Err(error(
                    DeckDiagnosticCode::PLAN_ATOMIC_OVERFLOW,
                    "header content exceeds its template frame at the readable floor",
                    Some(node.id),
                ));
            }
            if measured.font_risk {
                diagnostics.push(font_risk(node.id));
            }
            placements.push(planned_region(
                region,
                node,
                FragmentSlice::Whole,
                EmuRect {
                    height: measured.height,
                    ..region.frame
                },
                measured.font_size,
                0,
            ));
        }
        Ok(placements)
    }

    fn solve_pages(
        &self,
        request: PaginationRequest<'_, '_>,
        measurer: &mut Measurer<'_>,
        diagnostics: &mut Vec<DeckDiagnostic>,
        candidate_count: &mut usize,
    ) -> Result<Vec<Vec<PlannedRegion>>, PlanError> {
        if request.groups.len() > self.policy.limits.max_dynamic_states {
            return Err(error(
                DeckDiagnosticCode::PLAN_WORK_LIMIT,
                "pagination dynamic-state count exceeds the configured bound",
                Some(request.slide_id),
            ));
        }
        let mut best = vec![None::<Solution>; request.groups.len() + 1];
        best[request.groups.len()] = Some(Solution::default());
        for start in (0..request.groups.len()).rev() {
            let pages = self.candidate_pages(
                request.groups,
                start,
                request.region,
                measurer,
                diagnostics,
                candidate_count,
            )?;
            let mut selected = None::<Solution>;
            for page in pages {
                let Some(tail) = best[page.end].as_ref() else {
                    continue;
                };
                let candidate = tail.prepend(page);
                if selected
                    .as_ref()
                    .is_none_or(|current| candidate.score.cmp(&current.score) == Ordering::Less)
                {
                    selected = Some(candidate);
                }
            }
            best[start] = selected;
        }
        best[0]
            .take()
            .map(|solution| solution.pages)
            .ok_or_else(|| {
                error(
                    DeckDiagnosticCode::PLAN_ATOMIC_OVERFLOW,
                    "atomic content cannot fit any legal candidate at the readable floor",
                    Some(request.slide_id),
                )
            })
    }

    fn candidate_pages(
        &self,
        groups: &[FlowGroup<'_>],
        start: usize,
        region: &TemplateRegion,
        measurer: &mut Measurer<'_>,
        diagnostics: &mut Vec<DeckDiagnostic>,
        candidate_count: &mut usize,
    ) -> Result<Vec<CandidatePage>, PlanError> {
        let mut pages = Vec::new();
        for pattern in Pattern::ALL {
            if pattern.requires_dominant() && !groups[start].is_dominant() {
                continue;
            }
            let frames = pattern.frames(region.frame, self.policy.gap);
            let mut lane = 0usize;
            let mut y = frames[0].y;
            let mut placements = Vec::new();
            let mut font_cost = 0u64;
            for (offset, group) in groups[start..].iter().enumerate() {
                let repeat_table_header_rows = repeated_table_header_rows(group, &placements);
                let mut fitted = self.fit_group(
                    group,
                    &FitTarget {
                        region,
                        frames: &frames,
                        lane,
                        y,
                        repeat_table_header_rows,
                    },
                    measurer,
                )?;
                if fitted.is_none() {
                    lane += 1;
                    let Some(frame) = frames.get(lane) else {
                        break;
                    };
                    y = frame.y;
                    fitted = self.fit_group(
                        group,
                        &FitTarget {
                            region,
                            frames: &frames,
                            lane,
                            y,
                            repeat_table_header_rows,
                        },
                        measurer,
                    )?;
                }
                let Some(fitted) = fitted else {
                    break;
                };
                for placement in &fitted.placements {
                    if placement.font_risk {
                        diagnostics.push(font_risk(placement.node.id));
                    }
                    font_cost = font_cost.saturating_add(u64::from(placement.reduction));
                    placements.push(planned_region(
                        region,
                        placement.node,
                        placement.slice,
                        placement.frame,
                        placement.font_size,
                        placement.repeat_table_header_rows,
                    ));
                }
                y = fitted.bottom.saturating_add(self.policy.gap);
                let end = start + offset + 1;
                let page = CandidatePage {
                    end,
                    cost: page_cost(pattern, &frames, &placements, font_cost),
                    placements: placements.clone(),
                };
                pages.push(page);
                *candidate_count = candidate_count.saturating_add(1);
                if *candidate_count > self.policy.limits.max_candidate_pages {
                    return Err(error(
                        DeckDiagnosticCode::PLAN_WORK_LIMIT,
                        "candidate page count exceeds the configured planning bound",
                        None,
                    ));
                }
                if pages.len() == self.policy.limits.max_candidates_per_position {
                    break;
                }
            }
            if pages.len() == self.policy.limits.max_candidates_per_position {
                break;
            }
        }
        pages.sort_by_key(|page| (page.end, page.cost));
        pages.dedup_by(|left, right| left.end == right.end && left.cost == right.cost);
        Ok(pages)
    }

    fn fit_group<'a>(
        &self,
        group: &FlowGroup<'a>,
        target: &FitTarget<'_>,
        measurer: &mut Measurer<'_>,
    ) -> Result<Option<FittedGroup<'a>>, PlanError> {
        let Some(lane_frame) = target.frames.get(target.lane).copied() else {
            return Ok(None);
        };
        let initial_size = target
            .region
            .text_levels
            .first()
            .and_then(|level| level.font_size)
            .unwrap_or(2_000)
            .max(self.policy.readable_floor);
        let mut font_size = initial_size;
        loop {
            let mut cursor = target.y;
            let mut placements = Vec::new();
            let mut overflow = false;
            for unit in &group.units {
                let frame = EmuRect {
                    x: lane_frame.x,
                    y: cursor,
                    width: lane_frame.width,
                    height: lane_frame
                        .y
                        .saturating_add(lane_frame.height)
                        .saturating_sub(cursor),
                };
                let measured = measurer
                    .measure(
                        unit.node,
                        unit.slice,
                        target.region,
                        frame,
                        font_size,
                        target.repeat_table_header_rows,
                    )
                    .map_err(|failure| measure_error(failure, unit.node.id))?;
                let fragment_frame = EmuRect {
                    height: measured.height,
                    ..frame
                };
                if !fragment_frame.is_within(lane_frame) {
                    overflow = true;
                    break;
                }
                placements.push(FittedPlacement {
                    node: unit.node,
                    slice: unit.slice,
                    frame: fragment_frame,
                    font_size: measured.font_size,
                    reduction: initial_size.saturating_sub(measured.font_size),
                    font_risk: measured.font_risk,
                    repeat_table_header_rows: target.repeat_table_header_rows,
                });
                cursor = cursor
                    .saturating_add(measured.height)
                    .saturating_add(self.policy.gap);
            }
            if !overflow {
                return Ok(Some(FittedGroup {
                    placements,
                    bottom: cursor.saturating_sub(self.policy.gap),
                }));
            }
            if font_size <= self.policy.readable_floor {
                return Ok(None);
            }
            font_size = font_size
                .saturating_sub(self.policy.font_step.max(1))
                .max(self.policy.readable_floor);
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum Pattern {
    Stack,
    BalancedColumns,
    WeightedSplit,
    PeerGrid,
    LeadSupporting,
    Dominant,
}

impl Pattern {
    const ALL: [Self; 6] = [
        Self::Stack,
        Self::BalancedColumns,
        Self::WeightedSplit,
        Self::PeerGrid,
        Self::LeadSupporting,
        Self::Dominant,
    ];

    fn requires_dominant(self) -> bool {
        matches!(self, Self::Dominant)
    }

    fn frames(self, frame: EmuRect, gap: Emu) -> Vec<EmuRect> {
        match self {
            Self::Stack => vec![frame],
            Self::BalancedColumns => columns(frame, gap, &[1, 1]),
            Self::WeightedSplit => columns(frame, gap, &[3, 2]),
            Self::PeerGrid => {
                let rows = rows(frame, gap, &[1, 1]);
                rows.into_iter()
                    .flat_map(|row| columns(row, gap, &[1, 1]))
                    .collect()
            }
            Self::LeadSupporting => {
                let rows = rows(frame, gap, &[3, 2]);
                let mut frames = vec![rows[0]];
                frames.extend(columns(rows[1], gap, &[1, 1]));
                frames
            }
            Self::Dominant => columns(frame, gap, &[2, 1]),
        }
    }

    const fn complexity(self) -> u64 {
        match self {
            Self::Stack => 0,
            Self::BalancedColumns => 10,
            Self::WeightedSplit => 20,
            Self::Dominant => 25,
            Self::LeadSupporting => 30,
            Self::PeerGrid => 40,
        }
    }
}

fn columns(frame: EmuRect, gap: Emu, weights: &[i64]) -> Vec<EmuRect> {
    divide(frame, gap, weights, true)
}

fn rows(frame: EmuRect, gap: Emu, weights: &[i64]) -> Vec<EmuRect> {
    divide(frame, gap, weights, false)
}

fn divide(frame: EmuRect, gap: Emu, weights: &[i64], horizontal: bool) -> Vec<EmuRect> {
    let gaps = gap.saturating_mul(weights.len().saturating_sub(1) as i64);
    let available = if horizontal {
        frame.width
    } else {
        frame.height
    }
    .saturating_sub(gaps);
    let total = weights.iter().sum::<i64>().max(1);
    let mut cursor = if horizontal { frame.x } else { frame.y };
    weights
        .iter()
        .enumerate()
        .map(|(index, weight)| {
            let extent = if index + 1 == weights.len() {
                let end = if horizontal {
                    frame.x.saturating_add(frame.width)
                } else {
                    frame.y.saturating_add(frame.height)
                };
                end.saturating_sub(cursor)
            } else {
                available.saturating_mul(*weight) / total
            };
            let output = if horizontal {
                EmuRect {
                    x: cursor,
                    y: frame.y,
                    width: extent,
                    height: frame.height,
                }
            } else {
                EmuRect {
                    x: frame.x,
                    y: cursor,
                    width: frame.width,
                    height: extent,
                }
            };
            cursor = cursor.saturating_add(extent).saturating_add(gap);
            output
        })
        .collect()
}

#[derive(Clone)]
struct FlowGroup<'a> {
    units: Vec<&'a FlowUnit<'a>>,
}

struct PaginationRequest<'groups, 'nodes> {
    groups: &'groups [FlowGroup<'nodes>],
    region: &'groups TemplateRegion,
    slide_id: StableId,
}

struct FitTarget<'a> {
    region: &'a TemplateRegion,
    frames: &'a [EmuRect],
    lane: usize,
    y: Emu,
    repeat_table_header_rows: u32,
}

impl FlowGroup<'_> {
    fn is_dominant(&self) -> bool {
        self.units.first().is_some_and(|unit| {
            matches!(
                unit.node.role,
                SemanticRole::Figure
                    | SemanticRole::Gallery
                    | SemanticRole::Table
                    | SemanticRole::Chart
                    | SemanticRole::Diagram
                    | SemanticRole::DisplayMath
            )
        })
    }
}

fn group_units<'a>(units: &'a [FlowUnit<'a>]) -> Vec<FlowGroup<'a>> {
    let mut groups = Vec::<FlowGroup<'a>>::new();
    for unit in units {
        if let Some(group) = groups.last_mut() {
            if group.units[0].group == unit.group {
                group.units.push(unit);
                continue;
            }
        }
        groups.push(FlowGroup { units: vec![unit] });
    }
    groups
}

struct FittedPlacement<'a> {
    node: &'a SemanticNode,
    slice: FragmentSlice,
    frame: EmuRect,
    font_size: u32,
    reduction: u32,
    font_risk: bool,
    repeat_table_header_rows: u32,
}

struct FittedGroup<'a> {
    placements: Vec<FittedPlacement<'a>>,
    bottom: Emu,
}

#[derive(Clone)]
struct CandidatePage {
    end: usize,
    cost: u64,
    placements: Vec<PlannedRegion>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct Score {
    pages: usize,
    cost: u64,
}

impl Ord for Score {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.pages, self.cost).cmp(&(other.pages, other.cost))
    }
}

impl PartialOrd for Score {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Clone, Default)]
struct Solution {
    score: Score,
    pages: Vec<Vec<PlannedRegion>>,
}

impl Solution {
    fn prepend(&self, page: CandidatePage) -> Self {
        let mut pages = Vec::with_capacity(self.pages.len() + 1);
        pages.push(page.placements);
        pages.extend(self.pages.clone());
        Self {
            score: Score {
                pages: self.score.pages + 1,
                cost: self.score.cost.saturating_add(page.cost),
            },
            pages,
        }
    }
}

fn page_cost(
    pattern: Pattern,
    frames: &[EmuRect],
    placements: &[PlannedRegion],
    font_cost: u64,
) -> u64 {
    let used = placements
        .iter()
        .map(|placement| placement.frame.width.saturating_mul(placement.frame.height))
        .fold(0i64, i64::saturating_add)
        .max(0) as u64;
    let available = frames
        .iter()
        .map(|frame| frame.width.saturating_mul(frame.height))
        .fold(0i64, i64::saturating_add)
        .max(1) as u64;
    let whitespace = available.saturating_sub(used).saturating_mul(1_000) / available;
    let whitespace_imbalance = whitespace.saturating_mul(whitespace);
    let narrow = frames
        .iter()
        .filter(|frame| frame.width.saturating_mul(4) < frame.height.saturating_mul(3))
        .count() as u64
        * 100;
    let orphaning = placements
        .first()
        .filter(|_| placements.len() == 1)
        .and_then(|placement| placement.fragments.first())
        .filter(|fragment| matches!(fragment.slice, FragmentSlice::Text { .. }))
        .map_or(0, |_| 250);
    font_cost
        .saturating_mul(10)
        .saturating_add(whitespace_imbalance)
        .saturating_add(narrow)
        .saturating_add(orphaning)
        .saturating_add(pattern.complexity())
}

fn repeated_table_header_rows(group: &FlowGroup<'_>, placements: &[PlannedRegion]) -> u32 {
    let Some(unit) = group.units.first() else {
        return 0;
    };
    let (SemanticContent::Table(table), FragmentSlice::TableRows { start, .. }) =
        (&unit.node.content, unit.slice)
    else {
        return 0;
    };
    let already_on_page = placements
        .iter()
        .flat_map(|region| &region.fragments)
        .any(|fragment| fragment.source_node_id == unit.node.id);
    if !already_on_page && table.header_rows > 0 && start >= table.header_rows {
        table.header_rows
    } else {
        0
    }
}

fn select_layout<'a>(
    slide: &LogicalSlide,
    template: &'a DeckTemplatePlan,
) -> Option<&'a TemplateLayout> {
    let role = if slide.nodes.iter().any(|node| {
        matches!(
            node.role,
            SemanticRole::Statement | SemanticRole::Quote | SemanticRole::DisplayMath
        )
    }) {
        TemplateLayoutRole::Statement
    } else {
        match slide.kind {
            LogicalSlideKind::Title => TemplateLayoutRole::Title,
            LogicalSlideKind::Content => TemplateLayoutRole::Content,
        }
    };
    template.layouts.iter().find(|layout| layout.role == role)
}

fn layout_regions<'a>(
    layout: &TemplateLayout,
    template: &'a DeckTemplatePlan,
) -> Vec<&'a TemplateRegion> {
    let by_id = template
        .regions
        .iter()
        .map(|region| (region.id, region))
        .collect::<BTreeMap<_, _>>();
    layout
        .region_ids
        .iter()
        .filter_map(|id| by_id.get(id).copied())
        .collect()
}

fn split_headers(
    nodes: &[SemanticNode],
    layout: TemplateLayoutRole,
) -> (Vec<&SemanticNode>, &[SemanticNode]) {
    let header_count = nodes
        .iter()
        .take_while(|node| match layout {
            TemplateLayoutRole::Title => {
                matches!(node.role, SemanticRole::Title | SemanticRole::Subtitle)
            }
            TemplateLayoutRole::Content => {
                matches!(node.role, SemanticRole::Title | SemanticRole::Section)
            }
            TemplateLayoutRole::Statement => false,
        })
        .count();
    (
        nodes[..header_count].iter().collect(),
        &nodes[header_count..],
    )
}

fn primary_region<'a>(
    layout: TemplateLayoutRole,
    regions: &[&'a TemplateRegion],
) -> Option<&'a TemplateRegion> {
    let preferred = match layout {
        TemplateLayoutRole::Title => RegionRole::Subtitle,
        TemplateLayoutRole::Content => RegionRole::Body,
        TemplateLayoutRole::Statement => RegionRole::Statement,
    };
    regions
        .iter()
        .copied()
        .find(|region| region.role == preferred)
        .or_else(|| {
            regions
                .iter()
                .copied()
                .find(|region| !region.accepts.is_empty())
        })
}

fn compatible_region<'a>(
    node: &SemanticNode,
    regions: &[&'a TemplateRegion],
) -> Option<&'a TemplateRegion> {
    let preferred = match node.role {
        SemanticRole::Title | SemanticRole::Section => RegionRole::Title,
        SemanticRole::Subtitle => RegionRole::Subtitle,
        SemanticRole::Statement | SemanticRole::Quote | SemanticRole::DisplayMath => {
            RegionRole::Statement
        }
        SemanticRole::Figure | SemanticRole::Gallery | SemanticRole::Diagram => RegionRole::Media,
        SemanticRole::Caption | SemanticRole::Credit => RegionRole::Caption,
        SemanticRole::Table => RegionRole::Table,
        SemanticRole::Chart => RegionRole::Chart,
        SemanticRole::Code => RegionRole::Code,
        _ => RegionRole::Body,
    };
    regions
        .iter()
        .copied()
        .find(|region| region.role == preferred && region.accepts.contains(&node.role))
        .or_else(|| {
            regions
                .iter()
                .copied()
                .find(|region| region.accepts.contains(&node.role))
        })
}

fn planned_region(
    region: &TemplateRegion,
    node: &SemanticNode,
    slice: FragmentSlice,
    frame: EmuRect,
    font_size: u32,
    repeat_table_header_rows: u32,
) -> PlannedRegion {
    PlannedRegion {
        template_region_id: region.id,
        frame,
        fragments: vec![PlannedFragment {
            id: PlannedFragment::expected_id(node.id, slice),
            source_node_id: node.id,
            slice,
            frame,
            type_choice: TypeChoice {
                font_size: if is_text(node) { font_size } else { 0 },
                columns: 1,
                fit: if matches!(
                    node.content,
                    SemanticContent::Image(_) | SemanticContent::Svg(_) | SemanticContent::Chart(_)
                ) {
                    ContentFit::Contain
                } else {
                    ContentFit::None
                },
            },
            repeat_table_header_rows,
        }],
    }
}

fn is_text(node: &SemanticNode) -> bool {
    matches!(
        node.content,
        SemanticContent::Text(_)
            | SemanticContent::List(_)
            | SemanticContent::Table(_)
            | SemanticContent::Code(_)
    )
}

fn font_risk(node: StableId) -> DeckDiagnostic {
    DeckDiagnostic {
        code: DeckDiagnosticCode::PLAN_FONT_RISK,
        severity: DiagnosticSeverity::Warning,
        message: "exact font bytes are unavailable; layout used observable fallback metrics"
            .to_owned(),
        source: None,
        node_id: Some(node),
        page_id: None,
    }
}

fn measure_error(failure: MeasureError, node: StableId) -> PlanError {
    match failure {
        MeasureError::WorkLimit => error(
            DeckDiagnosticCode::PLAN_WORK_LIMIT,
            "font measurement count exceeds the configured planning bound",
            Some(node),
        ),
    }
}

fn error(code: DeckDiagnosticCode, message: &str, node_id: Option<StableId>) -> PlanError {
    PlanError {
        code,
        diagnostics: vec![DeckDiagnostic {
            code,
            severity: DiagnosticSeverity::Error,
            message: message.to_owned(),
            source: None,
            node_id,
            page_id: None,
        }],
    }
}

fn plan_id(
    spec: &DeckSpec,
    template: &DeckTemplatePlan,
    fonts: &FontCatalog,
    policy: &PlannerPolicy,
    limits: &DeckLimits,
) -> StableId {
    let mut digest = Sha256::new();
    digest.update(b"wasmppt/deck-layout/plan/v1\0");
    match spec.encode(limits) {
        Ok(encoded) => digest.update(Sha256::digest(encoded)),
        Err(_) => digest.update(spec.id.as_bytes()),
    }
    digest.update(template.id.as_bytes());
    digest.update(template.cache_key);
    digest.update(fonts.identity);
    digest.update(policy.readable_floor.to_le_bytes());
    digest.update(policy.font_step.to_le_bytes());
    digest.update(policy.gap.to_le_bytes());
    digest.update((policy.limits.max_font_faces as u64).to_le_bytes());
    digest.update((policy.limits.max_font_bytes as u64).to_le_bytes());
    digest.update((policy.limits.max_flow_units as u64).to_le_bytes());
    digest.update((policy.limits.max_candidate_pages as u64).to_le_bytes());
    digest.update((policy.limits.max_candidates_per_position as u64).to_le_bytes());
    digest.update((policy.limits.max_measurements as u64).to_le_bytes());
    digest.update((policy.limits.max_dynamic_states as u64).to_le_bytes());
    let bytes = digest.finalize();
    let mut id = [0; 16];
    id.copy_from_slice(&bytes[..16]);
    StableId::from_bytes(id)
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;
    use wasmppt_deck::{
        DeckResource, ImageContent, PixelSize, PlaceholderIdentity, ResourceKind, RichText,
        RichTextRun, SourceRange, SplitPolicy, TableCell, TableColumn, TableContent, TableRow,
        TemplateTextLevel, TemplateTheme, TextMargins, TextMarks,
    };

    use super::*;

    const PAGE: wasmppt_deck::EmuSize = wasmppt_deck::EmuSize {
        width: 10_000_000,
        height: 7_500_000,
    };

    #[test]
    fn planning_is_deterministic_and_preserves_exact_source_coverage() {
        let spec = spec(vec![
            text_node(
                3,
                SemanticRole::Title,
                SplitPolicy::Never,
                "A stable heading",
            ),
            text_node(
                4,
                SemanticRole::Prose,
                SplitPolicy::Text,
                "First sentence. Second sentence. Third sentence. Fourth sentence.",
            ),
            code_node(5, "one\ntwo\nthree\nfour\n"),
        ]);
        let template = template(850_000);
        let planner = DeckPlanner::default();
        let first = planner
            .plan(&spec, &template, &FontCatalog::default(), &limits())
            .unwrap();
        let second = planner
            .plan(&spec, &template, &FontCatalog::default(), &limits())
            .unwrap();

        assert_eq!(first, second);
        assert!(validate_deck_plan(&spec, &template, &first, &limits()).is_valid());
        assert!(first.pages.len() > 1);
        assert_eq!(
            first.pages[1].continuation.repeated_heading_node_id,
            Some(id(3))
        );
        assert_eq!(
            first.pages[1].continuation.label,
            Some(format!("2/{}", first.pages.len()))
        );
        assert!(first.pages.iter().flat_map(fragments).all(|fragment| {
            fragment.type_choice.font_size == 0 || fragment.type_choice.font_size >= 1_400
        }));
        assert!(
            first
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == DeckDiagnosticCode::PLAN_FONT_RISK)
        );
    }

    #[test]
    fn authored_figure_caption_relation_stays_on_one_page() {
        let spec = spec_with_resources(
            vec![
                text_node(3, SemanticRole::Title, SplitPolicy::Never, "Heading"),
                SemanticNode {
                    id: id(4),
                    source: range(40),
                    role: SemanticRole::Figure,
                    split: SplitPolicy::Never,
                    content: SemanticContent::Image(ImageContent {
                        resource_id: id(90),
                        alt_text: "wide figure".to_owned(),
                    }),
                },
                text_node(
                    5,
                    SemanticRole::Caption,
                    SplitPolicy::Text,
                    "Figure caption.",
                ),
            ],
            vec![DeckResource {
                id: id(90),
                kind: ResourceKind::RasterImage,
                media_type: "image/png".to_owned(),
                bytes: vec![1],
                intrinsic_size: Some(PixelSize {
                    width: 2_000,
                    height: 100,
                }),
            }],
        );
        let plan = DeckPlanner::default()
            .plan(
                &spec,
                &template(1_000_000),
                &FontCatalog::default(),
                &limits(),
            )
            .unwrap();

        let figure_page = page_containing(&plan, id(4));
        let caption_page = page_containing(&plan, id(5));
        assert_eq!(figure_page, caption_page);
    }

    #[test]
    fn repeated_table_headers_are_metadata_not_duplicate_source_rows() {
        let spec = spec(vec![
            text_node(3, SemanticRole::Title, SplitPolicy::Never, "Heading"),
            table_node(4, 6, 1),
        ]);
        let template = template(1_300_000);
        let plan = DeckPlanner::default()
            .plan(&spec, &template, &FontCatalog::default(), &limits())
            .unwrap();

        assert!(plan.pages.len() > 1);
        let table_fragments = plan
            .pages
            .iter()
            .flat_map(fragments)
            .filter(|fragment| fragment.source_node_id == id(4))
            .collect::<Vec<_>>();
        assert_eq!(table_fragments.len(), 6);
        assert_eq!(table_fragments[0].repeat_table_header_rows, 0);
        assert!(
            table_fragments[1..]
                .iter()
                .any(|fragment| fragment.repeat_table_header_rows == 1)
        );
        assert!(validate_deck_plan(&spec, &template, &plan, &limits()).is_valid());
    }

    #[test]
    fn atomic_content_that_cannot_fit_fails_at_the_readable_floor() {
        let spec = spec_with_resources(
            vec![
                text_node(3, SemanticRole::Title, SplitPolicy::Never, "Heading"),
                SemanticNode {
                    id: id(4),
                    source: range(40),
                    role: SemanticRole::Figure,
                    split: SplitPolicy::Never,
                    content: SemanticContent::Image(ImageContent {
                        resource_id: id(90),
                        alt_text: "tall figure".to_owned(),
                    }),
                },
            ],
            vec![DeckResource {
                id: id(90),
                kind: ResourceKind::RasterImage,
                media_type: "image/png".to_owned(),
                bytes: vec![1],
                intrinsic_size: Some(PixelSize {
                    width: 100,
                    height: 2_000,
                }),
            }],
        );
        let failure = DeckPlanner::default()
            .plan(
                &spec,
                &template(650_000),
                &FontCatalog::default(),
                &limits(),
            )
            .unwrap_err();

        assert_eq!(failure.code, DeckDiagnosticCode::PLAN_ATOMIC_OVERFLOW);
    }

    #[test]
    fn flow_and_candidate_work_are_explicitly_bounded() {
        let mut policy = PlannerPolicy::default();
        policy.limits.max_flow_units = 2;
        let planner = DeckPlanner::new(policy);
        let spec = spec(vec![
            text_node(3, SemanticRole::Title, SplitPolicy::Never, "Heading"),
            code_node(4, "one\ntwo\nthree\n"),
        ]);
        let failure = planner
            .plan(
                &spec,
                &template(1_000_000),
                &FontCatalog::default(),
                &limits(),
            )
            .unwrap_err();

        assert_eq!(failure.code, DeckDiagnosticCode::PLAN_WORK_LIMIT);
    }

    #[test]
    fn plan_identity_changes_with_source_template_font_and_policy_mutations() {
        let spec = spec(vec![
            text_node(3, SemanticRole::Title, SplitPolicy::Never, "Heading"),
            text_node(4, SemanticRole::Prose, SplitPolicy::Text, "Body."),
        ]);
        let template = template(1_000_000);
        let baseline = DeckPlanner::default()
            .plan(&spec, &template, &FontCatalog::default(), &limits())
            .unwrap();

        let mut changed_spec = spec.clone();
        let SemanticContent::Text(text) = &mut changed_spec.logical_slides[0].nodes[1].content
        else {
            unreachable!();
        };
        text.runs[0].text.push_str(" Changed.");
        let source_id = DeckPlanner::default()
            .plan(&changed_spec, &template, &FontCatalog::default(), &limits())
            .unwrap()
            .id;

        let mut changed_template = template.clone();
        changed_template.cache_key[0] ^= 1;
        let template_id = DeckPlanner::default()
            .plan(&spec, &changed_template, &FontCatalog::default(), &limits())
            .unwrap()
            .id;

        let font_id = DeckPlanner::default()
            .plan(
                &spec,
                &template,
                &FontCatalog {
                    identity: [1; 32],
                    ..FontCatalog::default()
                },
                &limits(),
            )
            .unwrap()
            .id;

        let mut policy = PlannerPolicy::default();
        policy.gap += 1;
        let policy_id = DeckPlanner::new(policy)
            .plan(&spec, &template, &FontCatalog::default(), &limits())
            .unwrap()
            .id;

        assert_ne!(baseline.id, source_id);
        assert_ne!(baseline.id, template_id);
        assert_ne!(baseline.id, font_id);
        assert_ne!(baseline.id, policy_id);
    }

    proptest! {
        #[test]
        fn arbitrary_code_line_counts_terminate_with_a_valid_plan_or_work_limit(line_count in 1usize..40) {
            let text = (0..line_count).map(|line| format!("line-{line}\n")).collect::<String>();
            let spec = spec(vec![
                text_node(3, SemanticRole::Title, SplitPolicy::Never, "Heading"),
                code_node(4, &text),
            ]);
            let mut policy = PlannerPolicy::default();
            policy.limits.max_flow_units = 128;
            policy.limits.max_candidate_pages = 64;
            let result = DeckPlanner::new(policy).plan(
                &spec,
                &template(700_000),
                &FontCatalog::default(),
                &limits(),
            );
            match result {
                Ok(plan) => prop_assert!(validate_deck_plan(&spec, &template(700_000), &plan, &limits()).is_valid()),
                Err(failure) => prop_assert_eq!(failure.code, DeckDiagnosticCode::PLAN_WORK_LIMIT),
            }
        }
    }

    fn spec(nodes: Vec<SemanticNode>) -> DeckSpec {
        spec_with_resources(nodes, vec![])
    }

    fn spec_with_resources(nodes: Vec<SemanticNode>, resources: Vec<DeckResource>) -> DeckSpec {
        DeckSpec {
            id: id(1),
            logical_slides: vec![LogicalSlide {
                id: id(2),
                source: SourceRange::new("deck.md", 0, 10_000),
                kind: LogicalSlideKind::Content,
                hidden: false,
                nodes,
            }],
            resources,
        }
    }

    fn text_node(identity: u8, role: SemanticRole, split: SplitPolicy, text: &str) -> SemanticNode {
        SemanticNode {
            id: id(identity),
            source: range(u32::from(identity) * 10),
            role,
            split,
            content: SemanticContent::Text(RichText {
                runs: vec![RichTextRun {
                    text: text.to_owned(),
                    marks: TextMarks::default(),
                    hyperlink: None,
                }],
            }),
        }
    }

    fn code_node(identity: u8, code: &str) -> SemanticNode {
        SemanticNode {
            id: id(identity),
            source: range(u32::from(identity) * 10),
            role: SemanticRole::Code,
            split: SplitPolicy::CodeLines,
            content: SemanticContent::Code(wasmppt_deck::CodeContent {
                language: None,
                code: code.to_owned(),
            }),
        }
    }

    fn table_node(identity: u8, rows: u8, header_rows: u32) -> SemanticNode {
        let source = range(u32::from(identity) * 10);
        SemanticNode {
            id: id(identity),
            source: source.clone(),
            role: SemanticRole::Table,
            split: SplitPolicy::TableRows,
            content: SemanticContent::Table(TableContent {
                columns: vec![TableColumn {
                    id: id(40),
                    source: source.clone(),
                }],
                header_rows,
                rows: (0..rows)
                    .map(|row| TableRow {
                        id: id(50 + row),
                        source: source.clone(),
                        cells: vec![TableCell {
                            id: id(60 + row),
                            source: source.clone(),
                            content: RichText {
                                runs: vec![RichTextRun {
                                    text: format!("row {row}"),
                                    marks: TextMarks::default(),
                                    hyperlink: None,
                                }],
                            },
                        }],
                    })
                    .collect(),
            }),
        }
    }

    fn template(body_height: Emu) -> DeckTemplatePlan {
        let layout_id = id(100);
        let title_id = id(101);
        let body_id = id(102);
        DeckTemplatePlan {
            id: id(99),
            template_hash: [7; 32],
            cache_key: [8; 32],
            validator_version: 1,
            compiler_policy: "test".to_owned(),
            page_size: PAGE,
            theme: TemplateTheme::default(),
            layouts: vec![TemplateLayout {
                id: layout_id,
                role: TemplateLayoutRole::Content,
                matching_name: "content".to_owned(),
                source_part: "ppt/slideLayouts/slideLayout1.xml".to_owned(),
                master_part: "ppt/slideMasters/slideMaster1.xml".to_owned(),
                region_ids: vec![title_id, body_id],
                asset_ids: vec![],
                background: None,
            }],
            regions: vec![
                TemplateRegion {
                    id: title_id,
                    layout_id,
                    role: RegionRole::Title,
                    placeholder: PlaceholderIdentity {
                        kind: "title".to_owned(),
                        index: 0,
                    },
                    frame: EmuRect {
                        x: 500_000,
                        y: 200_000,
                        width: 9_000_000,
                        height: 700_000,
                    },
                    margins: TextMargins::default(),
                    text_levels: vec![text_level(2_800)],
                    accepts: vec![SemanticRole::Title, SemanticRole::Section],
                    required: true,
                },
                TemplateRegion {
                    id: body_id,
                    layout_id,
                    role: RegionRole::Body,
                    placeholder: PlaceholderIdentity {
                        kind: "body".to_owned(),
                        index: 1,
                    },
                    frame: EmuRect {
                        x: 500_000,
                        y: 1_100_000,
                        width: 9_000_000,
                        height: body_height,
                    },
                    margins: TextMargins::default(),
                    text_levels: vec![text_level(2_000)],
                    accepts: vec![
                        SemanticRole::Prose,
                        SemanticRole::List,
                        SemanticRole::Table,
                        SemanticRole::Code,
                        SemanticRole::Figure,
                        SemanticRole::Caption,
                        SemanticRole::Gallery,
                        SemanticRole::Chart,
                        SemanticRole::Diagram,
                        SemanticRole::DisplayMath,
                        SemanticRole::Quote,
                        SemanticRole::Credit,
                        SemanticRole::Definition,
                        SemanticRole::DefinitionTerm,
                        SemanticRole::DefinitionDescription,
                        SemanticRole::Statement,
                    ],
                    required: true,
                },
            ],
            assets: vec![],
            diagnostics: vec![],
        }
    }

    fn text_level(font_size: u32) -> TemplateTextLevel {
        TemplateTextLevel {
            level: 0,
            font_size: Some(font_size),
            ..TemplateTextLevel::default()
        }
    }

    fn limits() -> DeckLimits {
        DeckLimits {
            max_physical_pages: 1_000,
            max_planned_fragments: 10_000,
            ..DeckLimits::default()
        }
    }

    fn page_containing(plan: &DeckPlan, node: StableId) -> StableId {
        plan.pages
            .iter()
            .find(|page| fragments(page).any(|fragment| fragment.source_node_id == node))
            .map(|page| page.id)
            .unwrap()
    }

    fn fragments(page: &PhysicalPage) -> impl Iterator<Item = &PlannedFragment> {
        page.regions
            .iter()
            .flat_map(|region| region.fragments.iter())
    }

    fn range(start: u32) -> SourceRange {
        SourceRange::new("deck.md", start, start + 9)
    }

    fn id(value: u8) -> StableId {
        StableId::from_bytes([value; 16])
    }
}
