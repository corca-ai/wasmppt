//! Bounded contain-only fitting for an indivisible multi-panel figure and its caption.

use super::*;

impl DeckPlanner {
    /// A hard relation may contain several source images before its shared caption. Give each
    /// image a real allocation rather than letting the first consume the remaining lane.
    pub(super) fn fit_multi_image_group<'a>(
        &self,
        group: &FlowGroup<'a>,
        lane_frame: EmuRect,
        initial_size: u32,
        target: &FitTarget<'_>,
        measurer: &mut Measurer<'_>,
    ) -> Result<Option<FittedGroup<'a>>, PlanError> {
        if let Some(cards) = self.fit_caption_cards(group, lane_frame, target, measurer)? {
            return Ok(Some(cards));
        }
        let media_count = group
            .units
            .iter()
            .take_while(|unit| {
                unit.node.role != SemanticRole::DisplayMath
                    && matches!(
                        unit.node.content,
                        SemanticContent::Image(_) | SemanticContent::Svg(_)
                    )
            })
            .count();
        if !(2..=6).contains(&media_count)
            || group.units[media_count..].iter().any(|unit| {
                !matches!(
                    unit.node.content,
                    SemanticContent::Text(_)
                        | SemanticContent::List(_)
                        | SemanticContent::Code(_)
                        | SemanticContent::Table(_)
                )
            })
        {
            return Ok(None);
        }
        let sizes = group.units[..media_count]
            .iter()
            .map(|unit| measurer.intrinsic_size(unit.node))
            .collect::<Option<Vec<_>>>();
        let Some(sizes) = sizes else {
            return Ok(None);
        };
        let remaining = EmuRect {
            y: target.y,
            height: lane_frame
                .y
                .saturating_add(lane_frame.height)
                .saturating_sub(target.y),
            ..lane_frame
        };
        if !remaining.is_positive() {
            return Ok(None);
        }
        let floor = target
            .region
            .minimum_font_size
            .max(self.policy.readable_floor);
        let can_shrink_text = group.units[media_count..].iter().any(|unit| {
            measurer.font_size_for_role(unit.node, target.region, initial_size)
                > measurer.floor_for_role(unit.node, target.region, self.policy.readable_floor)
        });
        let mut body_size = initial_size;
        loop {
            let mut tails = Vec::new();
            let mut reserved = 0i64;
            let mut tables_on_page = target.tables_on_page.clone();
            let mut text_fits = true;
            for unit in &group.units[media_count..] {
                let repeat = repeated_table_header_rows(unit.node, unit.slice, &tables_on_page);
                if matches!(unit.node.content, SemanticContent::Table(_)) {
                    tables_on_page.insert(unit.node.id);
                }
                let size = measurer.font_size_for_role(unit.node, target.region, body_size);
                let measured = measurer
                    .measure(
                        unit.node,
                        unit.slice,
                        target.region,
                        remaining,
                        size,
                        repeat,
                    )
                    .map_err(|failure| measure_error(failure, unit.node.id))?;
                let usable_width = remaining
                    .width
                    .saturating_sub(target.region.margins.left)
                    .saturating_sub(target.region.margins.right);
                if measured.width.min > usable_width {
                    text_fits = false;
                    break;
                }
                reserved = reserved
                    .saturating_add(measured.height)
                    .saturating_add(self.policy.gap);
                tails.push((unit, measured, repeat));
            }
            let media_frame = EmuRect {
                height: remaining.height.saturating_sub(reserved),
                ..remaining
            };
            if text_fits && media_frame.is_positive() {
                let equal = vec![1; media_count];
                // Canonical aspects justify unequal tracks; source order and equal visual
                // weight are retained, without assigning an arbitrary lead/hero image.
                let aspect = sizes
                    .iter()
                    .map(|size| {
                        (i64::from(size.width).saturating_mul(1_000)
                            / i64::from(size.height).max(1))
                        .clamp(1, 1_000_000)
                    })
                    .collect::<Vec<_>>();
                let inverse_aspect = sizes
                    .iter()
                    .map(|size| {
                        (i64::from(size.height).saturating_mul(1_000)
                            / i64::from(size.width).max(1))
                        .clamp(1, 1_000_000)
                    })
                    .collect::<Vec<_>>();
                let variants = [
                    columns(media_frame, self.policy.gap, &equal),
                    columns(media_frame, self.policy.gap, &aspect),
                    rows(media_frame, self.policy.gap, &equal),
                    rows(media_frame, self.policy.gap, &inverse_aspect),
                ];
                let mut best = None::<(u64, Vec<MediaPlacement>)>;
                for frames in variants {
                    let mut media = Vec::with_capacity(media_count);
                    let mut whitespace = 0u64;
                    for (frame, size) in frames.into_iter().zip(&sizes) {
                        if !frame.is_within(media_frame) {
                            break;
                        }
                        let slot = inset_frame(frame, target.region.margins);
                        let Some(placement) = MediaPlacement::contain(slot, *size) else {
                            break;
                        };
                        if !placement.visible_frame.is_within(frame)
                            || placement
                                .visible_frame
                                .width
                                .min(placement.visible_frame.height)
                                < self.policy.readable_media_floor
                        {
                            break;
                        }
                        whitespace =
                            whitespace.saturating_add(contain_whitespace_penalty(placement));
                        media.push(placement);
                    }
                    if media.len() == media_count
                        && best.as_ref().is_none_or(|(score, _)| whitespace < *score)
                    {
                        best = Some((whitespace, media));
                    }
                }
                if let Some((_, media)) = best {
                    let mut placements = Vec::with_capacity(group.units.len());
                    for (unit, media) in group.units[..media_count].iter().zip(media) {
                        placements.push(FittedPlacement {
                            node: unit.node,
                            slice: unit.slice,
                            frame: media.visible_frame,
                            font_size: 0,
                            reduction: 0,
                            width_penalty: contain_whitespace_penalty(media),
                            font_risk: false,
                            media: Some(media),
                            repeat_table_header_rows: 0,
                        });
                    }
                    let mut cursor = media_frame.y.saturating_add(media_frame.height);
                    for (unit, measured, repeat) in tails {
                        cursor = cursor.saturating_add(self.policy.gap);
                        let frame = EmuRect {
                            y: cursor,
                            height: measured.height,
                            ..remaining
                        };
                        if !frame.is_within(remaining) {
                            return Ok(None);
                        }
                        placements.push(FittedPlacement {
                            node: unit.node,
                            slice: unit.slice,
                            frame,
                            font_size: measured.font_size,
                            reduction: measurer
                                .font_size_for_role(unit.node, target.region, initial_size)
                                .saturating_sub(measured.font_size),
                            width_penalty: width_penalty(
                                measured.width.preferred.min(measured.width.max),
                                remaining
                                    .width
                                    .saturating_sub(target.region.margins.left)
                                    .saturating_sub(target.region.margins.right),
                            ),
                            font_risk: measured.font_risk,
                            media: None,
                            repeat_table_header_rows: repeat,
                        });
                        cursor = cursor.saturating_add(measured.height);
                    }
                    return Ok(Some(FittedGroup {
                        placements,
                        bottom: cursor,
                    }));
                }
            }
            if !can_shrink_text || body_size <= floor {
                return Ok(None);
            }
            body_size = body_size
                .saturating_sub(self.policy.font_step.max(1))
                .max(floor);
        }
    }
    fn fit_caption_cards<'a>(
        &self,
        group: &FlowGroup<'a>,
        lane_frame: EmuRect,
        target: &FitTarget<'_>,
        measurer: &mut Measurer<'_>,
    ) -> Result<Option<FittedGroup<'a>>, PlanError> {
        if !(4..=12).contains(&group.units.len()) || group.units.len() % 2 != 0 {
            return Ok(None);
        }
        let mut cards = Vec::new();
        for pair in group.units.chunks_exact(2) {
            if !matches!(
                pair[0].node.content,
                SemanticContent::Image(_) | SemanticContent::Svg(_)
            ) || pair[0].node.role == SemanticRole::DisplayMath
                || pair[1].node.role != SemanticRole::Caption
            {
                return Ok(None);
            }
            cards.push(FlowGroup {
                units: pair.to_vec(),
            });
        }
        let remaining = EmuRect {
            y: target.y,
            height: lane_frame
                .y
                .saturating_add(lane_frame.height)
                .saturating_sub(target.y),
            ..lane_frame
        };
        if !remaining.is_positive() {
            return Ok(None);
        }
        let mut best = None::<(u64, FittedGroup<'a>)>;
        for frames in gallery_frame_variants(remaining, self.policy.gap, cards.len()) {
            let mut placements = Vec::new();
            let mut bottom = target.y;
            for (index, card) in cards.iter().enumerate() {
                let Some(fitted) = self.fit_group(
                    card,
                    &FitTarget {
                        frames: &frames,
                        lane: index,
                        y: frames[index].y,
                        center_single_statement: false,
                        ..*target
                    },
                    measurer,
                )?
                else {
                    break;
                };
                bottom = bottom.max(fitted.bottom);
                placements.extend(fitted.placements);
            }
            if placements.len() != group.units.len() {
                continue;
            }
            let loss = placements
                .iter()
                .map(|placement| placement.width_penalty)
                .sum::<u64>();
            if best.as_ref().is_none_or(|(score, _)| loss < *score) {
                best = Some((loss, FittedGroup { placements, bottom }));
            }
        }
        Ok(best.map(|(_, fitted)| fitted))
    }
}
