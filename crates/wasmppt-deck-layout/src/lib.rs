//! Bounded, deterministic semantic layout and automatic pagination.
//!
//! The planner evaluates a small generic candidate family over template-owned frames. It never
//! depends on a DOM, host font APIs, or a product slide-count limit.

mod flow;
mod measure;

use std::{
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet},
    fmt,
    sync::Arc,
};

use flow::{FlowError, FlowUnit, build_flow};
use measure::{MeasureError, Measurer};
use sha2::{Digest, Sha256};
use wasmppt_deck::{
    ContentFit, DeckDiagnostic, DeckDiagnosticCode, DeckLimits, DeckPlan, DeckResource, DeckSpec,
    DeckTemplatePlan, DiagnosticSeverity, Emu, EmuRect, FragmentSlice, LayoutTopology,
    LogicalSlide, LogicalSlideKind, MediaPlacement, MediaTextProximity, MediaTextRelation,
    MediaTextSide, PhysicalPage, PixelSize, PlannedFragment, PlannedRegion, RegionPlacement,
    RegionRole, SemanticContent, SemanticNode, SemanticRole, StableId, TemplateLayout,
    TemplateLayoutCapability, TemplateRegion, TopologyChoice, TypeChoice, validate_deck_plan,
    validate_deck_spec,
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
    pub max_candidate_assignments: usize,
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
            max_candidate_assignments: 250_000,
            max_candidates_per_position: 64,
            max_measurements: 1_000_000,
            max_dynamic_states: 100_000,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlannerPolicy {
    pub readable_floor: u32,
    pub readable_media_floor: Emu,
    /// Maximum centered-cover source loss, in thousandths of the source area.
    pub max_cover_crop_per_mille: u16,
    pub font_step: u32,
    pub gap: Emu,
    pub limits: PlannerLimits,
}

impl Default for PlannerPolicy {
    fn default() -> Self {
        Self {
            readable_floor: 1_400,
            readable_media_floor: 1_200_000,
            max_cover_crop_per_mille: 300,
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

/// Exact invalidation metadata produced by an incremental planning pass.
#[derive(Clone, Debug, PartialEq)]
pub struct IncrementalPlanUpdate {
    pub plan: DeckPlan,
    pub invalidated_logical_slides: Vec<StableId>,
    pub invalidated_previous_pages: Vec<StableId>,
    pub invalidated_pages: Vec<StableId>,
    pub reused_pages: usize,
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

    /// Replan only logical slides whose semantic content or referenced resources changed.
    ///
    /// Reuse is valid only when the caller supplies the same template plan, font catalog,
    /// planner policy, and contract limits that produced `previous_plan`. A mismatch in any
    /// identity falls back to a complete plan so the fast path has an explicit boundary.
    pub fn replan(
        &self,
        previous_spec: &DeckSpec,
        previous_plan: &DeckPlan,
        next_spec: &DeckSpec,
        template: &DeckTemplatePlan,
        fonts: &FontCatalog,
        contract_limits: &DeckLimits,
    ) -> Result<IncrementalPlanUpdate, PlanError> {
        let expected_previous_id = plan_id(
            previous_spec,
            template,
            fonts,
            &self.policy,
            contract_limits,
        );
        if previous_plan.id != expected_previous_id
            || previous_plan.spec_id != previous_spec.id
            || previous_plan.template_id != template.id
        {
            let plan = self.plan(next_spec, template, fonts, contract_limits)?;
            return Ok(IncrementalPlanUpdate {
                invalidated_logical_slides: next_spec
                    .logical_slides
                    .iter()
                    .map(|slide| slide.id)
                    .collect(),
                invalidated_previous_pages: previous_plan
                    .pages
                    .iter()
                    .map(|page| page.id)
                    .collect(),
                invalidated_pages: plan.pages.iter().map(|page| page.id).collect(),
                reused_pages: 0,
                plan,
            });
        }

        validate_planning_inputs(next_spec, template, fonts, contract_limits, &self.policy)?;
        let previous_slides = previous_spec
            .logical_slides
            .iter()
            .map(|slide| (slide.id, slide))
            .collect::<BTreeMap<_, _>>();
        let previous_resources = previous_spec
            .resources
            .iter()
            .map(|resource| (resource.id, resource))
            .collect::<BTreeMap<_, _>>();
        let next_resources = next_spec
            .resources
            .iter()
            .map(|resource| (resource.id, resource))
            .collect::<BTreeMap<_, _>>();
        let previous_pages = previous_plan.pages.iter().fold(
            BTreeMap::<StableId, Vec<PhysicalPage>>::new(),
            |mut pages, page| {
                pages
                    .entry(page.logical_slide_id)
                    .or_default()
                    .push(page.clone());
                pages
            },
        );

        let mut diagnostics = Vec::new();
        let mut pages = Vec::new();
        let mut invalidated_logical_slides = Vec::new();
        let mut invalidated_previous_pages = Vec::new();
        let mut invalidated_pages = Vec::new();
        let mut reused_pages = 0usize;
        let mut measurer = Measurer::new(fonts, next_spec, &self.policy.limits);
        let mut reused_node_ids = BTreeSet::new();
        let mut reused_page_ids = BTreeSet::new();

        for slide in &next_spec.logical_slides {
            let can_reuse = previous_slides.get(&slide.id).is_some_and(|previous| {
                *previous == slide
                    && referenced_resources_equal(slide, &previous_resources, &next_resources)
            });
            if can_reuse {
                if let Some(previous) = previous_pages.get(&slide.id) {
                    reused_pages = reused_pages.saturating_add(previous.len());
                    reused_page_ids.extend(previous.iter().map(|page| page.id));
                    collect_node_ids(&slide.nodes, &mut reused_node_ids);
                    pages.extend(previous.iter().cloned());
                    continue;
                }
            }

            invalidated_logical_slides.push(slide.id);
            if let Some(previous) = previous_pages.get(&slide.id) {
                invalidated_previous_pages.extend(previous.iter().map(|page| page.id));
            }
            let layout = select_layout(slide, template).ok_or_else(|| {
                error(
                    DeckDiagnosticCode::PLAN_MISSING_LAYOUT,
                    "template has no compatible layout for a logical slide",
                    Some(slide.id),
                )
            })?;
            let planned =
                self.plan_slide(slide, layout, template, &mut measurer, &mut diagnostics)?;
            invalidated_pages.extend(planned.iter().map(|page| page.id));
            pages.extend(planned);
        }
        for (slide_id, previous) in previous_pages {
            if !next_spec
                .logical_slides
                .iter()
                .any(|slide| slide.id == slide_id)
            {
                invalidated_previous_pages.extend(previous.iter().map(|page| page.id));
            }
        }
        diagnostics.extend(
            previous_plan
                .diagnostics
                .iter()
                .filter(|diagnostic| {
                    diagnostic
                        .node_id
                        .is_some_and(|id| reused_node_ids.contains(&id))
                        || diagnostic
                            .page_id
                            .is_some_and(|id| reused_page_ids.contains(&id))
                })
                .cloned(),
        );
        let mut plan = DeckPlan {
            id: plan_id(next_spec, template, fonts, &self.policy, contract_limits),
            spec_id: next_spec.id,
            template_id: template.id,
            page_size: template.page_size,
            pages,
            diagnostics,
        };
        let report = validate_deck_plan(next_spec, template, &plan, contract_limits);
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
        invalidated_logical_slides.sort_unstable();
        invalidated_previous_pages.sort_unstable();
        invalidated_previous_pages.dedup();
        invalidated_pages.sort_unstable();
        invalidated_pages.dedup();
        Ok(IncrementalPlanUpdate {
            plan,
            invalidated_logical_slides,
            invalidated_previous_pages,
            invalidated_pages,
            reused_pages,
        })
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
        let (header_nodes, body_nodes) = split_headers(&slide.nodes, layout.capability);
        let heading = header_nodes
            .iter()
            .find(|node| matches!(node.role, SemanticRole::Title | SemanticRole::Section))
            .map(|node| node.id);
        let header_placements =
            self.place_headers(&header_nodes, &regions, measurer, diagnostics)?;
        let units = build_flow(
            body_nodes,
            &slide.media_text_relations,
            self.policy.limits.max_flow_units,
        )
        .map_err(|failure| match failure {
            FlowError::UnitLimit => error(
                DeckDiagnosticCode::PLAN_WORK_LIMIT,
                "semantic flow unit count exceeds the configured planning bound",
                Some(slide.id),
            ),
        })?;
        let groups = group_units(&units);
        let primary = primary_region(layout.capability, &regions).ok_or_else(|| {
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
                    relations: &slide.media_text_relations,
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
            placements.extend(body.regions);
            pages.push(PhysicalPage {
                id: slide.id.derive(b"physical-page", ordinal as u32),
                logical_slide_id: slide.id,
                template_layout_id: layout.id,
                topology: body.topology,
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
                RegionPlacement::Fixed,
                node,
                FragmentSlice::Whole,
                FragmentPlacement {
                    frame: EmuRect {
                        height: measured.height,
                        ..region.frame
                    },
                    font_size: measured.font_size,
                    media: None,
                    repeat_table_header_rows: 0,
                },
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
    ) -> Result<Vec<PagePlacement>, PlanError> {
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
            let pages =
                self.candidate_pages(request, start, measurer, diagnostics, candidate_count)?;
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
        request: PaginationRequest<'_, '_>,
        start: usize,
        measurer: &mut Measurer<'_>,
        diagnostics: &mut Vec<DeckDiagnostic>,
        candidate_count: &mut usize,
    ) -> Result<Vec<CandidatePage>, PlanError> {
        let mut pages = Vec::new();
        for pattern in Pattern::ALL {
            if !pattern.accepts_first(&request.groups[start]) {
                continue;
            }
            for end in start + 1..=request.groups.len() {
                let Some(candidate) = self.fit_candidate(
                    pattern,
                    &request.groups[start..end],
                    request.region,
                    request.relations,
                    measurer,
                    candidate_count,
                )?
                else {
                    if pattern.prefix_closed() {
                        break;
                    }
                    continue;
                };
                diagnostics.extend(candidate.font_risks.into_iter().map(font_risk));
                pages.push(CandidatePage {
                    end,
                    score: candidate.score,
                    flow_units: request.groups[start..end]
                        .iter()
                        .map(|group| u64::try_from(group.units.len()).unwrap_or(u64::MAX))
                        .sum(),
                    demand: candidate.demand,
                    topology: pattern.topology(candidate.slot_count),
                    placements: candidate.placements,
                });
                if pages.len() == self.policy.limits.max_candidates_per_position {
                    break;
                }
            }
            if pages.len() == self.policy.limits.max_candidates_per_position {
                break;
            }
        }
        pages.sort_by_key(|page| (page.end, page.score, page.topology.slot_count));
        // Candidates reaching the same next source position share the same tail state. The first
        // one after deterministic score ordering dominates every later candidate at that end.
        pages.dedup_by_key(|page| page.end);
        Ok(pages)
    }

    fn fit_candidate<'a>(
        &self,
        pattern: Pattern,
        groups: &[FlowGroup<'a>],
        region: &TemplateRegion,
        relations: &[MediaTextRelation],
        measurer: &mut Measurer<'_>,
        candidate_count: &mut usize,
    ) -> Result<Option<FittedCandidate>, PlanError> {
        let bleed_region = if groups.iter().all(FlowGroup::is_media_only) {
            region.bleed_frame.map(|frame| {
                let mut region = region.clone();
                region.frame = frame;
                region
            })
        } else {
            None
        };
        let region = bleed_region.as_ref().unwrap_or(region);
        let collapsed_table = (pattern == Pattern::TableWide).then(|| FlowGroup {
            units: groups
                .iter()
                .flat_map(|group| group.units.iter().copied())
                .collect(),
        });
        let related_cards = if pattern == Pattern::RelatedCards {
            let Some(cards) = related_media_text_groups(groups, relations) else {
                return Ok(None);
            };
            Some(cards)
        } else {
            None
        };
        let search_groups = if let Some(cards) = related_cards.as_deref() {
            cards
        } else {
            collapsed_table
                .as_ref()
                .map_or(groups, std::slice::from_ref)
        };
        let mut selected = None::<FittedCandidate>;
        for frames in self.frame_variants(pattern, region, search_groups, measurer)? {
            for assignment in pattern.assignments(search_groups, frames.len()) {
                *candidate_count = candidate_count.saturating_add(1);
                if *candidate_count > self.policy.limits.max_candidate_assignments {
                    return Err(error(
                        DeckDiagnosticCode::PLAN_WORK_LIMIT,
                        "topology and slot-assignment search exceeds the configured candidate bound",
                        None,
                    ));
                }
                let Some(mut candidate) = self.fit_assignment(
                    AssignmentRequest {
                        pattern,
                        groups: search_groups,
                        region,
                        frames: &frames,
                        assignment: &assignment,
                        relations,
                    },
                    measurer,
                )?
                else {
                    continue;
                };
                candidate.slot_count = frames.len();
                if selected
                    .as_ref()
                    .is_none_or(|current| candidate.score < current.score)
                {
                    selected = Some(candidate);
                }
            }
        }
        Ok(selected)
    }

    fn frame_variants(
        &self,
        pattern: Pattern,
        region: &TemplateRegion,
        groups: &[FlowGroup<'_>],
        measurer: &mut Measurer<'_>,
    ) -> Result<Vec<Vec<EmuRect>>, PlanError> {
        if matches!(
            pattern,
            Pattern::Gallery2 | Pattern::Gallery4 | Pattern::Gallery6 | Pattern::RelatedCards
        ) {
            return Ok(aspect_packed_frame_variants(
                region.frame,
                self.policy.gap,
                groups,
                measurer,
            ));
        }
        if !matches!(pattern, Pattern::MediaStart | Pattern::MediaEnd) {
            return Ok(pattern.frame_variants(region.frame, self.policy.gap, groups.len()));
        }
        let Some(demand) = media_text_demand(groups, region, measurer)? else {
            return Ok(pattern.frame_variants(region.frame, self.policy.gap, groups.len()));
        };
        Ok(media_text_frame_variants(
            pattern,
            region.frame,
            region.margins,
            self.policy.gap,
            self.policy.readable_media_floor,
            demand,
        ))
    }

    fn fit_assignment<'a>(
        &self,
        request: AssignmentRequest<'_, 'a>,
        measurer: &mut Measurer<'_>,
    ) -> Result<Option<FittedCandidate>, PlanError> {
        let AssignmentRequest {
            pattern,
            groups,
            region,
            frames,
            assignment,
            relations,
        } = request;
        if assignment.len() != groups.len() || assignment.iter().any(|slot| *slot >= frames.len()) {
            return Ok(None);
        }
        let mut cursors = frames.iter().map(|frame| frame.y).collect::<Vec<_>>();
        let mut placements = Vec::new();
        let mut reductions = Vec::new();
        let mut width_loss = 0u64;
        let mut font_risks = BTreeSet::new();
        for (group, slot) in groups.iter().zip(assignment.iter().copied()) {
            if !pattern.accepts_group(group, slot) {
                return Ok(None);
            }
            let repeat_table_header_rows = repeated_table_header_rows(group, &placements);
            let Some(fitted) = self.fit_group(
                group,
                &FitTarget {
                    pattern,
                    region,
                    frames,
                    lane: slot,
                    y: cursors[slot],
                    repeat_table_header_rows,
                },
                measurer,
            )?
            else {
                return Ok(None);
            };
            for placement in &fitted.placements {
                if placement.font_risk {
                    font_risks.insert(placement.node.id);
                }
                reductions.push(placement.reduction);
                width_loss = width_loss.saturating_add(placement.width_penalty);
                push_coalesced_region(
                    &mut placements,
                    planned_region(
                        region,
                        RegionPlacement::Slot(slot as u16),
                        placement.node,
                        placement.slice,
                        FragmentPlacement {
                            frame: placement.frame,
                            font_size: placement.font_size,
                            media: placement.media,
                            repeat_table_header_rows: placement.repeat_table_header_rows,
                        },
                    ),
                );
            }
            cursors[slot] = fitted.bottom.saturating_add(self.policy.gap);
        }
        let score = candidate_score(CandidateScoreInput {
            pattern,
            groups,
            frames,
            cursors: &cursors,
            reductions: &reductions,
            width_loss,
            relation_loss: relation_penalty(relations, &placements, region.frame),
            font_step: self.policy.font_step,
        });
        let demand = placements
            .iter()
            .flat_map(|region| &region.fragments)
            .map(|fragment| {
                let width = u64::try_from(fragment.frame.width.max(0)).unwrap_or(u64::MAX);
                let height = u64::try_from(fragment.frame.height.max(0)).unwrap_or(u64::MAX);
                width
                    .saturating_mul(height)
                    .checked_div(u64::try_from(region.frame.width.max(1)).unwrap_or(u64::MAX))
                    .unwrap_or(u64::MAX)
            })
            .sum();
        Ok(Some(FittedCandidate {
            placements,
            score,
            demand,
            slot_count: frames.len(),
            font_risks: font_risks.into_iter().collect(),
        }))
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
        if let Some((node, slice)) = contiguous_table_group(group) {
            return self.fit_table_group(node, slice, lane_frame, initial_size, target, measurer);
        }
        if let Some(fitted) = self.fit_extreme_portrait_media_text_group(
            group,
            lane_frame,
            initial_size,
            target,
            measurer,
        )? {
            return Ok(Some(fitted));
        }
        let mut font_size = initial_size;
        loop {
            let mut cursor = target.y;
            let mut placements = Vec::new();
            let mut overflow = false;
            for (unit_index, unit) in group.units.iter().enumerate() {
                let frame = EmuRect {
                    x: lane_frame.x,
                    y: cursor,
                    width: lane_frame.width,
                    height: lane_frame
                        .y
                        .saturating_add(lane_frame.height)
                        .saturating_sub(cursor),
                };
                let measurement_frame = if unit_index + 1 < group.units.len()
                    && matches!(
                        unit.node.role,
                        SemanticRole::Figure | SemanticRole::Chart | SemanticRole::Diagram
                    ) {
                    EmuRect {
                        height: frame.height.saturating_mul(3) / 4,
                        ..frame
                    }
                } else {
                    frame
                };
                let intrinsic_size = measurer.intrinsic_size(unit.node);
                let preferred_fit = content_fit(target.pattern, unit.node, unit.gallery_item);
                let fit = match (preferred_fit, intrinsic_size) {
                    (ContentFit::Cover, Some(size))
                        if cover_crop_penalty(
                            size,
                            inset_frame(measurement_frame, target.region.margins),
                        ) > u64::from(self.policy.max_cover_crop_per_mille) =>
                    {
                        ContentFit::Contain
                    }
                    _ => preferred_fit,
                };
                if matches!(
                    unit.node.content,
                    SemanticContent::Image(_) | SemanticContent::Svg(_)
                ) && intrinsic_size.is_none()
                {
                    overflow = true;
                    break;
                }
                let measured = measurer
                    .measure(
                        unit.node,
                        unit.slice,
                        target.region,
                        measurement_frame,
                        font_size,
                        target.repeat_table_header_rows,
                    )
                    .map_err(|failure| measure_error(failure, unit.node.id))?;
                let usable_width = frame
                    .width
                    .saturating_sub(target.region.margins.left)
                    .saturating_sub(target.region.margins.right)
                    .max(1);
                if measured.width.min > usable_width {
                    overflow = true;
                    break;
                }
                if fit == ContentFit::Contain && measured.height > measurement_frame.height {
                    overflow = true;
                    break;
                }
                let media = intrinsic_size.and_then(|size| match fit {
                    ContentFit::Contain => MediaPlacement::contain(
                        inset_frame(measurement_frame, target.region.margins),
                        size,
                    ),
                    ContentFit::Cover => MediaPlacement::cover(
                        inset_frame(measurement_frame, target.region.margins),
                        size,
                    ),
                    ContentFit::None => None,
                });
                if media.is_some_and(|placement| {
                    placement
                        .visible_frame
                        .width
                        .min(placement.visible_frame.height)
                        < self.policy.readable_media_floor
                }) {
                    overflow = true;
                    break;
                }
                let fragment_frame = media.map_or_else(
                    || EmuRect {
                        height: measured.height,
                        ..frame
                    },
                    |media| media.visible_frame,
                );
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
                    width_penalty: width_penalty(
                        measured.width.preferred.min(measured.width.max),
                        usable_width,
                    )
                    .saturating_add(
                        intrinsic_size
                            .filter(|_| fit == ContentFit::Cover)
                            .map_or(0, |size| cover_crop_penalty(size, fragment_frame)),
                    )
                    .saturating_add(media.map_or(0, contain_whitespace_penalty)),
                    font_risk: measured.font_risk,
                    media,
                    repeat_table_header_rows: target.repeat_table_header_rows,
                });
                cursor = fragment_frame
                    .y
                    .saturating_add(fragment_frame.height)
                    .saturating_add(if fit == ContentFit::None {
                        0
                    } else {
                        target.region.margins.bottom
                    })
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

    fn fit_extreme_portrait_media_text_group<'a>(
        &self,
        group: &FlowGroup<'a>,
        lane_frame: EmuRect,
        initial_size: u32,
        target: &FitTarget<'_>,
        measurer: &mut Measurer<'_>,
    ) -> Result<Option<FittedGroup<'a>>, PlanError> {
        let [media_unit, text_unit] = group.units.as_slice() else {
            return Ok(None);
        };
        if !matches!(text_unit.node.content, SemanticContent::Text(_))
            || !matches!(
                media_unit.node.role,
                SemanticRole::Figure | SemanticRole::Chart | SemanticRole::Diagram
            )
        {
            return Ok(None);
        }
        let Some(source_size) = measurer.intrinsic_size(media_unit.node) else {
            return Ok(None);
        };
        if u64::from(source_size.height) <= u64::from(source_size.width).saturating_mul(2) {
            return Ok(None);
        }
        let remaining = EmuRect {
            x: lane_frame.x,
            y: target.y,
            width: lane_frame.width,
            height: lane_frame
                .y
                .saturating_add(lane_frame.height)
                .saturating_sub(target.y),
        };
        if !remaining.is_positive() {
            return Ok(None);
        }
        let margins = target.region.margins;
        let stacked_slot = inset_frame(
            EmuRect {
                height: remaining.height.saturating_mul(3) / 4,
                ..remaining
            },
            margins,
        );
        let Some(stacked) = MediaPlacement::contain(stacked_slot, source_size) else {
            return Ok(None);
        };
        if stacked
            .visible_frame
            .width
            .min(stacked.visible_frame.height)
            >= self.policy.readable_media_floor
        {
            return Ok(None);
        }

        let horizontal_margins = margins.left.saturating_add(margins.right);
        let vertical_margins = margins.top.saturating_add(margins.bottom);
        let usable_height = remaining.height.saturating_sub(vertical_margins).max(1);
        let visible_width = usable_height
            .saturating_mul(i64::from(source_size.width))
            .saturating_add(i64::from(source_size.height).saturating_sub(1))
            .checked_div(i64::from(source_size.height).max(1))
            .unwrap_or(1)
            .max(self.policy.readable_media_floor);
        let media_width = visible_width.saturating_add(horizontal_margins);
        if media_width.saturating_add(self.policy.gap) >= remaining.width {
            return Ok(None);
        }
        let frames = split_pair(remaining, self.policy.gap, media_width, true);
        let Some(media) = MediaPlacement::contain(inset_frame(frames[0], margins), source_size)
        else {
            return Ok(None);
        };
        if media.visible_frame.width.min(media.visible_frame.height)
            < self.policy.readable_media_floor
        {
            return Ok(None);
        }

        let text_frame = frames[1];
        let usable_text_width = text_frame.width.saturating_sub(horizontal_margins).max(1);
        let mut font_size = initial_size;
        loop {
            let measured = measurer
                .measure(
                    text_unit.node,
                    text_unit.slice,
                    target.region,
                    text_frame,
                    font_size,
                    target.repeat_table_header_rows,
                )
                .map_err(|failure| measure_error(failure, text_unit.node.id))?;
            if measured.width.min <= usable_text_width && measured.height <= text_frame.height {
                let text_bounds = EmuRect {
                    height: measured.height,
                    ..text_frame
                };
                return Ok(Some(FittedGroup {
                    placements: vec![
                        FittedPlacement {
                            node: media_unit.node,
                            slice: media_unit.slice,
                            frame: media.visible_frame,
                            font_size: 0,
                            reduction: 0,
                            width_penalty: contain_whitespace_penalty(media),
                            font_risk: false,
                            media: Some(media),
                            repeat_table_header_rows: target.repeat_table_header_rows,
                        },
                        FittedPlacement {
                            node: text_unit.node,
                            slice: text_unit.slice,
                            frame: text_bounds,
                            font_size: measured.font_size,
                            reduction: initial_size.saturating_sub(measured.font_size),
                            width_penalty: width_penalty(
                                measured.width.preferred.min(measured.width.max),
                                usable_text_width,
                            ),
                            font_risk: measured.font_risk,
                            media: None,
                            repeat_table_header_rows: target.repeat_table_header_rows,
                        },
                    ],
                    bottom: media
                        .visible_frame
                        .y
                        .saturating_add(media.visible_frame.height)
                        .max(text_bounds.y.saturating_add(text_bounds.height)),
                }));
            }
            if font_size <= self.policy.readable_floor {
                return Ok(None);
            }
            font_size = font_size
                .saturating_sub(self.policy.font_step)
                .max(self.policy.readable_floor);
        }
    }

    fn fit_table_group<'a>(
        &self,
        node: &'a SemanticNode,
        slice: FragmentSlice,
        lane_frame: EmuRect,
        initial_size: u32,
        target: &FitTarget<'_>,
        measurer: &mut Measurer<'_>,
    ) -> Result<Option<FittedGroup<'a>>, PlanError> {
        let frame = EmuRect {
            y: target.y,
            height: lane_frame
                .y
                .saturating_add(lane_frame.height)
                .saturating_sub(target.y),
            ..lane_frame
        };
        let usable_width = frame
            .width
            .saturating_sub(target.region.margins.left)
            .saturating_sub(target.region.margins.right)
            .max(1);
        let mut font_size = initial_size;
        loop {
            let measured = measurer
                .measure(
                    node,
                    slice,
                    target.region,
                    frame,
                    font_size,
                    target.repeat_table_header_rows,
                )
                .map_err(|failure| measure_error(failure, node.id))?;
            let fragment_frame = EmuRect {
                height: measured.height,
                ..frame
            };
            if measured.width.min <= usable_width && fragment_frame.is_within(lane_frame) {
                return Ok(Some(FittedGroup {
                    placements: vec![FittedPlacement {
                        node,
                        slice,
                        frame: fragment_frame,
                        font_size: measured.font_size,
                        reduction: initial_size.saturating_sub(measured.font_size),
                        width_penalty: width_penalty(
                            measured.width.preferred.min(measured.width.max),
                            usable_width,
                        ),
                        font_risk: measured.font_risk,
                        media: None,
                        repeat_table_header_rows: target.repeat_table_header_rows,
                    }],
                    bottom: fragment_frame.y.saturating_add(fragment_frame.height),
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

fn contiguous_table_group<'a>(group: &FlowGroup<'a>) -> Option<(&'a SemanticNode, FragmentSlice)> {
    let first = *group.units.first()?;
    let FragmentSlice::TableRows {
        start,
        end: mut previous_end,
    } = first.slice
    else {
        return None;
    };
    for unit in group.units.iter().skip(1) {
        let FragmentSlice::TableRows { start, end } = unit.slice else {
            return None;
        };
        if unit.node.id != first.node.id || start != previous_end {
            return None;
        }
        previous_end = end;
    }
    Some((
        first.node,
        FragmentSlice::TableRows {
            start,
            end: previous_end,
        },
    ))
}

fn validate_planning_inputs(
    spec: &DeckSpec,
    template: &DeckTemplatePlan,
    fonts: &FontCatalog,
    contract_limits: &DeckLimits,
    policy: &PlannerPolicy,
) -> Result<(), PlanError> {
    let font_bytes = fonts
        .faces
        .iter()
        .map(|face| face.bytes.len())
        .try_fold(0usize, usize::checked_add);
    if fonts.faces.len() > policy.limits.max_font_faces
        || font_bytes.is_none_or(|bytes| bytes > policy.limits.max_font_bytes)
    {
        return Err(error(
            DeckDiagnosticCode::PLAN_WORK_LIMIT,
            "font catalog exceeds the configured planning bound",
            None,
        ));
    }
    let report = validate_deck_spec(spec, contract_limits);
    if !report.is_valid() {
        return Err(PlanError {
            code: DeckDiagnosticCode::INVALID_SEMANTIC_CONTENT,
            diagnostics: report.diagnostics,
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
    Ok(())
}

fn referenced_resources_equal(
    slide: &LogicalSlide,
    previous: &BTreeMap<StableId, &DeckResource>,
    next: &BTreeMap<StableId, &DeckResource>,
) -> bool {
    let mut ids = BTreeSet::new();
    collect_resource_ids(&slide.nodes, &mut ids);
    ids.into_iter().all(|id| previous.get(&id) == next.get(&id))
}

fn collect_resource_ids(nodes: &[SemanticNode], output: &mut BTreeSet<StableId>) {
    for node in nodes {
        match &node.content {
            SemanticContent::Image(image) => {
                output.insert(image.resource_id);
            }
            SemanticContent::Svg(svg) => {
                output.insert(svg.resource_id);
            }
            SemanticContent::Children(children) => collect_resource_ids(children, output),
            SemanticContent::List(list) => {
                for item in &list.items {
                    collect_resource_ids(&item.blocks, output);
                    for children in &item.children {
                        collect_list_resource_ids(children, output);
                    }
                }
            }
            SemanticContent::Text(_)
            | SemanticContent::Table(_)
            | SemanticContent::Chart(_)
            | SemanticContent::Code(_) => {}
        }
    }
}

fn collect_list_resource_ids(list: &wasmppt_deck::ListContent, output: &mut BTreeSet<StableId>) {
    for item in &list.items {
        collect_resource_ids(&item.blocks, output);
        for children in &item.children {
            collect_list_resource_ids(children, output);
        }
    }
}

fn collect_node_ids(nodes: &[SemanticNode], output: &mut BTreeSet<StableId>) {
    for node in nodes {
        output.insert(node.id);
        match &node.content {
            SemanticContent::Children(children) => collect_node_ids(children, output),
            SemanticContent::List(list) => collect_list_node_ids(list, output),
            _ => {}
        }
    }
}

fn collect_list_node_ids(list: &wasmppt_deck::ListContent, output: &mut BTreeSet<StableId>) {
    for item in &list.items {
        output.insert(item.id);
        collect_node_ids(&item.blocks, output);
        for children in &item.children {
            collect_list_node_ids(children, output);
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Pattern {
    Stack,
    FlowColumns2,
    FlowColumns3,
    WeightedStart,
    WeightedEnd,
    MediaStart,
    MediaEnd,
    PeerGrid2,
    PeerGrid4,
    PeerGrid6,
    LeadSupporting,
    RelatedCards,
    Gallery2,
    Gallery4,
    Gallery6,
    TableWide,
    Comparison,
}

impl Pattern {
    const MAX_STANDARD_FLOW_PER_SLOT: usize = 8;
    const MAX_CODE_FLOW_PER_SLOT: usize = 12;
    const ALL: [Self; 17] = [
        Self::Stack,
        Self::FlowColumns2,
        Self::FlowColumns3,
        Self::WeightedStart,
        Self::WeightedEnd,
        Self::MediaStart,
        Self::MediaEnd,
        Self::PeerGrid2,
        Self::PeerGrid4,
        Self::PeerGrid6,
        Self::LeadSupporting,
        Self::RelatedCards,
        Self::Gallery2,
        Self::Gallery4,
        Self::Gallery6,
        Self::TableWide,
        Self::Comparison,
    ];

    fn accepts_first(self, group: &FlowGroup<'_>) -> bool {
        match self {
            Self::TableWide => group.is_table(),
            Self::Gallery2 | Self::Gallery4 | Self::Gallery6 => group.is_media(),
            _ => true,
        }
    }

    fn prefix_closed(self) -> bool {
        matches!(
            self,
            Self::Stack | Self::FlowColumns2 | Self::FlowColumns3 | Self::TableWide
        )
    }

    fn frame_variants(self, frame: EmuRect, gap: Emu, groups: usize) -> Vec<Vec<EmuRect>> {
        match self {
            Self::MediaStart => vec![
                columns(frame, gap, &[2, 3]),
                columns(frame, gap, &[1, 1]),
                columns(frame, gap, &[3, 2]),
                rows(frame, gap, &[3, 2]),
            ],
            Self::MediaEnd => vec![
                columns(frame, gap, &[3, 2]),
                columns(frame, gap, &[1, 1]),
                columns(frame, gap, &[2, 3]),
                rows(frame, gap, &[2, 3]),
            ],
            Self::Gallery2 | Self::Gallery4 | Self::Gallery6 => {
                gallery_frame_variants(frame, gap, groups)
            }
            _ => vec![match self {
                Self::Stack | Self::TableWide => vec![frame],
                Self::FlowColumns2 | Self::PeerGrid2 | Self::Comparison => {
                    columns(frame, gap, &[1, 1])
                }
                Self::FlowColumns3 => columns(frame, gap, &[1, 1, 1]),
                Self::WeightedStart => columns(frame, gap, &[3, 2]),
                Self::WeightedEnd => columns(frame, gap, &[2, 3]),
                Self::PeerGrid4 => grid(frame, gap, 2, 2),
                Self::PeerGrid6 => grid(frame, gap, 2, 3),
                Self::LeadSupporting => lead_supporting(frame, gap),
                Self::RelatedCards => Vec::new(),
                Self::MediaStart
                | Self::MediaEnd
                | Self::Gallery2
                | Self::Gallery4
                | Self::Gallery6 => unreachable!(),
            }],
        }
    }

    fn topology(self, slot_count: usize) -> TopologyChoice {
        let kind = match self {
            Self::Stack => LayoutTopology::Stack,
            Self::FlowColumns2 | Self::FlowColumns3 => LayoutTopology::FlowColumns,
            Self::WeightedStart | Self::WeightedEnd => LayoutTopology::WeightedSplit,
            Self::MediaStart => LayoutTopology::MediaStart,
            Self::MediaEnd => LayoutTopology::MediaEnd,
            Self::PeerGrid2 | Self::PeerGrid4 | Self::PeerGrid6 => LayoutTopology::PeerGrid,
            Self::LeadSupporting => LayoutTopology::LeadSupporting,
            Self::RelatedCards if slot_count == 1 => LayoutTopology::Stack,
            Self::RelatedCards => LayoutTopology::PeerGrid,
            Self::Gallery2 | Self::Gallery4 | Self::Gallery6 => LayoutTopology::Gallery,
            Self::TableWide => LayoutTopology::TableWide,
            Self::Comparison => LayoutTopology::Comparison,
        };
        TopologyChoice {
            kind,
            slot_count: u16::try_from(slot_count).unwrap_or(u16::MAX),
        }
    }

    const fn complexity(self) -> u64 {
        match self {
            Self::Stack => 0,
            Self::FlowColumns2 => 10,
            Self::FlowColumns3 => 15,
            Self::WeightedStart | Self::WeightedEnd => 20,
            Self::MediaStart | Self::MediaEnd => 25,
            Self::TableWide => 25,
            Self::Comparison => 30,
            Self::LeadSupporting => 35,
            Self::RelatedCards => 38,
            Self::PeerGrid2 => 40,
            Self::PeerGrid4 => 45,
            Self::PeerGrid6 => 50,
            Self::Gallery2 => 55,
            Self::Gallery4 => 60,
            Self::Gallery6 => 65,
        }
    }

    const fn tie_break(self) -> u64 {
        match self {
            Self::Stack => 0,
            Self::FlowColumns2 => 1,
            Self::FlowColumns3 => 2,
            Self::WeightedStart => 3,
            Self::WeightedEnd => 4,
            Self::MediaStart => 5,
            Self::MediaEnd => 6,
            Self::PeerGrid2 => 7,
            Self::PeerGrid4 => 8,
            Self::PeerGrid6 => 9,
            Self::LeadSupporting => 10,
            Self::RelatedCards => 11,
            Self::Gallery2 => 12,
            Self::Gallery4 => 13,
            Self::Gallery6 => 14,
            Self::TableWide => 15,
            Self::Comparison => 16,
        }
    }

    fn assignments(self, groups: &[FlowGroup<'_>], slots: usize) -> Vec<Vec<usize>> {
        if self != Self::TableWide && groups.iter().any(FlowGroup::is_table) {
            return Vec::new();
        }
        match self {
            Self::Stack => {
                if semantic_slots_required(groups)
                    || (groups.len() > Self::MAX_STANDARD_FLOW_PER_SLOT
                        && groups.iter().all(FlowGroup::is_breakable_flow))
                {
                    Vec::new()
                } else {
                    vec![vec![0; groups.len()]]
                }
            }
            Self::FlowColumns2 => {
                if groups.iter().any(FlowGroup::is_media) || peer_collection(groups) {
                    Vec::new()
                } else {
                    bounded_contiguous_assignments(
                        groups.len(),
                        slots,
                        Self::MAX_STANDARD_FLOW_PER_SLOT,
                    )
                }
            }
            Self::FlowColumns3 => {
                if groups.iter().all(FlowGroup::is_code) {
                    bounded_contiguous_assignments(
                        groups.len(),
                        slots,
                        Self::MAX_CODE_FLOW_PER_SLOT,
                    )
                } else {
                    Vec::new()
                }
            }
            Self::WeightedStart | Self::WeightedEnd => {
                if have_distinct_nodes(groups, 2)
                    && !mixed_media(groups)
                    && !groups.iter().all(FlowGroup::is_media)
                {
                    contiguous_assignments(groups.len(), slots)
                } else {
                    Vec::new()
                }
            }
            Self::MediaStart | Self::MediaEnd => media_assignment(groups, self),
            Self::PeerGrid2 => {
                if groups.len() == 2 && groups.iter().all(FlowGroup::is_peer) {
                    unique_assignment(groups.len(), slots)
                } else {
                    Vec::new()
                }
            }
            Self::PeerGrid4 => {
                if (3..=4).contains(&groups.len()) && groups.iter().all(FlowGroup::is_peer) {
                    unique_assignment(groups.len(), slots)
                } else {
                    Vec::new()
                }
            }
            Self::PeerGrid6 => {
                if (5..=6).contains(&groups.len()) && groups.iter().all(FlowGroup::is_peer) {
                    unique_assignment(groups.len(), slots)
                } else {
                    Vec::new()
                }
            }
            Self::LeadSupporting => {
                if groups.len() == 3 && groups.iter().all(FlowGroup::is_peer) {
                    unique_assignment(groups.len(), slots)
                } else {
                    Vec::new()
                }
            }
            Self::RelatedCards => {
                if (1..=3).contains(&groups.len()) && groups.iter().all(FlowGroup::is_related_card)
                {
                    unique_assignment(groups.len(), slots)
                } else {
                    Vec::new()
                }
            }
            Self::Gallery2 => {
                if groups.len() == 2 && gallery_collection(groups) {
                    unique_assignment(groups.len(), slots)
                } else {
                    Vec::new()
                }
            }
            Self::Gallery4 => {
                if (3..=4).contains(&groups.len()) && gallery_collection(groups) {
                    unique_assignment(groups.len(), slots)
                } else {
                    Vec::new()
                }
            }
            Self::Gallery6 => {
                if (5..=6).contains(&groups.len()) && gallery_collection(groups) {
                    unique_assignment(groups.len(), slots)
                } else {
                    Vec::new()
                }
            }
            Self::TableWide => {
                if groups.iter().all(FlowGroup::is_table) {
                    vec![vec![0; groups.len()]]
                } else {
                    Vec::new()
                }
            }
            Self::Comparison => {
                if groups.len() == 2 && groups.iter().all(FlowGroup::is_peer) {
                    vec![vec![0, 1]]
                } else {
                    Vec::new()
                }
            }
        }
    }

    fn accepts_group(self, group: &FlowGroup<'_>, slot: usize) -> bool {
        match self {
            Self::MediaStart => group.is_media() == (slot == 0),
            Self::MediaEnd => group.is_media() == (slot == 1),
            Self::Gallery2 | Self::Gallery4 | Self::Gallery6 => group.is_media(),
            Self::TableWide => group.is_table(),
            Self::Comparison => group.is_peer(),
            _ => true,
        }
    }
}

fn grid(frame: EmuRect, gap: Emu, columns_count: usize, rows_count: usize) -> Vec<EmuRect> {
    rows(frame, gap, &vec![1; rows_count])
        .into_iter()
        .flat_map(|row| columns(row, gap, &vec![1; columns_count]))
        .collect()
}

fn lead_supporting(frame: EmuRect, gap: Emu) -> Vec<EmuRect> {
    let row_frames = rows(frame, gap, &[3, 2]);
    let mut frames = vec![row_frames[0]];
    frames.extend(columns(row_frames[1], gap, &[1, 1]));
    frames
}

fn gallery_frame_variants(frame: EmuRect, gap: Emu, groups: usize) -> Vec<Vec<EmuRect>> {
    match groups {
        0 => Vec::new(),
        1 => vec![vec![frame]],
        2 => vec![columns(frame, gap, &[1, 1]), rows(frame, gap, &[1, 1])],
        3 => vec![columns(frame, gap, &[1, 1, 1]), lead_supporting(frame, gap)],
        4 => vec![grid(frame, gap, 2, 2)],
        5 | 6 => vec![
            grid(frame, gap, 3, 2).into_iter().take(groups).collect(),
            grid(frame, gap, 2, 3).into_iter().take(groups).collect(),
        ],
        _ => Vec::new(),
    }
}

fn related_media_text_groups<'a>(
    groups: &[FlowGroup<'a>],
    relations: &[MediaTextRelation],
) -> Option<Vec<FlowGroup<'a>>> {
    if !matches!(groups.len(), 2 | 4 | 6) {
        return None;
    }
    let mut cards = Vec::with_capacity(groups.len() / 2);
    for pair in groups.chunks_exact(2) {
        let [left, right] = pair else {
            return None;
        };
        let related = relations.iter().any(|relation| {
            !relation.explicit_caption
                && ((group_contains(left, relation.media_node_id)
                    && group_contains(right, relation.text_node_id))
                    || (group_contains(right, relation.media_node_id)
                        && group_contains(left, relation.text_node_id)))
        });
        if !related || left.is_media() == right.is_media() {
            return None;
        }
        cards.push(FlowGroup {
            units: pair
                .iter()
                .flat_map(|group| group.units.iter().copied())
                .collect(),
        });
    }
    Some(cards)
}

fn group_contains(group: &FlowGroup<'_>, node_id: StableId) -> bool {
    group.units.iter().any(|unit| unit.node.id == node_id)
}

fn aspect_packed_frame_variants(
    frame: EmuRect,
    gap: Emu,
    groups: &[FlowGroup<'_>],
    measurer: &Measurer<'_>,
) -> Vec<Vec<EmuRect>> {
    const ASPECT_SCALE: u64 = 10_000;
    const TRACK_SCALE: u64 = 100_000_000;
    const MAX_VARIANTS: usize = 48;

    if groups.is_empty() {
        return Vec::new();
    }
    let aspects = groups
        .iter()
        .map(|group| {
            let size = group
                .units
                .iter()
                .find_map(|unit| measurer.intrinsic_size(unit.node))?;
            let mut aspect = u64::from(size.width)
                .saturating_mul(ASPECT_SCALE)
                .checked_div(u64::from(size.height).max(1))?
                .clamp(1, i64::MAX as u64);
            if group
                .units
                .iter()
                .any(|unit| matches!(unit.node.content, SemanticContent::Text(_)))
            {
                aspect = aspect.saturating_mul(3) / 4;
            }
            Some(i64::try_from(aspect).unwrap_or(i64::MAX).max(1))
        })
        .collect::<Option<Vec<_>>>();
    let Some(aspects) = aspects else {
        return gallery_frame_variants(frame, gap, groups.len());
    };

    let mut variants = Vec::new();
    for tracks in 1..=groups.len().min(3) {
        for assignment in contiguous_assignments(groups.len(), tracks) {
            let ranges = assignment_ranges(&assignment, tracks);
            let row_weights = ranges
                .iter()
                .map(|range| {
                    let sum = aspects[range.clone()].iter().copied().sum::<i64>().max(1);
                    i64::try_from(TRACK_SCALE / u64::try_from(sum).unwrap_or(u64::MAX).max(1))
                        .unwrap_or(1)
                        .max(1)
                })
                .collect::<Vec<_>>();
            let row_frames = rows(frame, gap, &row_weights);
            let mut packed_rows = Vec::with_capacity(groups.len());
            for (row, range) in row_frames.into_iter().zip(&ranges) {
                packed_rows.extend(columns(row, gap, &aspects[range.clone()]));
            }
            variants.push(packed_rows);

            let column_weights = ranges
                .iter()
                .map(|range| {
                    let inverse_sum = aspects[range.clone()]
                        .iter()
                        .map(|aspect| {
                            i64::try_from(TRACK_SCALE / u64::try_from(*aspect).unwrap_or(1).max(1))
                                .unwrap_or(i64::MAX)
                        })
                        .sum::<i64>()
                        .max(1);
                    i64::try_from(TRACK_SCALE / u64::try_from(inverse_sum).unwrap_or(u64::MAX))
                        .unwrap_or(1)
                        .max(1)
                })
                .collect::<Vec<_>>();
            let column_frames = columns(frame, gap, &column_weights);
            let mut packed_columns = Vec::with_capacity(groups.len());
            for (column, range) in column_frames.into_iter().zip(&ranges) {
                let inverse = aspects[range.clone()]
                    .iter()
                    .map(|aspect| {
                        i64::try_from(TRACK_SCALE / u64::try_from(*aspect).unwrap_or(1).max(1))
                            .unwrap_or(i64::MAX)
                            .max(1)
                    })
                    .collect::<Vec<_>>();
                packed_columns.extend(rows(column, gap, &inverse));
            }
            variants.push(packed_columns);
        }
    }
    if groups.len() >= 3 {
        let horizontal = columns(frame, gap, &[3, 2]);
        let mut lead = vec![horizontal[0]];
        lead.extend(rows(horizontal[1], gap, &vec![1; groups.len() - 1]));
        variants.push(lead);

        let vertical = rows(frame, gap, &[3, 2]);
        let mut lead = vec![vertical[0]];
        lead.extend(columns(vertical[1], gap, &vec![1; groups.len() - 1]));
        variants.push(lead);
    }
    variants.sort_by_key(|frames| {
        frames
            .iter()
            .map(|frame| (frame.x, frame.y, frame.width, frame.height))
            .collect::<Vec<_>>()
    });
    variants.dedup();
    variants.truncate(MAX_VARIANTS);
    variants
}

fn assignment_ranges(assignment: &[usize], slots: usize) -> Vec<std::ops::Range<usize>> {
    (0..slots)
        .filter_map(|slot| {
            let start = assignment.iter().position(|assigned| *assigned == slot)?;
            let end = assignment
                .iter()
                .rposition(|assigned| *assigned == slot)?
                .saturating_add(1);
            Some(start..end)
        })
        .collect()
}

fn unique_assignment(groups: usize, slots: usize) -> Vec<Vec<usize>> {
    if groups <= slots {
        vec![(0..groups).collect()]
    } else {
        Vec::new()
    }
}

fn contiguous_assignments(groups: usize, slots: usize) -> Vec<Vec<usize>> {
    if groups == 0 || slots == 0 {
        return Vec::new();
    }
    let used_slots = slots.min(groups);
    let mut output = Vec::new();
    let mut cuts = Vec::with_capacity(used_slots.saturating_sub(1));
    enumerate_cuts(groups, used_slots, 1, &mut cuts, &mut output);
    output
}

fn bounded_contiguous_assignments(
    groups: usize,
    slots: usize,
    maximum_per_slot: usize,
) -> Vec<Vec<usize>> {
    contiguous_assignments(groups, slots)
        .into_iter()
        .filter(|assignment| {
            (0..slots).all(|slot| {
                assignment
                    .iter()
                    .filter(|assigned| **assigned == slot)
                    .count()
                    <= maximum_per_slot
            })
        })
        .collect()
}

fn enumerate_cuts(
    groups: usize,
    slots: usize,
    next: usize,
    cuts: &mut Vec<usize>,
    output: &mut Vec<Vec<usize>>,
) {
    const MAX_PARTITIONS: usize = 256;
    if output.len() == MAX_PARTITIONS {
        return;
    }
    if cuts.len() + 1 == slots {
        let mut assignment = Vec::with_capacity(groups);
        let mut previous = 0usize;
        for (slot, end) in cuts.iter().copied().chain([groups]).enumerate() {
            assignment.extend(std::iter::repeat_n(slot, end.saturating_sub(previous)));
            previous = end;
        }
        output.push(assignment);
        return;
    }
    let remaining_cuts = slots.saturating_sub(cuts.len() + 1);
    let last = groups.saturating_sub(remaining_cuts);
    for cut in next..=last {
        cuts.push(cut);
        enumerate_cuts(groups, slots, cut + 1, cuts, output);
        cuts.pop();
        if output.len() == MAX_PARTITIONS {
            break;
        }
    }
}

fn media_assignment(groups: &[FlowGroup<'_>], pattern: Pattern) -> Vec<Vec<usize>> {
    let media = groups.iter().filter(|group| group.is_media()).count();
    if media == 0 || media == groups.len() {
        return Vec::new();
    }
    let media_slot = usize::from(pattern == Pattern::MediaEnd);
    vec![
        groups
            .iter()
            .map(|group| {
                if group.is_media() {
                    media_slot
                } else {
                    1 - media_slot
                }
            })
            .collect(),
    ]
}

fn have_distinct_nodes(groups: &[FlowGroup<'_>], minimum: usize) -> bool {
    groups
        .iter()
        .filter_map(|group| group.units.first().map(|unit| unit.node.id))
        .collect::<BTreeSet<_>>()
        .len()
        >= minimum
}

fn mixed_media(groups: &[FlowGroup<'_>]) -> bool {
    groups.iter().any(FlowGroup::is_media) && !groups.iter().all(FlowGroup::is_media)
}

fn peer_collection(groups: &[FlowGroup<'_>]) -> bool {
    groups.len() >= 2 && groups.iter().all(FlowGroup::is_peer)
}

fn gallery_collection(groups: &[FlowGroup<'_>]) -> bool {
    groups.iter().all(FlowGroup::is_media) && groups.iter().any(FlowGroup::is_gallery_item)
}

fn semantic_slots_required(groups: &[FlowGroup<'_>]) -> bool {
    peer_collection(groups)
        || mixed_media(groups)
        || (groups.len() >= 2 && groups.iter().all(FlowGroup::is_media))
        || groups.iter().all(FlowGroup::is_table)
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

#[derive(Clone, Copy)]
struct PaginationRequest<'groups, 'nodes> {
    groups: &'groups [FlowGroup<'nodes>],
    region: &'groups TemplateRegion,
    slide_id: StableId,
    relations: &'groups [MediaTextRelation],
}

struct AssignmentRequest<'request, 'nodes> {
    pattern: Pattern,
    groups: &'request [FlowGroup<'nodes>],
    region: &'request TemplateRegion,
    frames: &'request [EmuRect],
    assignment: &'request [usize],
    relations: &'request [MediaTextRelation],
}

#[derive(Clone, Copy)]
struct MediaTextDemand {
    media_size: PixelSize,
    text_min_width: Emu,
    text_preferred_width: Emu,
    text_height: Emu,
}

fn media_text_demand(
    groups: &[FlowGroup<'_>],
    region: &TemplateRegion,
    measurer: &mut Measurer<'_>,
) -> Result<Option<MediaTextDemand>, PlanError> {
    let mut media_size = None;
    let mut media_count = 0usize;
    let mut text_min_width = 0;
    let mut text_preferred_width = 0;
    let mut text_height: Emu = 0;
    let mut text_count = 0usize;
    for unit in groups.iter().flat_map(|group| &group.units) {
        if let Some(size) = measurer.intrinsic_size(unit.node) {
            media_size = Some(size);
            media_count = media_count.saturating_add(1);
            continue;
        }
        if !matches!(unit.node.content, SemanticContent::Text(_)) {
            continue;
        }
        let measured = measurer
            .measure(unit.node, unit.slice, region, region.frame, 0, 0)
            .map_err(|failure| measure_error(failure, unit.node.id))?;
        text_min_width = text_min_width.max(measured.width.min);
        text_preferred_width = text_preferred_width.max(measured.width.preferred);
        text_height = text_height.saturating_add(measured.height);
        text_count = text_count.saturating_add(1);
    }
    if media_count != 1 || text_count == 0 {
        return Ok(None);
    }
    Ok(media_size.map(|media_size| MediaTextDemand {
        media_size,
        text_min_width,
        text_preferred_width,
        text_height,
    }))
}

fn media_text_frame_variants(
    pattern: Pattern,
    frame: EmuRect,
    margins: wasmppt_deck::TextMargins,
    gap: Emu,
    media_floor: Emu,
    demand: MediaTextDemand,
) -> Vec<Vec<EmuRect>> {
    let horizontal_margins = margins.left.saturating_add(margins.right);
    let vertical_margins = margins.top.saturating_add(margins.bottom);
    let available_width = frame.width.saturating_sub(gap).max(1);
    let available_height = frame.height.saturating_sub(gap).max(1);
    let usable_height = frame
        .height
        .saturating_sub(vertical_margins)
        .max(media_floor);
    let ideal_media_width = usable_height
        .saturating_mul(i64::from(demand.media_size.width))
        .checked_div(i64::from(demand.media_size.height).max(1))
        .unwrap_or(available_width)
        .saturating_add(horizontal_margins);
    let media_min_width = media_floor.saturating_add(horizontal_margins);
    let text_min_width = demand.text_min_width.saturating_add(horizontal_margins);
    let text_preferred_width = demand
        .text_preferred_width
        .saturating_add(horizontal_margins);
    let mut media_widths = BTreeSet::new();
    media_widths.extend([
        media_min_width,
        ideal_media_width,
        available_width / 2,
        available_width.saturating_sub(text_preferred_width),
        available_width.saturating_sub(text_min_width),
    ]);

    let mut variants = Vec::new();
    for media_width in media_widths {
        if media_width < media_min_width
            || available_width.saturating_sub(media_width) < text_min_width
        {
            continue;
        }
        let first_width = if pattern == Pattern::MediaStart {
            media_width
        } else {
            available_width.saturating_sub(media_width)
        };
        variants.push(split_pair(frame, gap, first_width, true));
    }

    let usable_width = frame
        .width
        .saturating_sub(horizontal_margins)
        .max(media_floor);
    let ideal_media_height = usable_width
        .saturating_mul(i64::from(demand.media_size.height))
        .checked_div(i64::from(demand.media_size.width).max(1))
        .unwrap_or(available_height)
        .saturating_add(vertical_margins);
    let media_min_height = media_floor.saturating_add(vertical_margins);
    let text_min_height = demand
        .text_height
        .min(available_height / 2)
        .max(vertical_margins.saturating_add(1));
    let mut media_heights = BTreeSet::new();
    media_heights.extend([
        media_min_height,
        ideal_media_height,
        available_height / 2,
        available_height.saturating_sub(demand.text_height),
    ]);
    for media_height in media_heights {
        if media_height < media_min_height
            || available_height.saturating_sub(media_height) < text_min_height
        {
            continue;
        }
        let first_height = if pattern == Pattern::MediaStart {
            media_height
        } else {
            available_height.saturating_sub(media_height)
        };
        variants.push(split_pair(frame, gap, first_height, false));
    }
    variants.sort_by_key(|frames| {
        frames
            .iter()
            .map(|frame| (frame.x, frame.y, frame.width, frame.height))
            .collect::<Vec<_>>()
    });
    variants.dedup();
    variants
}

fn split_pair(frame: EmuRect, gap: Emu, first_extent: Emu, horizontal: bool) -> Vec<EmuRect> {
    let available = if horizontal {
        frame.width
    } else {
        frame.height
    }
    .saturating_sub(gap)
    .max(1);
    let first_extent = first_extent.clamp(1, available.saturating_sub(1).max(1));
    let second_extent = available.saturating_sub(first_extent).max(1);
    if horizontal {
        vec![
            EmuRect {
                width: first_extent,
                ..frame
            },
            EmuRect {
                x: frame.x.saturating_add(first_extent).saturating_add(gap),
                width: second_extent,
                ..frame
            },
        ]
    } else {
        vec![
            EmuRect {
                height: first_extent,
                ..frame
            },
            EmuRect {
                y: frame.y.saturating_add(first_extent).saturating_add(gap),
                height: second_extent,
                ..frame
            },
        ]
    }
}

struct FitTarget<'a> {
    pattern: Pattern,
    region: &'a TemplateRegion,
    frames: &'a [EmuRect],
    lane: usize,
    y: Emu,
    repeat_table_header_rows: u32,
}

impl FlowGroup<'_> {
    fn is_media(&self) -> bool {
        self.units.first().is_some_and(|unit| {
            matches!(
                unit.node.role,
                SemanticRole::Figure
                    | SemanticRole::Gallery
                    | SemanticRole::Chart
                    | SemanticRole::Diagram
                    | SemanticRole::DisplayMath
            )
        })
    }

    fn is_media_only(&self) -> bool {
        !self.units.is_empty()
            && self
                .units
                .iter()
                .all(|unit| semantic_node_is_media_only(unit.node))
    }

    fn is_table(&self) -> bool {
        self.units
            .first()
            .is_some_and(|unit| unit.node.role == SemanticRole::Table)
    }

    fn is_gallery_item(&self) -> bool {
        !self.units.is_empty() && self.units.iter().all(|unit| unit.gallery_item)
    }

    fn is_related_card(&self) -> bool {
        self.units.iter().any(|unit| {
            matches!(
                unit.node.role,
                SemanticRole::Figure | SemanticRole::Chart | SemanticRole::Diagram
            )
        }) && self
            .units
            .iter()
            .any(|unit| matches!(unit.node.content, SemanticContent::Text(_)))
    }

    fn is_peer(&self) -> bool {
        !self.is_gallery_item()
            && self.units.first().is_some_and(|unit| {
                matches!(
                    unit.node.role,
                    SemanticRole::Section
                        | SemanticRole::Statement
                        | SemanticRole::Quote
                        | SemanticRole::Definition
                        | SemanticRole::Figure
                        | SemanticRole::Chart
                        | SemanticRole::Diagram
                )
            })
    }

    fn is_breakable_flow(&self) -> bool {
        self.units.first().is_some_and(|unit| {
            matches!(
                unit.node.role,
                SemanticRole::Prose | SemanticRole::List | SemanticRole::Code
            ) && !matches!(unit.slice, FragmentSlice::Whole)
        })
    }

    fn is_code(&self) -> bool {
        self.units
            .first()
            .is_some_and(|unit| unit.node.role == SemanticRole::Code)
    }
}

fn semantic_node_is_media_only(node: &SemanticNode) -> bool {
    match node.role {
        SemanticRole::Figure | SemanticRole::Chart | SemanticRole::Diagram => true,
        SemanticRole::Gallery => match &node.content {
            SemanticContent::Children(children) => {
                !children.is_empty() && children.iter().all(semantic_node_is_media_only)
            }
            _ => false,
        },
        _ => false,
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
    width_penalty: u64,
    font_risk: bool,
    media: Option<MediaPlacement>,
    repeat_table_header_rows: u32,
}

struct FittedGroup<'a> {
    placements: Vec<FittedPlacement<'a>>,
    bottom: Emu,
}

#[derive(Clone)]
struct CandidatePage {
    end: usize,
    score: CandidateScore,
    flow_units: u64,
    demand: u64,
    topology: TopologyChoice,
    placements: Vec<PlannedRegion>,
}

struct FittedCandidate {
    placements: Vec<PlannedRegion>,
    score: CandidateScore,
    demand: u64,
    slot_count: usize,
    font_risks: Vec<StableId>,
}

#[derive(Clone)]
struct PagePlacement {
    topology: TopologyChoice,
    regions: Vec<PlannedRegion>,
}

impl Default for PagePlacement {
    fn default() -> Self {
        Self {
            topology: TopologyChoice::stack(),
            regions: Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
struct CandidateScore {
    readability_band: u32,
    crop_loss: u64,
    relation_loss: u64,
    imbalance: u64,
    orphaning: u64,
    whitespace: u64,
    complexity: u64,
    tie_break: u64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct Score {
    readability_band: u32,
    pages: usize,
    page_flow_imbalance: u64,
    page_flow_order: u64,
    page_demand_imbalance: u64,
    page_order_balance: u64,
    crop_loss: u64,
    relation_loss: u64,
    imbalance: u64,
    orphaning: u64,
    whitespace: u64,
    complexity: u64,
    tie_break: u64,
}

impl Ord for Score {
    fn cmp(&self, other: &Self) -> Ordering {
        (
            self.readability_band,
            self.pages,
            (
                self.page_flow_imbalance,
                self.page_flow_order,
                self.page_demand_imbalance,
                self.page_order_balance,
                self.crop_loss,
                self.relation_loss,
                self.imbalance,
                self.orphaning,
                self.whitespace,
                self.complexity,
                self.tie_break,
            ),
        )
            .cmp(&(
                other.readability_band,
                other.pages,
                (
                    other.page_flow_imbalance,
                    other.page_flow_order,
                    other.page_demand_imbalance,
                    other.page_order_balance,
                    other.crop_loss,
                    other.relation_loss,
                    other.imbalance,
                    other.orphaning,
                    other.whitespace,
                    other.complexity,
                    other.tie_break,
                ),
            ))
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
    pages: Vec<PagePlacement>,
    page_flow_units: Vec<u64>,
    page_demands: Vec<u64>,
}

impl Solution {
    fn prepend(&self, page: CandidatePage) -> Self {
        let mut pages = Vec::with_capacity(self.pages.len() + 1);
        pages.push(PagePlacement {
            topology: page.topology,
            regions: page.placements,
        });
        pages.extend(self.pages.clone());
        let mut page_flow_units = Vec::with_capacity(self.page_flow_units.len() + 1);
        page_flow_units.push(page.flow_units);
        page_flow_units.extend(self.page_flow_units.iter().copied());
        let max_flow_units = page_flow_units.iter().copied().max().unwrap_or(0);
        let min_flow_units = page_flow_units.iter().copied().min().unwrap_or(0);
        let page_flow_imbalance = max_flow_units
            .saturating_sub(min_flow_units)
            .saturating_mul(1_000)
            / max_flow_units.max(1);
        let page_flow_order = page_flow_units.windows(2).fold(0u64, |penalty, pair| {
            penalty.saturating_add(
                pair[1].saturating_sub(pair[0]).saturating_mul(1_000) / pair[1].max(1),
            )
        });
        let mut page_demands = Vec::with_capacity(self.page_demands.len() + 1);
        page_demands.push(page.demand);
        page_demands.extend(self.page_demands.iter().copied());
        let max_demand = page_demands.iter().copied().max().unwrap_or(0);
        let min_demand = page_demands.iter().copied().min().unwrap_or(0);
        let page_demand_imbalance =
            max_demand.saturating_sub(min_demand).saturating_mul(1_000) / max_demand.max(1);
        let page_order_balance = page_demands.windows(2).fold(0u64, |penalty, pair| {
            penalty.saturating_add(
                pair[1].saturating_sub(pair[0]).saturating_mul(1_000) / pair[1].max(1),
            )
        });
        Self {
            score: Score {
                readability_band: self.score.readability_band.max(page.score.readability_band),
                pages: self.score.pages + 1,
                page_flow_imbalance,
                page_flow_order,
                page_demand_imbalance,
                page_order_balance,
                crop_loss: self.score.crop_loss.saturating_add(page.score.crop_loss),
                relation_loss: self
                    .score
                    .relation_loss
                    .saturating_add(page.score.relation_loss),
                imbalance: self.score.imbalance.saturating_add(page.score.imbalance),
                orphaning: self.score.orphaning.saturating_add(page.score.orphaning),
                whitespace: self.score.whitespace.saturating_add(page.score.whitespace),
                complexity: self.score.complexity.saturating_add(page.score.complexity),
                tie_break: self.score.tie_break.saturating_add(page.score.tie_break),
            },
            pages,
            page_flow_units,
            page_demands,
        }
    }
}

struct CandidateScoreInput<'a, 'nodes> {
    pattern: Pattern,
    groups: &'a [FlowGroup<'nodes>],
    frames: &'a [EmuRect],
    cursors: &'a [Emu],
    reductions: &'a [u32],
    width_loss: u64,
    relation_loss: u64,
    font_step: u32,
}

fn candidate_score(input: CandidateScoreInput<'_, '_>) -> CandidateScore {
    let CandidateScoreInput {
        pattern,
        groups,
        frames,
        cursors,
        reductions,
        width_loss,
        relation_loss,
        font_step,
    } = input;
    let used_heights = frames
        .iter()
        .zip(cursors)
        .map(|(frame, cursor)| cursor.saturating_sub(frame.y).min(frame.height).max(0) as u64)
        .collect::<Vec<_>>();
    let max_used = used_heights.iter().copied().max().unwrap_or(0);
    let min_used = used_heights.iter().copied().min().unwrap_or(0);
    let imbalance = max_used.saturating_sub(min_used).saturating_mul(1_000) / max_used.max(1);
    let available = frames
        .iter()
        .map(|frame| frame.height.max(0) as u64)
        .sum::<u64>()
        .max(1);
    let used = used_heights.iter().sum::<u64>().min(available);
    let whitespace_ratio = available.saturating_sub(used).saturating_mul(1_000) / available;
    let whitespace = whitespace_ratio.saturating_mul(whitespace_ratio);
    let orphaning = u64::from(groups.len() == 1 && groups[0].is_breakable_flow());
    let readability_band = reductions
        .iter()
        .copied()
        .max()
        .unwrap_or(0)
        .div_ceil(font_step.max(1));
    let source_order_balance = used_heights.windows(2).fold(0u64, |penalty, pair| {
        penalty.saturating_add(pair[1].saturating_sub(pair[0]))
    });
    CandidateScore {
        readability_band,
        crop_loss: width_loss,
        relation_loss,
        imbalance,
        orphaning,
        whitespace,
        complexity: pattern.complexity(),
        tie_break: pattern
            .tie_break()
            .saturating_mul(1_000_000)
            .saturating_add(source_order_balance),
    }
}

fn width_penalty(preferred: Emu, available: Emu) -> u64 {
    if preferred <= available || preferred <= 0 {
        return 0;
    }
    u64::try_from(preferred.saturating_sub(available))
        .unwrap_or(u64::MAX)
        .saturating_mul(100)
        / u64::try_from(preferred).unwrap_or(u64::MAX).max(1)
}

fn inset_frame(frame: EmuRect, margins: wasmppt_deck::TextMargins) -> EmuRect {
    EmuRect {
        x: frame.x.saturating_add(margins.left),
        y: frame.y.saturating_add(margins.top),
        width: frame
            .width
            .saturating_sub(margins.left)
            .saturating_sub(margins.right)
            .max(1),
        height: frame
            .height
            .saturating_sub(margins.top)
            .saturating_sub(margins.bottom)
            .max(1),
    }
}

fn cover_crop_penalty(size: PixelSize, frame: EmuRect) -> u64 {
    let image_width = u128::from(size.width);
    let image_height = u128::from(size.height);
    let frame_width = u128::try_from(frame.width.max(1)).unwrap_or(u128::MAX);
    let frame_height = u128::try_from(frame.height.max(1)).unwrap_or(u128::MAX);
    let (visible, complete) =
        if image_width.saturating_mul(frame_height) > frame_width.saturating_mul(image_height) {
            (
                frame_width.saturating_mul(image_height),
                frame_height.saturating_mul(image_width),
            )
        } else {
            (
                frame_height.saturating_mul(image_width),
                frame_width.saturating_mul(image_height),
            )
        };
    let loss = complete.saturating_sub(visible).saturating_mul(1_000) / complete.max(1);
    u64::try_from(loss).unwrap_or(u64::MAX)
}

fn contain_whitespace_penalty(media: MediaPlacement) -> u64 {
    if media.fit != ContentFit::Contain {
        return 0;
    }
    let slot_area = u128::try_from(media.slot.width.max(1))
        .unwrap_or(u128::MAX)
        .saturating_mul(u128::try_from(media.slot.height.max(1)).unwrap_or(u128::MAX));
    let visible_area = u128::try_from(media.visible_frame.width.max(1))
        .unwrap_or(u128::MAX)
        .saturating_mul(u128::try_from(media.visible_frame.height.max(1)).unwrap_or(u128::MAX));
    let unused = slot_area.saturating_sub(visible_area).saturating_mul(1_000) / slot_area.max(1);
    u64::try_from(unused).unwrap_or(u64::MAX)
}

fn relation_penalty(
    relations: &[MediaTextRelation],
    placements: &[PlannedRegion],
    page_frame: EmuRect,
) -> u64 {
    relations.iter().fold(0u64, |penalty, relation| {
        let media = fragment_frame(placements, relation.media_node_id);
        let text = fragment_frame(placements, relation.text_node_id);
        let (Some(media), Some(text)) = (media, text) else {
            return penalty;
        };
        let media_center = (
            media.x.saturating_add(media.width / 2),
            media.y.saturating_add(media.height / 2),
        );
        let text_center = (
            text.x.saturating_add(text.width / 2),
            text.y.saturating_add(text.height / 2),
        );
        let dx = media_center.0.abs_diff(text_center.0);
        let dy = media_center.1.abs_diff(text_center.1);
        let distance = dx.saturating_add(dy).saturating_mul(1_000)
            / page_frame
                .width
                .abs_diff(0)
                .saturating_add(page_frame.height.abs_diff(0))
                .max(1);
        let text_is_before = if dx >= dy {
            text_center.0 <= media_center.0
        } else {
            text_center.1 <= media_center.1
        };
        let expected_before = relation.text_side == MediaTextSide::BeforeMedia;
        let side_loss = if text_is_before == expected_before {
            0
        } else {
            250
        };
        let strength = if relation.explicit_caption {
            8
        } else {
            match relation.proximity {
                MediaTextProximity::SameParagraph => 4,
                MediaTextProximity::AdjacentBlocks => 2,
                MediaTextProximity::BlankSeparatedBlocks => 1,
            }
        };
        penalty.saturating_add(distance.saturating_add(side_loss).saturating_mul(strength))
    })
}

fn fragment_frame(placements: &[PlannedRegion], node_id: StableId) -> Option<EmuRect> {
    placements
        .iter()
        .flat_map(|region| &region.fragments)
        .find(|fragment| fragment.source_node_id == node_id)
        .map(|fragment| fragment.frame)
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
    let role = if slide.kind == LogicalSlideKind::Content
        && !slide
            .nodes
            .first()
            .is_some_and(|node| matches!(node.role, SemanticRole::Title | SemanticRole::Section))
        && slide.nodes.iter().any(|node| {
            matches!(
                node.role,
                SemanticRole::Statement | SemanticRole::Quote | SemanticRole::DisplayMath
            )
        }) {
        TemplateLayoutCapability::Statement
    } else {
        match slide.kind {
            LogicalSlideKind::Title => TemplateLayoutCapability::Title,
            LogicalSlideKind::Content => TemplateLayoutCapability::ContentEnvelope,
        }
    };
    template
        .layouts
        .iter()
        .find(|layout| layout.capability == role)
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
    layout: TemplateLayoutCapability,
) -> (Vec<&SemanticNode>, &[SemanticNode]) {
    let header_count = usize::from(nodes.first().is_some_and(|node| match layout {
        TemplateLayoutCapability::Title => node.role == SemanticRole::Title,
        TemplateLayoutCapability::Statement => false,
        TemplateLayoutCapability::ContentEnvelope => {
            matches!(node.role, SemanticRole::Title | SemanticRole::Section)
        }
    }));
    (
        nodes[..header_count].iter().collect(),
        &nodes[header_count..],
    )
}

fn primary_region<'a>(
    layout: TemplateLayoutCapability,
    regions: &[&'a TemplateRegion],
) -> Option<&'a TemplateRegion> {
    let preferred = match layout {
        TemplateLayoutCapability::Title => RegionRole::Subtitle,
        TemplateLayoutCapability::ContentEnvelope => RegionRole::Body,
        TemplateLayoutCapability::Statement => RegionRole::Statement,
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
    placement: RegionPlacement,
    node: &SemanticNode,
    slice: FragmentSlice,
    choice: FragmentPlacement,
) -> PlannedRegion {
    let FragmentPlacement {
        frame,
        font_size,
        media,
        repeat_table_header_rows,
    } = choice;
    let planned_region_frame = media.map_or(frame, |media| media.slot);
    PlannedRegion {
        template_region_id: region.id,
        placement,
        frame: planned_region_frame,
        fragments: vec![PlannedFragment {
            id: PlannedFragment::expected_id(node.id, slice),
            source_node_id: node.id,
            slice,
            frame,
            type_choice: TypeChoice {
                font_size: if is_text(node) { font_size } else { 0 },
            },
            media,
            repeat_table_header_rows,
        }],
    }
}

#[derive(Clone, Copy)]
struct FragmentPlacement {
    frame: EmuRect,
    font_size: u32,
    media: Option<MediaPlacement>,
    repeat_table_header_rows: u32,
}

fn content_fit(pattern: Pattern, node: &SemanticNode, gallery_item: bool) -> ContentFit {
    match &node.content {
        SemanticContent::Image(_)
            if gallery_item
                && matches!(
                    pattern,
                    Pattern::Gallery2 | Pattern::Gallery4 | Pattern::Gallery6
                ) =>
        {
            ContentFit::Cover
        }
        SemanticContent::Image(_) | SemanticContent::Svg(_) | SemanticContent::Chart(_) => {
            ContentFit::Contain
        }
        _ => ContentFit::None,
    }
}

fn push_coalesced_region(regions: &mut Vec<PlannedRegion>, next: PlannedRegion) {
    let Some(previous) = regions.last_mut() else {
        regions.push(next);
        return;
    };
    if previous.template_region_id != next.template_region_id
        || previous.placement != next.placement
        || previous.fragments.len() != 1
        || next.fragments.len() != 1
    {
        regions.push(next);
        return;
    }
    let previous_fragment = &mut previous.fragments[0];
    let next_fragment = &next.fragments[0];
    let Some(slice) = contiguous_slice(previous_fragment.slice, next_fragment.slice) else {
        regions.push(next);
        return;
    };
    if previous_fragment.source_node_id != next_fragment.source_node_id
        || previous_fragment.type_choice != next_fragment.type_choice
        || previous_fragment.media != next_fragment.media
        || previous_fragment.frame.x != next_fragment.frame.x
        || previous_fragment.frame.width != next_fragment.frame.width
        || (previous_fragment.repeat_table_header_rows != next_fragment.repeat_table_header_rows
            && next_fragment.repeat_table_header_rows != 0)
    {
        regions.push(next);
        return;
    }
    let Some(bottom) = next_fragment
        .frame
        .y
        .checked_add(next_fragment.frame.height)
    else {
        regions.push(next);
        return;
    };
    let height = bottom.saturating_sub(previous_fragment.frame.y);
    previous_fragment.slice = slice;
    previous_fragment.id = PlannedFragment::expected_id(previous_fragment.source_node_id, slice);
    previous_fragment.frame.height = height;
    previous.frame.height = height;
}

fn contiguous_slice(left: FragmentSlice, right: FragmentSlice) -> Option<FragmentSlice> {
    match (left, right) {
        (
            FragmentSlice::Text { start, end },
            FragmentSlice::Text {
                start: right_start,
                end: right_end,
            },
        ) if end == right_start => Some(FragmentSlice::Text {
            start,
            end: right_end,
        }),
        (
            FragmentSlice::ListItems { start, end },
            FragmentSlice::ListItems {
                start: right_start,
                end: right_end,
            },
        ) if end == right_start => Some(FragmentSlice::ListItems {
            start,
            end: right_end,
        }),
        (
            FragmentSlice::TableRows { start, end },
            FragmentSlice::TableRows {
                start: right_start,
                end: right_end,
            },
        ) if end == right_start => Some(FragmentSlice::TableRows {
            start,
            end: right_end,
        }),
        (
            FragmentSlice::CodeLines { start, end },
            FragmentSlice::CodeLines {
                start: right_start,
                end: right_end,
            },
        ) if end == right_start => Some(FragmentSlice::CodeLines {
            start,
            end: right_end,
        }),
        _ => None,
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
    digest.update(b"wasmppt/deck-layout/plan/v3\0");
    match spec.encode(limits) {
        Ok(encoded) => digest.update(Sha256::digest(encoded)),
        Err(_) => digest.update(spec.id.as_bytes()),
    }
    digest.update(template.id.as_bytes());
    digest.update(template.cache_key);
    digest.update(fonts.identity);
    digest.update(policy.readable_floor.to_le_bytes());
    digest.update(policy.readable_media_floor.to_le_bytes());
    digest.update(policy.max_cover_crop_per_mille.to_le_bytes());
    digest.update(policy.font_step.to_le_bytes());
    digest.update(policy.gap.to_le_bytes());
    digest.update((policy.limits.max_font_faces as u64).to_le_bytes());
    digest.update((policy.limits.max_font_bytes as u64).to_le_bytes());
    digest.update((policy.limits.max_flow_units as u64).to_le_bytes());
    digest.update((policy.limits.max_candidate_assignments as u64).to_le_bytes());
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
        DeckResource, ImageContent, ListContent, ListItem, PixelSize, PlaceholderIdentity,
        ResourceKind, RichText, RichTextRun, SourceRange, SplitPolicy, TableCell, TableColumn,
        TableContent, TableRow, TemplateTextLevel, TemplateTheme, TextMargins, TextMarks,
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
    fn title_details_share_one_non_overlapping_cover_flow() {
        let mut spec = spec(vec![
            text_node(3, SemanticRole::Title, SplitPolicy::Never, "Title"),
            text_node(4, SemanticRole::Subtitle, SplitPolicy::Text, "Alan Kang"),
            text_node(5, SemanticRole::Prose, SplitPolicy::Text, "2025-08-01"),
        ]);
        spec.logical_slides[0].kind = LogicalSlideKind::Title;
        let template = title_template(2_500_000);

        let plan = DeckPlanner::default()
            .plan(&spec, &template, &FontCatalog::default(), &limits())
            .unwrap();
        let details = fragments(&plan.pages[0])
            .filter(|fragment| {
                matches!(fragment.source_node_id, value if value == id(4) || value == id(5))
            })
            .collect::<Vec<_>>();

        assert_eq!(details.len(), 2);
        assert!(details[0].frame.y.saturating_add(details[0].frame.height) <= details[1].frame.y);
        assert!(validate_deck_plan(&spec, &template, &plan, &limits()).is_valid());
    }

    #[test]
    fn titled_display_math_keeps_the_content_layout_and_heading() {
        let spec = spec(vec![
            text_node(3, SemanticRole::Title, SplitPolicy::Never, "Result"),
            SemanticNode {
                id: id(4),
                source: range(40),
                role: SemanticRole::DisplayMath,
                split: SplitPolicy::Never,
                content: SemanticContent::Svg(wasmppt_deck::SvgContent {
                    resource_id: id(90),
                    source_text: Some("y=mx+b".to_owned()),
                }),
            },
        ]);
        let mut spec = spec;
        spec.resources.push(DeckResource {
            id: id(90),
            kind: ResourceKind::Svg,
            media_type: "image/svg+xml".to_owned(),
            bytes: br#"<svg xmlns="http://www.w3.org/2000/svg"/>"#.to_vec(),
            intrinsic_size: Some(PixelSize {
                width: 100,
                height: 20,
            }),
        });
        let mut template = template_with_statement(5_500_000);
        let body = template
            .regions
            .iter_mut()
            .find(|region| region.role == RegionRole::Body)
            .unwrap();
        let safe = body.frame;
        let body_id = body.id;
        body.bleed_frame = Some(EmuRect {
            x: 100_000,
            y: 1_000_000,
            width: 9_800_000,
            height: 6_000_000,
        });

        let plan = DeckPlanner::default()
            .plan(&spec, &template, &FontCatalog::default(), &limits())
            .unwrap();

        assert_eq!(plan.pages[0].template_layout_id, id(100));
        assert_eq!(plan.pages[0].regions[0].fragments[0].source_node_id, id(3));
        assert!(
            plan.pages[0]
                .regions
                .iter()
                .filter(|region| region.template_region_id == body_id)
                .all(|region| region.frame.is_within(safe))
        );
        assert!(validate_deck_plan(&spec, &template, &plan, &limits()).is_valid());
    }

    #[test]
    fn nested_section_headings_flow_through_the_content_body() {
        let spec = spec(vec![
            text_node(3, SemanticRole::Title, SplitPolicy::Never, "Comparison"),
            text_node(4, SemanticRole::Section, SplitPolicy::Never, "Control"),
            text_node(
                5,
                SemanticRole::Prose,
                SplitPolicy::Text,
                "Stable baseline.",
            ),
            text_node(6, SemanticRole::Section, SplitPolicy::Never, "Treatment"),
            text_node(
                7,
                SemanticRole::Prose,
                SplitPolicy::Text,
                "Measured improvement.",
            ),
        ]);
        let template = template(5_500_000);

        let plan = DeckPlanner::default()
            .plan(&spec, &template, &FontCatalog::default(), &limits())
            .unwrap();

        assert_eq!(plan.pages[0].regions[0].fragments[0].source_node_id, id(3));
        assert!(fragments(&plan.pages[0]).any(|fragment| fragment.source_node_id == id(4)));
        assert!(fragments(&plan.pages[0]).any(|fragment| fragment.source_node_id == id(6)));
        assert!(validate_deck_plan(&spec, &template, &plan, &limits()).is_valid());
    }

    #[test]
    fn peer_sections_are_assigned_to_distinct_topology_slots() {
        let spec = spec(vec![
            text_node(3, SemanticRole::Title, SplitPolicy::Never, "Comparison"),
            text_node(4, SemanticRole::Section, SplitPolicy::Never, "Control"),
            text_node(
                5,
                SemanticRole::Prose,
                SplitPolicy::Text,
                "Stable baseline.",
            ),
            text_node(6, SemanticRole::Section, SplitPolicy::Never, "Treatment"),
            text_node(
                7,
                SemanticRole::Prose,
                SplitPolicy::Text,
                "Measured improvement.",
            ),
        ]);

        let plan = DeckPlanner::default()
            .plan(
                &spec,
                &template(5_500_000),
                &FontCatalog::default(),
                &limits(),
            )
            .unwrap();
        let slots = [id(4), id(6)]
            .into_iter()
            .map(|node_id| {
                plan.pages[0]
                    .regions
                    .iter()
                    .find(|region| {
                        region
                            .fragments
                            .iter()
                            .any(|fragment| fragment.source_node_id == node_id)
                    })
                    .map(|region| match region.placement {
                        RegionPlacement::Slot(slot) => slot,
                        RegionPlacement::Fixed => u16::MAX,
                    })
                    .unwrap()
            })
            .collect::<BTreeSet<_>>();

        assert_eq!(slots.len(), 2);
        assert_ne!(plan.pages[0].topology.kind, LayoutTopology::Stack);
        assert!(validate_deck_plan(&spec, &template(5_500_000), &plan, &limits()).is_valid());
    }

    #[test]
    fn media_text_source_orders_evaluate_both_mirrored_assignments() {
        let prose = text_node(4, SemanticRole::Prose, SplitPolicy::Text, "Explanation");
        let figure = figure_node(5, 90, "diagram");
        for nodes in [
            vec![prose.clone(), figure.clone()],
            vec![figure.clone(), prose.clone()],
        ] {
            let units = build_flow(&nodes, &[], 16).unwrap();
            let groups = group_units(&units);
            let start = Pattern::MediaStart.assignments(&groups, 2);
            let end = Pattern::MediaEnd.assignments(&groups, 2);

            assert_eq!(start.len(), 1);
            assert_eq!(end.len(), 1);
            for (index, group) in groups.iter().enumerate() {
                assert_eq!(start[0][index] == 0, group.is_media());
                assert_eq!(end[0][index] == 1, group.is_media());
            }
        }
    }

    #[test]
    fn gallery_frames_follow_portrait_and_wide_aspects_without_unsafe_crop() {
        for (aspects, horizontal) in [
            (vec![(600, 1_000), (600, 1_000)], true),
            (vec![(2_000, 600), (2_000, 600)], false),
        ] {
            let spec = gallery_spec(&aspects);
            let plan = DeckPlanner::default()
                .plan(
                    &spec,
                    &template(5_500_000),
                    &FontCatalog::default(),
                    &limits(),
                )
                .unwrap();
            let media = fragments(&plan.pages[0])
                .filter(|fragment| fragment.media.is_some())
                .collect::<Vec<_>>();

            assert_eq!(plan.pages[0].topology.kind, LayoutTopology::Gallery);
            assert_eq!(media.len(), 2);
            assert!(media.iter().all(|fragment| {
                let placement = fragment.media.unwrap();
                placement.fit != ContentFit::Cover
                    || cover_crop_penalty(placement.source_size, placement.slot)
                        <= u64::from(PlannerPolicy::default().max_cover_crop_per_mille)
            }));
            if horizontal {
                assert_eq!(media[0].frame.y, media[1].frame.y);
                assert_ne!(media[0].frame.x, media[1].frame.x);
            } else {
                assert_eq!(media[0].frame.x, media[1].frame.x);
                assert_ne!(media[0].frame.y, media[1].frame.y);
            }
            assert!(validate_deck_plan(&spec, &template(5_500_000), &plan, &limits()).is_valid());
        }
    }

    #[test]
    fn media_only_content_uses_bleed_but_mixed_content_stays_in_the_safe_envelope() {
        let mut template = template(5_500_000);
        let safe = template.regions[1].frame;
        let bleed = EmuRect {
            x: 100_000,
            y: 1_000_000,
            width: 9_800_000,
            height: 6_000_000,
        };
        template.regions[1].bleed_frame = Some(bleed);
        let body_id = template.regions[1].id;

        let media_plan = DeckPlanner::default()
            .plan(
                &gallery_spec(&[(1_600, 900), (900, 1_600)]),
                &template,
                &FontCatalog::default(),
                &limits(),
            )
            .unwrap();
        let media_regions = media_plan.pages[0]
            .regions
            .iter()
            .filter(|region| region.template_region_id == body_id)
            .collect::<Vec<_>>();
        assert!(
            media_regions
                .iter()
                .all(|region| region.frame.is_within(bleed))
        );
        assert!(
            media_regions
                .iter()
                .any(|region| !region.frame.is_within(safe))
        );

        let mixed_spec = single_media_spec(
            (1_600, 900),
            Some("A related paragraph keeps image and copy inside the conservative safe area."),
        );
        let mixed_plan = DeckPlanner::default()
            .plan(&mixed_spec, &template, &FontCatalog::default(), &limits())
            .unwrap();
        assert!(
            mixed_plan.pages[0]
                .regions
                .iter()
                .filter(|region| region.template_region_id == body_id)
                .all(|region| region.frame.is_within(safe))
        );
        assert!(validate_deck_plan(&mixed_spec, &template, &mixed_plan, &limits()).is_valid());
    }

    #[test]
    fn gallery_captions_stay_with_their_figures_without_overlap() {
        let mut spec = gallery_spec(&[(1_600, 900), (900, 1_600)]);
        let SemanticContent::Children(children) = &mut spec.logical_slides[0].nodes[1].content
        else {
            unreachable!();
        };
        for index in (0..2usize).rev() {
            let mut caption = text_node(
                20 + u8::try_from(index).unwrap(),
                SemanticRole::Caption,
                SplitPolicy::Text,
                &format!("Caption {index}"),
            );
            caption.source = SourceRange::new(
                "deck.md",
                415 + u32::try_from(index).unwrap() * 20,
                419 + u32::try_from(index).unwrap() * 20,
            );
            children[index].source = SourceRange::new(
                "deck.md",
                410 + u32::try_from(index).unwrap() * 20,
                414 + u32::try_from(index).unwrap() * 20,
            );
            children.insert(index + 1, caption);
        }
        let mut template = template(5_500_000);
        let safe = template.regions[1].frame;
        template.regions[1].bleed_frame = Some(EmuRect {
            x: 100_000,
            y: 1_000_000,
            width: 9_800_000,
            height: 6_000_000,
        });
        let body_id = template.regions[1].id;
        let plan = DeckPlanner::default()
            .plan(&spec, &template, &FontCatalog::default(), &limits())
            .unwrap();

        for (figure, caption) in [(id(10), id(20)), (id(11), id(21))] {
            let figure_region = plan.pages[0]
                .regions
                .iter()
                .find(|region| {
                    region
                        .fragments
                        .iter()
                        .any(|fragment| fragment.source_node_id == figure)
                })
                .unwrap();
            let caption_region = plan.pages[0]
                .regions
                .iter()
                .find(|region| {
                    region
                        .fragments
                        .iter()
                        .any(|fragment| fragment.source_node_id == caption)
                })
                .unwrap();
            assert_eq!(figure_region.placement, caption_region.placement);
            assert!(
                figure_region
                    .frame
                    .y
                    .saturating_add(figure_region.frame.height)
                    <= caption_region.frame.y,
                "figure {:?} at {:?} overlaps caption {:?} at {:?}",
                figure,
                figure_region.frame,
                caption,
                caption_region.frame
            );
        }
        assert!(
            plan.pages[0]
                .regions
                .iter()
                .filter(|region| region.template_region_id == body_id)
                .all(|region| region.frame.is_within(safe))
        );
        assert!(validate_deck_plan(&spec, &template, &plan, &limits()).is_valid());
    }

    #[test]
    fn ten_mixed_gallery_captions_paginate_with_their_figures() {
        let aspects = [
            (2_400, 600),
            (600, 1_600),
            (1_000, 1_000),
            (1_800, 900),
            (800, 1_400),
            (2_000, 700),
            (700, 1_500),
            (1_100, 1_000),
            (1_600, 900),
            (900, 1_600),
        ];
        let mut spec = gallery_spec(&aspects);
        {
            let SemanticContent::Children(children) = &mut spec.logical_slides[0].nodes[1].content
            else {
                unreachable!();
            };
            for index in (0..aspects.len()).rev() {
                let identity = u8::try_from(index).unwrap();
                let mut caption = text_node(
                    30 + identity,
                    SemanticRole::Caption,
                    SplitPolicy::Text,
                    &format!("Caption {index}"),
                );
                caption.source = SourceRange::new(
                    "deck.md",
                    419 + u32::try_from(index).unwrap() * 10,
                    420 + u32::try_from(index).unwrap() * 10,
                );
                children.insert(index + 1, caption);
            }
        }
        spec.logical_slides[0].media_text_relations = (0..aspects.len())
            .map(|index| {
                let identity = u8::try_from(index).unwrap();
                MediaTextRelation {
                    media_node_id: id(10 + identity),
                    text_node_id: id(30 + identity),
                    proximity: MediaTextProximity::AdjacentBlocks,
                    text_side: MediaTextSide::AfterMedia,
                    explicit_caption: true,
                }
            })
            .collect();
        let plan = DeckPlanner::default()
            .plan(
                &spec,
                &template(5_500_000),
                &FontCatalog::default(),
                &limits(),
            )
            .unwrap();

        assert!(plan.pages.len() >= 2);
        for index in 0..aspects.len() {
            let identity = u8::try_from(index).unwrap();
            let figure = id(10 + identity);
            let caption = id(30 + identity);
            let page_id = page_containing(&plan, figure);
            assert_eq!(page_id, page_containing(&plan, caption));
            let page = plan.pages.iter().find(|page| page.id == page_id).unwrap();
            let placement = |node| {
                page.regions
                    .iter()
                    .find(|region| {
                        region
                            .fragments
                            .iter()
                            .any(|fragment| fragment.source_node_id == node)
                    })
                    .unwrap()
                    .placement
            };
            assert_eq!(placement(figure), placement(caption));
        }
        assert!(validate_deck_plan(&spec, &template(5_500_000), &plan, &limits()).is_valid());
    }

    #[test]
    fn ten_mixed_gallery_items_with_sparse_captions_balance_across_pages() {
        let aspects = [
            (1_000, 1_000),
            (1_000, 1_000),
            (500, 1_000),
            (2_667, 1_000),
            (1_000, 1_000),
            (1_000, 1_000),
            (500, 1_000),
            (2_667, 1_000),
            (1_000, 1_000),
            (1_000, 1_000),
        ];
        let mut spec = gallery_spec(&aspects);
        let SemanticContent::Children(children) = &mut spec.logical_slides[0].nodes[1].content
        else {
            unreachable!();
        };
        for index in [7usize, 2] {
            let identity = u8::try_from(index).unwrap();
            let mut caption = text_node(
                40 + identity,
                SemanticRole::Caption,
                SplitPolicy::Never,
                &format!("Caption {index}"),
            );
            caption.source = SourceRange::new(
                "deck.md",
                419 + u32::try_from(index).unwrap() * 10,
                420 + u32::try_from(index).unwrap() * 10,
            );
            children.insert(index + 1, caption);
        }
        let plan = DeckPlanner::default()
            .plan(
                &spec,
                &template(5_500_000),
                &FontCatalog::default(),
                &limits(),
            )
            .unwrap();
        let loads = plan
            .pages
            .iter()
            .map(|page| {
                fragments(page)
                    .filter(|fragment| fragment.media.is_some())
                    .count()
            })
            .collect::<Vec<_>>();

        assert_eq!(loads.iter().sum::<usize>(), 10);
        assert_eq!(loads.len(), 3);
        assert!(loads.iter().max().unwrap() - loads.iter().min().unwrap() <= 1);
        assert!(validate_deck_plan(&spec, &template(5_500_000), &plan, &limits()).is_valid());
    }

    #[test]
    fn galleries_from_two_to_ten_items_use_balanced_bounded_pages() {
        for item_count in 2usize..=10 {
            let spec = gallery_spec(&vec![(1_000, 1_000); item_count]);
            let plan = DeckPlanner::default()
                .plan(
                    &spec,
                    &template(5_500_000),
                    &FontCatalog::default(),
                    &limits(),
                )
                .unwrap();
            let loads = plan
                .pages
                .iter()
                .map(|page| {
                    assert_eq!(page.topology.kind, LayoutTopology::Gallery);
                    fragments(page)
                        .filter(|fragment| fragment.media.is_some())
                        .count()
                })
                .collect::<Vec<_>>();

            assert_eq!(loads.iter().sum::<usize>(), item_count);
            assert!(loads.iter().all(|load| (2..=6).contains(load)));
            assert!(loads.iter().max().unwrap() - loads.iter().min().unwrap() <= 1);
            assert!(validate_deck_plan(&spec, &template(5_500_000), &plan, &limits()).is_valid());
        }
    }

    #[test]
    fn mixed_aspect_gallery_uses_bounded_crop_and_non_uniform_tracks() {
        let spec = gallery_spec(&[
            (2_400, 600),
            (600, 1_600),
            (1_000, 1_000),
            (1_800, 900),
            (800, 1_400),
        ]);
        let plan = DeckPlanner::default()
            .plan(
                &spec,
                &template(5_500_000),
                &FontCatalog::default(),
                &limits(),
            )
            .unwrap();
        let placements = plan
            .pages
            .iter()
            .flat_map(fragments)
            .filter_map(|fragment| fragment.media)
            .collect::<Vec<_>>();

        assert_eq!(placements.len(), 5);
        assert!(placements.iter().all(|placement| {
            placement.fit != ContentFit::Cover
                || cover_crop_penalty(placement.source_size, placement.slot)
                    <= u64::from(PlannerPolicy::default().max_cover_crop_per_mille)
        }));
        assert!(
            placements
                .iter()
                .map(|placement| placement.slot.width)
                .collect::<BTreeSet<_>>()
                .len()
                > 1
                || placements
                    .iter()
                    .map(|placement| placement.slot.height)
                    .collect::<BTreeSet<_>>()
                    .len()
                    > 1
        );
        assert!(validate_deck_plan(&spec, &template(5_500_000), &plan, &limits()).is_valid());
    }

    #[test]
    fn adjacent_media_text_relations_can_form_source_ordered_cards() {
        let mut spec = spec_with_resources(
            vec![
                text_node(3, SemanticRole::Title, SplitPolicy::Never, "Cards"),
                figure_node(4, 90, "wide photo"),
                text_node(
                    5,
                    SemanticRole::Prose,
                    SplitPolicy::Text,
                    "First explanation.",
                ),
                figure_node(6, 91, "portrait photo"),
                text_node(
                    7,
                    SemanticRole::Prose,
                    SplitPolicy::Text,
                    "Second explanation.",
                ),
            ],
            vec![
                DeckResource {
                    id: id(90),
                    kind: ResourceKind::RasterImage,
                    media_type: "image/png".to_owned(),
                    bytes: vec![1],
                    intrinsic_size: Some(PixelSize {
                        width: 1_600,
                        height: 900,
                    }),
                },
                DeckResource {
                    id: id(91),
                    kind: ResourceKind::RasterImage,
                    media_type: "image/png".to_owned(),
                    bytes: vec![1],
                    intrinsic_size: Some(PixelSize {
                        width: 900,
                        height: 1_600,
                    }),
                },
            ],
        );
        spec.logical_slides[0].media_text_relations = vec![
            MediaTextRelation {
                media_node_id: id(4),
                text_node_id: id(5),
                proximity: MediaTextProximity::AdjacentBlocks,
                text_side: MediaTextSide::AfterMedia,
                explicit_caption: false,
            },
            MediaTextRelation {
                media_node_id: id(6),
                text_node_id: id(7),
                proximity: MediaTextProximity::AdjacentBlocks,
                text_side: MediaTextSide::AfterMedia,
                explicit_caption: false,
            },
        ];

        let units = build_flow(
            &spec.logical_slides[0].nodes[1..],
            &spec.logical_slides[0].media_text_relations,
            16,
        )
        .unwrap();
        let groups = group_units(&units);
        let cards =
            related_media_text_groups(&groups, &spec.logical_slides[0].media_text_relations)
                .unwrap();
        assert_eq!(cards.len(), 2);
        assert!(cards.iter().all(FlowGroup::is_related_card));

        let plan = DeckPlanner::default()
            .plan(
                &spec,
                &template(5_500_000),
                &FontCatalog::default(),
                &limits(),
            )
            .unwrap();
        let slots = [id(4), id(5), id(6), id(7)].map(|node| {
            plan.pages[0]
                .regions
                .iter()
                .find(|region| {
                    region
                        .fragments
                        .iter()
                        .any(|fragment| fragment.source_node_id == node)
                })
                .unwrap()
                .placement
        });
        assert_eq!(slots[0], slots[1]);
        assert_eq!(slots[2], slots[3]);
        assert_ne!(slots[0], slots[2]);
        assert_eq!(plan.pages[0].topology.kind, LayoutTopology::PeerGrid);
        assert!(validate_deck_plan(&spec, &template(5_500_000), &plan, &limits()).is_valid());
    }

    #[test]
    fn extreme_portrait_and_short_related_copy_share_one_readable_page() {
        let mut spec = spec_with_resources(
            vec![
                text_node(3, SemanticRole::Title, SplitPolicy::Never, "Portrait"),
                figure_node(4, 90, "extreme portrait"),
                text_node(
                    5,
                    SemanticRole::Prose,
                    SplitPolicy::Never,
                    "Short copy explains the portrait without overwhelming it.",
                ),
            ],
            vec![DeckResource {
                id: id(90),
                kind: ResourceKind::RasterImage,
                media_type: "image/png".to_owned(),
                bytes: vec![1],
                intrinsic_size: Some(PixelSize {
                    width: 64,
                    height: 256,
                }),
            }],
        );
        spec.logical_slides[0].media_text_relations = vec![MediaTextRelation {
            media_node_id: id(4),
            text_node_id: id(5),
            proximity: MediaTextProximity::AdjacentBlocks,
            text_side: MediaTextSide::AfterMedia,
            explicit_caption: false,
        }];
        let template = template(5_500_000);
        let plan = DeckPlanner::default()
            .plan(&spec, &template, &FontCatalog::default(), &limits())
            .unwrap();

        assert_eq!(plan.pages.len(), 1);
        let page = &plan.pages[0];
        let located = |node| {
            page.regions
                .iter()
                .find_map(|region| {
                    region
                        .fragments
                        .iter()
                        .find(|fragment| fragment.source_node_id == node)
                        .map(|fragment| (region.placement, fragment))
                })
                .unwrap()
        };
        let (media_slot, media) = located(id(4));
        let (text_slot, text) = located(id(5));
        assert_eq!(media_slot, text_slot);
        assert!(
            media.frame.x.saturating_add(media.frame.width) <= text.frame.x,
            "portrait and related copy overlap: {:?} {:?}",
            media.frame,
            text.frame
        );
        assert!(
            media.frame.width.min(media.frame.height)
                >= PlannerPolicy::default().readable_media_floor
        );
        assert!(text.type_choice.font_size >= PlannerPolicy::default().readable_floor);
        assert!(validate_deck_plan(&spec, &template, &plan, &limits()).is_valid());
    }

    #[test]
    fn data_bearing_media_contains_while_gallery_photos_cover() {
        let mut spec = gallery_spec(&[(1_600, 900), (900, 1_600)]);
        let gallery = spec.logical_slides[0].nodes.pop().unwrap();
        spec.logical_slides[0].nodes.extend([
            figure_node(30, 130, "standalone photograph"),
            SemanticNode {
                id: id(31),
                source: range(310),
                role: SemanticRole::Diagram,
                split: SplitPolicy::Never,
                content: SemanticContent::Svg(wasmppt_deck::SvgContent {
                    resource_id: id(131),
                    source_text: Some("flowchart".to_owned()),
                }),
            },
            gallery,
        ]);
        spec.resources.extend([
            DeckResource {
                id: id(130),
                kind: ResourceKind::RasterImage,
                media_type: "image/png".to_owned(),
                bytes: vec![1],
                intrinsic_size: Some(PixelSize {
                    width: 1_600,
                    height: 900,
                }),
            },
            DeckResource {
                id: id(131),
                kind: ResourceKind::Svg,
                media_type: "image/svg+xml".to_owned(),
                bytes: br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 9"/>"#.to_vec(),
                intrinsic_size: Some(PixelSize {
                    width: 1_600,
                    height: 900,
                }),
            },
        ]);
        let plan = DeckPlanner::default()
            .plan(
                &spec,
                &template(5_500_000),
                &FontCatalog::default(),
                &limits(),
            )
            .unwrap();

        for node in [id(30), id(31)] {
            assert_eq!(
                plan.pages
                    .iter()
                    .flat_map(fragments)
                    .find(|fragment| fragment.source_node_id == node)
                    .unwrap()
                    .media
                    .unwrap()
                    .fit,
                ContentFit::Contain
            );
        }
        assert!(plan.pages.iter().flat_map(fragments).any(|fragment| {
            fragment.source_node_id == id(10)
                && fragment
                    .media
                    .is_some_and(|media| media.fit == ContentFit::Cover)
        }));
        assert!(validate_deck_plan(&spec, &template(5_500_000), &plan, &limits()).is_valid());
    }

    #[test]
    fn contained_media_uses_an_aspect_preserving_frame_inside_template_margins() {
        for (width, height) in [(4_000, 1_000), (1_000, 1_000), (1_000, 4_000)] {
            let mut spec = gallery_spec(&[(width, height)]);
            let gallery = spec.logical_slides[0].nodes.pop().unwrap();
            let SemanticContent::Children(mut children) = gallery.content else {
                unreachable!();
            };
            let figure = children.pop().unwrap();
            let figure_id = figure.id;
            spec.logical_slides[0].nodes.push(figure);
            let mut template = template(5_500_000);
            template.regions[1].margins = TextMargins {
                left: 110_000,
                top: 120_000,
                right: 130_000,
                bottom: 140_000,
            };

            let plan = DeckPlanner::default()
                .plan(&spec, &template, &FontCatalog::default(), &limits())
                .unwrap();
            let fragment = plan
                .pages
                .iter()
                .flat_map(fragments)
                .find(|fragment| fragment.source_node_id == figure_id)
                .unwrap();
            let error = (i128::from(fragment.frame.width) * i128::from(height)
                - i128::from(fragment.frame.height) * i128::from(width))
            .abs();

            assert_eq!(fragment.media.unwrap().fit, ContentFit::Contain);
            assert!(error <= i128::from(width.max(height)));
            assert!(fragment.frame.x >= template.regions[1].frame.x + 110_000);
            assert!(fragment.frame.y >= template.regions[1].frame.y + 120_000);
            assert!(validate_deck_plan(&spec, &template, &plan, &limits()).is_valid());
        }
    }

    #[test]
    fn one_image_uses_context_responsive_measured_geometry() {
        let long_copy = std::iter::repeat_n(
            "Measured copy must retain readable type while the image keeps a useful visual floor",
            18,
        )
        .collect::<Vec<_>>()
        .join(" ");
        for aspect in [(1_600, 900), (1_000, 1_000), (900, 1_600)] {
            let variants =
                [None, Some("Short explanation."), Some(long_copy.as_str())].map(|copy| {
                    let spec = single_media_spec(aspect, copy);
                    let plan = DeckPlanner::default()
                        .plan(
                            &spec,
                            &template(5_500_000),
                            &FontCatalog::default(),
                            &limits(),
                        )
                        .unwrap();
                    assert!(
                        validate_deck_plan(&spec, &template(5_500_000), &plan, &limits())
                            .is_valid()
                    );
                    let media = plan
                        .pages
                        .iter()
                        .flat_map(fragments)
                        .find(|fragment| fragment.source_node_id == id(4))
                        .and_then(|fragment| fragment.media)
                        .unwrap();
                    assert_eq!(media.fit, ContentFit::Contain);
                    assert!(
                        media.visible_frame.width.min(media.visible_frame.height)
                            >= PlannerPolicy::default().readable_media_floor
                    );
                    (plan, media)
                });

            assert_ne!(variants[0].1.slot, variants[1].1.slot);
            assert!(
                variants[1].1.slot != variants[2].1.slot || variants[2].0.pages.len() > 1,
                "long measured demand must change geometry or continue"
            );
        }
    }

    #[test]
    fn long_flow_uses_balanced_contiguous_columns() {
        let spec = spec(vec![
            text_node(3, SemanticRole::Title, SplitPolicy::Never, "Agenda"),
            list_node(4, 10),
        ]);
        let template = template(1_400_000);
        let plan = DeckPlanner::default()
            .plan(&spec, &template, &FontCatalog::default(), &limits())
            .unwrap();

        let column_page = plan
            .pages
            .iter()
            .find(|page| page.topology.kind == LayoutTopology::FlowColumns)
            .expect("long flow should select a column topology");
        let mut items = BTreeMap::<u16, u32>::new();
        for region in &column_page.regions {
            let RegionPlacement::Slot(slot) = region.placement else {
                continue;
            };
            for fragment in &region.fragments {
                if let FragmentSlice::ListItems { start, end } = fragment.slice {
                    *items.entry(slot).or_default() += end - start;
                }
            }
        }
        let loads = items.values().copied().collect::<Vec<_>>();
        assert!(loads.len() >= 2);
        assert!(loads.iter().max().unwrap() - loads.iter().min().unwrap() <= 1);
        assert!(validate_deck_plan(&spec, &template, &plan, &limits()).is_valid());
    }

    #[test]
    fn paper_flow_thresholds_balance_columns_and_continuations() {
        for (items, expected_columns) in [(11, vec![6, 5]), (16, vec![8, 8])] {
            let spec = spec(vec![
                text_node(3, SemanticRole::Title, SplitPolicy::Never, "Agenda"),
                list_node(4, items),
            ]);
            let template = template(5_500_000);
            let plan = DeckPlanner::default()
                .plan(&spec, &template, &FontCatalog::default(), &limits())
                .unwrap();

            assert_eq!(plan.pages.len(), 1);
            assert_eq!(list_items_by_slot(&plan.pages[0], id(4)), expected_columns);
            assert!(validate_deck_plan(&spec, &template, &plan, &limits()).is_valid());
        }

        for (items, expected_pages) in [(17, vec![9, 8]), (18, vec![9, 9])] {
            let spec = spec(vec![
                text_node(3, SemanticRole::Title, SplitPolicy::Never, "Agenda"),
                list_node(4, items),
            ]);
            let template = template(5_500_000);
            let plan = DeckPlanner::default()
                .plan(&spec, &template, &FontCatalog::default(), &limits())
                .unwrap();
            let per_page = plan
                .pages
                .iter()
                .map(|page| list_items_by_slot(page, id(4)).into_iter().sum())
                .collect::<Vec<u32>>();

            assert_eq!(per_page, expected_pages);
            assert!(validate_deck_plan(&spec, &template, &plan, &limits()).is_valid());
        }
    }

    #[test]
    fn readability_band_is_compared_before_page_count() {
        let comfortable = Score {
            readability_band: 0,
            pages: 2,
            ..Score::default()
        };
        let compressed = Score {
            readability_band: 6,
            pages: 1,
            ..Score::default()
        };

        assert!(comfortable < compressed);
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
        let plan = DeckPlanner::new(PlannerPolicy {
            readable_media_floor: 40_000,
            ..PlannerPolicy::default()
        })
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
        assert_eq!(table_fragments[0].repeat_table_header_rows, 0);
        assert!(
            table_fragments[1..]
                .iter()
                .any(|fragment| fragment.repeat_table_header_rows == 1)
        );
        let mut expected_start = 0;
        for page in &plan.pages {
            let fragments = fragments(page)
                .filter(|fragment| fragment.source_node_id == id(4))
                .collect::<Vec<_>>();
            assert_eq!(fragments.len(), 1, "one editable table is emitted per page");
            let FragmentSlice::TableRows { start, end } = fragments[0].slice else {
                panic!("continued table must retain a contiguous row range");
            };
            assert_eq!(start, expected_start);
            assert!(end > start);
            if start > 0 {
                assert_eq!(fragments[0].repeat_table_header_rows, 1);
            }
            expected_start = end;
        }
        assert_eq!(expected_start, 6);
        assert!(validate_deck_plan(&spec, &template, &plan, &limits()).is_valid());
    }

    #[test]
    fn trailing_empty_list_item_remains_in_the_editable_plan() {
        let mut list = list_node(4, 3);
        let SemanticContent::List(content) = &mut list.content else {
            unreachable!();
        };
        content.items[2].blocks.clear();
        let spec = spec(vec![
            text_node(3, SemanticRole::Title, SplitPolicy::Never, "Draft"),
            list,
        ]);
        let template = template(5_500_000);
        let plan = DeckPlanner::default()
            .plan(&spec, &template, &FontCatalog::default(), &limits())
            .unwrap();
        let slices = plan
            .pages
            .iter()
            .flat_map(fragments)
            .filter(|fragment| fragment.source_node_id == id(4))
            .map(|fragment| fragment.slice)
            .collect::<Vec<_>>();

        assert_eq!(slices, [FragmentSlice::ListItems { start: 0, end: 3 }]);
        assert!(validate_deck_plan(&spec, &template, &plan, &limits()).is_valid());
    }

    #[test]
    fn contiguous_table_rows_coalesce_into_one_editable_page_slice() {
        let spec = spec(vec![
            text_node(3, SemanticRole::Title, SplitPolicy::Never, "Heading"),
            table_node(4, 6, 1),
        ]);
        let template = template(5_500_000);
        let plan = DeckPlanner::default()
            .plan(&spec, &template, &FontCatalog::default(), &limits())
            .unwrap();

        let table_fragments = plan
            .pages
            .iter()
            .flat_map(fragments)
            .filter(|fragment| fragment.source_node_id == id(4))
            .collect::<Vec<_>>();
        assert_eq!(table_fragments.len(), 1);
        assert_eq!(
            table_fragments[0].slice,
            FragmentSlice::TableRows { start: 0, end: 6 }
        );
        assert!(validate_deck_plan(&spec, &template, &plan, &limits()).is_valid());
    }

    #[test]
    fn contiguous_prose_list_and_code_ranges_coalesce_per_page_lane() {
        let cases = [
            (
                text_node(
                    4,
                    SemanticRole::Prose,
                    SplitPolicy::Text,
                    "One. Two. Three. Four.",
                ),
                FragmentSlice::Text { start: 0, end: 22 },
            ),
            (
                list_node(4, 4),
                FragmentSlice::ListItems { start: 0, end: 4 },
            ),
            (
                code_node(4, "one\ntwo\nthree\nfour\n"),
                FragmentSlice::CodeLines { start: 0, end: 4 },
            ),
        ];
        for (node, expected) in cases {
            let spec = spec(vec![
                text_node(3, SemanticRole::Title, SplitPolicy::Never, "Heading"),
                node,
            ]);
            let template = template(5_500_000);
            let plan = DeckPlanner::default()
                .plan(&spec, &template, &FontCatalog::default(), &limits())
                .unwrap();
            let content = plan
                .pages
                .iter()
                .flat_map(fragments)
                .filter(|fragment| fragment.source_node_id == id(4))
                .collect::<Vec<_>>();
            assert_eq!(content.len(), 1);
            assert_eq!(content[0].slice, expected);
            assert!(validate_deck_plan(&spec, &template, &plan, &limits()).is_valid());
        }
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
        let mut crop_policy = PlannerPolicy::default();
        crop_policy.max_cover_crop_per_mille += 1;
        let crop_policy_id = DeckPlanner::new(crop_policy)
            .plan(&spec, &template, &FontCatalog::default(), &limits())
            .unwrap()
            .id;

        assert_ne!(baseline.id, source_id);
        assert_ne!(baseline.id, template_id);
        assert_ne!(baseline.id, font_id);
        assert_ne!(baseline.id, policy_id);
        assert_ne!(baseline.id, crop_policy_id);
    }

    #[test]
    fn incremental_planning_reuses_only_proven_independent_slides() {
        let first_slide = LogicalSlide {
            id: id(2),
            source: SourceRange::new("deck.md", 0, 100),
            kind: LogicalSlideKind::Content,
            hidden: false,
            nodes: vec![
                text_node(3, SemanticRole::Title, SplitPolicy::Never, "First"),
                text_node(4, SemanticRole::Prose, SplitPolicy::Text, "Unchanged."),
            ],
            media_text_relations: Vec::new(),
        };
        let second_slide = LogicalSlide {
            id: id(12),
            source: SourceRange::new("deck.md", 101, 200),
            kind: LogicalSlideKind::Content,
            hidden: false,
            nodes: vec![
                text_node(13, SemanticRole::Title, SplitPolicy::Never, "Second"),
                text_node(14, SemanticRole::Prose, SplitPolicy::Text, "Before."),
            ],
            media_text_relations: Vec::new(),
        };
        let previous_spec = DeckSpec {
            id: id(1),
            logical_slides: vec![first_slide, second_slide],
            resources: vec![],
        };
        let template = template(1_000_000);
        let planner = DeckPlanner::default();
        let previous_plan = planner
            .plan(
                &previous_spec,
                &template,
                &FontCatalog::default(),
                &limits(),
            )
            .unwrap();
        let first_page_ids = previous_plan
            .pages
            .iter()
            .filter(|page| page.logical_slide_id == id(2))
            .map(|page| page.id)
            .collect::<Vec<_>>();

        let mut next_spec = previous_spec.clone();
        next_spec.id = id(21);
        let SemanticContent::Text(text) = &mut next_spec.logical_slides[1].nodes[1].content else {
            unreachable!();
        };
        text.runs[0].text = "After.".to_owned();
        let update = planner
            .replan(
                &previous_spec,
                &previous_plan,
                &next_spec,
                &template,
                &FontCatalog::default(),
                &limits(),
            )
            .unwrap();

        assert_eq!(update.invalidated_logical_slides, vec![id(12)]);
        assert_eq!(update.reused_pages, first_page_ids.len());
        assert_eq!(
            update
                .plan
                .pages
                .iter()
                .filter(|page| page.logical_slide_id == id(2))
                .map(|page| page.id)
                .collect::<Vec<_>>(),
            first_page_ids
        );
        assert!(validate_deck_plan(&next_spec, &template, &update.plan, &limits()).is_valid());
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
            policy.limits.max_candidate_assignments = 64;
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
                media_text_relations: Vec::new(),
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

    fn figure_node(identity: u8, resource: u8, alt_text: &str) -> SemanticNode {
        SemanticNode {
            id: id(identity),
            source: range(u32::from(identity) * 10),
            role: SemanticRole::Figure,
            split: SplitPolicy::Never,
            content: SemanticContent::Image(ImageContent {
                resource_id: id(resource),
                alt_text: alt_text.to_owned(),
            }),
        }
    }

    fn gallery_spec(aspects: &[(u32, u32)]) -> DeckSpec {
        let children = aspects
            .iter()
            .enumerate()
            .map(|(index, _)| {
                let index = u8::try_from(index).unwrap();
                let mut figure = figure_node(10 + index, 100 + index, &format!("photo {index}"));
                figure.source = SourceRange::new(
                    "deck.md",
                    410 + u32::from(index) * 10,
                    419 + u32::from(index) * 10,
                );
                figure
            })
            .collect();
        let resources = aspects
            .iter()
            .enumerate()
            .map(|(index, (width, height))| DeckResource {
                id: id(100 + u8::try_from(index).unwrap()),
                kind: ResourceKind::RasterImage,
                media_type: "image/png".to_owned(),
                bytes: vec![1],
                intrinsic_size: Some(PixelSize {
                    width: *width,
                    height: *height,
                }),
            })
            .collect();
        spec_with_resources(
            vec![
                text_node(3, SemanticRole::Title, SplitPolicy::Never, "Gallery"),
                SemanticNode {
                    id: id(4),
                    source: SourceRange::new("deck.md", 400, 900),
                    role: SemanticRole::Gallery,
                    split: SplitPolicy::Children,
                    content: SemanticContent::Children(children),
                },
            ],
            resources,
        )
    }

    fn single_media_spec(aspect: (u32, u32), copy: Option<&str>) -> DeckSpec {
        let mut spec = spec_with_resources(
            vec![
                text_node(3, SemanticRole::Title, SplitPolicy::Never, "Media"),
                figure_node(4, 90, "measured media"),
            ],
            vec![DeckResource {
                id: id(90),
                kind: ResourceKind::RasterImage,
                media_type: "image/png".to_owned(),
                bytes: vec![1],
                intrinsic_size: Some(PixelSize {
                    width: aspect.0,
                    height: aspect.1,
                }),
            }],
        );
        if let Some(copy) = copy {
            spec.logical_slides[0].nodes.push(text_node(
                5,
                SemanticRole::Prose,
                SplitPolicy::Text,
                copy,
            ));
            spec.logical_slides[0]
                .media_text_relations
                .push(MediaTextRelation {
                    media_node_id: id(4),
                    text_node_id: id(5),
                    proximity: MediaTextProximity::AdjacentBlocks,
                    text_side: MediaTextSide::AfterMedia,
                    explicit_caption: false,
                });
        }
        spec
    }

    fn list_node(identity: u8, items: u8) -> SemanticNode {
        let source = range(u32::from(identity) * 10);
        SemanticNode {
            id: id(identity),
            source: source.clone(),
            role: SemanticRole::List,
            split: SplitPolicy::ListItems,
            content: SemanticContent::List(ListContent {
                ordered: false,
                start: 1,
                items: (0..items)
                    .map(|item| ListItem {
                        id: id(100 + item),
                        source: source.clone(),
                        blocks: vec![SemanticNode {
                            id: id(120 + item),
                            source: source.clone(),
                            role: SemanticRole::Prose,
                            split: SplitPolicy::Never,
                            content: SemanticContent::Text(RichText {
                                runs: vec![RichTextRun {
                                    text: format!("item {item}"),
                                    marks: TextMarks::default(),
                                    hyperlink: None,
                                }],
                            }),
                        }],
                        children: vec![],
                    })
                    .collect(),
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
                    alignment: wasmppt_deck::TableColumnAlignment::Start,
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
                capability: TemplateLayoutCapability::ContentEnvelope,
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
                    bleed_frame: None,
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
                    bleed_frame: None,
                    margins: TextMargins::default(),
                    text_levels: vec![text_level(2_000)],
                    accepts: vec![
                        SemanticRole::Section,
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

    fn title_template(details_height: Emu) -> DeckTemplatePlan {
        let mut template = template(details_height);
        template.layouts[0].capability = TemplateLayoutCapability::Title;
        template.layouts[0].matching_name = "wasmppt:title-v3".to_owned();
        template.regions[1].role = RegionRole::Subtitle;
        template.regions[1].placeholder.kind = "subTitle".to_owned();
        template.regions[1].accepts = vec![
            SemanticRole::Subtitle,
            SemanticRole::Prose,
            SemanticRole::Credit,
        ];
        template
    }

    fn template_with_statement(body_height: Emu) -> DeckTemplatePlan {
        let mut template = template(body_height);
        template.layouts.push(TemplateLayout {
            id: id(110),
            capability: TemplateLayoutCapability::Statement,
            matching_name: "statement".to_owned(),
            source_part: "ppt/slideLayouts/statement.xml".to_owned(),
            master_part: "ppt/slideMasters/slideMaster1.xml".to_owned(),
            region_ids: vec![id(111)],
            asset_ids: vec![],
            background: None,
        });
        template.regions.push(TemplateRegion {
            id: id(111),
            layout_id: id(110),
            role: RegionRole::Statement,
            placeholder: PlaceholderIdentity {
                kind: "ctrTitle".to_owned(),
                index: 1,
            },
            frame: EmuRect {
                x: 500_000,
                y: 500_000,
                width: 9_000_000,
                height: body_height.min(PAGE.height - 1_000_000),
            },
            bleed_frame: None,
            margins: TextMargins::default(),
            text_levels: vec![text_level(2_800)],
            accepts: vec![
                SemanticRole::Statement,
                SemanticRole::Quote,
                SemanticRole::DisplayMath,
            ],
            required: true,
        });
        template
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

    fn list_items_by_slot(page: &PhysicalPage, node: StableId) -> Vec<u32> {
        let mut slots = BTreeMap::<u16, u32>::new();
        for region in &page.regions {
            let RegionPlacement::Slot(slot) = region.placement else {
                continue;
            };
            for fragment in &region.fragments {
                if fragment.source_node_id != node {
                    continue;
                }
                if let FragmentSlice::ListItems { start, end } = fragment.slice {
                    *slots.entry(slot).or_default() += end - start;
                }
            }
        }
        slots.into_values().collect()
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
