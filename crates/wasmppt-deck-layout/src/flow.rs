use wasmppt_deck::{
    FragmentSlice, MediaTextRelation, SemanticContent, SemanticNode, SemanticRole, SplitPolicy,
};
use wasmppt_shaper::line_breaks;

#[derive(Clone, Debug)]
pub(crate) struct FlowUnit<'a> {
    pub(crate) node: &'a SemanticNode,
    pub(crate) slice: FragmentSlice,
    pub(crate) group: u32,
    pub(crate) gallery_item: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FlowError {
    UnitLimit,
}

pub(crate) fn build_flow<'a>(
    nodes: &'a [SemanticNode],
    relations: &[MediaTextRelation],
    max_units: usize,
) -> Result<Vec<FlowUnit<'a>>, FlowError> {
    let mut builder = FlowBuilder {
        units: Vec::new(),
        next_group: 1,
        max_units,
    };
    for node in nodes {
        builder.node(node, None, false)?;
    }
    link_adjacent_relations(&mut builder.units);
    link_explicit_media_captions(&mut builder.units, relations);
    Ok(builder.units)
}

fn link_explicit_media_captions(units: &mut [FlowUnit<'_>], relations: &[MediaTextRelation]) {
    for relation in relations
        .iter()
        .filter(|relation| relation.explicit_caption)
    {
        let media = units
            .iter()
            .position(|unit| unit.node.id == relation.media_node_id);
        let text = units
            .iter()
            .rposition(|unit| unit.node.id == relation.text_node_id);
        let (Some(media), Some(text)) = (media, text) else {
            continue;
        };
        let start = media.min(text);
        let end = media.max(text);
        merge_flow_groups(units, start, end + 1);
    }
}

fn link_adjacent_relations(units: &mut [FlowUnit<'_>]) {
    let mut groups = Vec::<(usize, usize)>::new();
    let mut start = 0;
    while start < units.len() {
        let node = units[start].node.id;
        let end = units[start..]
            .iter()
            .position(|unit| unit.node.id != node)
            .map_or(units.len(), |offset| start + offset);
        groups.push((start, end));
        start = end;
    }
    for pair in groups.windows(2) {
        let (left_start, left_end) = pair[0];
        let (right_start, right_end) = pair[1];
        let related = matches!(
            (units[left_start].node.role, units[right_start].node.role),
            (SemanticRole::Figure, SemanticRole::Caption)
                | (SemanticRole::Quote, SemanticRole::Credit)
        ) || units[left_start].node.role == SemanticRole::Section;
        if related {
            let end = if units[left_start].node.role == SemanticRole::Section {
                // A heading owns the first existing flow group, across leaf-node boundaries.
                // Splittable prose/list groups still contribute only their first unit.
                let first_group = units[right_start].group;
                units[right_start..]
                    .iter()
                    .position(|unit| unit.group != first_group)
                    .map_or(units.len(), |offset| right_start + offset)
            } else {
                right_end
            };
            merge_flow_groups(units, left_start, end);
            // A splittable relation is intentionally indivisible once authored as a pair.
            debug_assert!(left_end <= right_start);
        }
    }
}

// Relations union existing contiguous groups; a node boundary cannot cut a hard group.
fn merge_flow_groups(units: &mut [FlowUnit<'_>], mut start: usize, mut end: usize) {
    let first_group = units[start].group;
    let last_group = units[end - 1].group;
    while start > 0 && units[start - 1].group == first_group {
        start -= 1;
    }
    while end < units.len() && units[end].group == last_group {
        end += 1;
    }
    let group = units[start].group;
    for unit in &mut units[start..end] {
        unit.group = group;
    }
}

struct FlowBuilder<'a> {
    units: Vec<FlowUnit<'a>>,
    next_group: u32,
    max_units: usize,
}

impl<'a> FlowBuilder<'a> {
    fn node(
        &mut self,
        node: &'a SemanticNode,
        forced_group: Option<u32>,
        gallery_item: bool,
    ) -> Result<(), FlowError> {
        let SemanticContent::Children(children) = &node.content else {
            return self.leaf(node, forced_group, gallery_item);
        };
        if is_inline_math_container(node, children) {
            let group = forced_group.unwrap_or_else(|| self.group());
            for child in children {
                self.node(child, Some(group), gallery_item)?;
            }
            return Ok(());
        }
        match node.role {
            SemanticRole::Figure
                if forced_group.is_none()
                    && node.split != SplitPolicy::Never
                    && !children
                        .iter()
                        .any(|child| child.role == SemanticRole::Caption) =>
            {
                for child in children {
                    self.node(child, None, gallery_item)?;
                }
            }
            SemanticRole::Figure | SemanticRole::Definition => {
                let group = forced_group.unwrap_or_else(|| self.group());
                for child in children {
                    self.node(child, Some(group), gallery_item)?;
                }
            }
            SemanticRole::Gallery => {
                let inherited_group = forced_group
                    .or_else(|| (node.split == SplitPolicy::Never).then(|| self.group()));
                let mut index = 0;
                while index < children.len() {
                    let group = inherited_group.unwrap_or_else(|| self.group());
                    self.node(&children[index], Some(group), true)?;
                    if children[index].role == SemanticRole::Figure
                        && children
                            .get(index + 1)
                            .is_some_and(|child| child.role == SemanticRole::Caption)
                    {
                        index += 1;
                        self.node(&children[index], Some(group), true)?;
                    }
                    index += 1;
                }
            }
            SemanticRole::Quote => self.quote(children, forced_group, gallery_item)?,
            SemanticRole::Section => {
                let relation = forced_group.unwrap_or_else(|| self.group());
                for (index, child) in children.iter().enumerate() {
                    if index == 0 {
                        self.node(child, Some(relation), gallery_item)?;
                    } else {
                        let start = self.units.len();
                        self.node(child, forced_group, gallery_item)?;
                        if index == 1 {
                            if let Some(first) = self.units.get(start) {
                                let first_group = first.group;
                                for unit in self.units[start..]
                                    .iter_mut()
                                    .take_while(|unit| unit.group == first_group)
                                {
                                    unit.group = relation;
                                }
                            }
                        }
                    }
                }
            }
            _ => {
                for child in children {
                    self.node(child, forced_group, gallery_item)?;
                }
            }
        }
        Ok(())
    }

    fn quote(
        &mut self,
        children: &'a [SemanticNode],
        forced_group: Option<u32>,
        gallery_item: bool,
    ) -> Result<(), FlowError> {
        let start = self.units.len();
        for child in children {
            self.node(child, forced_group, gallery_item)?;
        }
        if forced_group.is_none() && self.units.len() > start + 1 {
            let relation = self.group();
            let last = self.units.len() - 1;
            self.units[last].group = relation;
            self.units[last - 1].group = relation;
        }
        Ok(())
    }

    fn leaf(
        &mut self,
        node: &'a SemanticNode,
        forced_group: Option<u32>,
        gallery_item: bool,
    ) -> Result<(), FlowError> {
        let slices = match (&node.content, node.split) {
            (SemanticContent::Text(text), SplitPolicy::Text) => text_ranges(&text.plain_text()),
            (SemanticContent::List(list), SplitPolicy::ListItems) => (0..list.items.len())
                .map(|index| FragmentSlice::ListItems {
                    start: index as u32,
                    end: index as u32 + 1,
                })
                .collect(),
            (SemanticContent::Table(table), SplitPolicy::TableRows) => (0..table.rows.len())
                .map(|index| FragmentSlice::TableRows {
                    start: index as u32,
                    end: index as u32 + 1,
                })
                .collect(),
            (SemanticContent::Code(code), SplitPolicy::CodeLines) => {
                (0..logical_line_count(&code.code))
                    .map(|index| FragmentSlice::CodeLines {
                        start: index,
                        end: index + 1,
                    })
                    .collect()
            }
            _ => vec![FragmentSlice::Whole],
        };
        for slice in slices {
            if self.units.len() == self.max_units {
                return Err(FlowError::UnitLimit);
            }
            let group = forced_group.unwrap_or_else(|| self.group());
            self.units.push(FlowUnit {
                node,
                slice,
                group,
                gallery_item,
            });
        }
        Ok(())
    }

    fn group(&mut self) -> u32 {
        let group = self.next_group;
        self.next_group = self.next_group.saturating_add(1);
        group
    }
}

fn is_inline_math_container(node: &SemanticNode, children: &[SemanticNode]) -> bool {
    matches!(node.role, SemanticRole::Prose | SemanticRole::Subtitle)
        && children
            .iter()
            .any(|child| matches!(child.content, SemanticContent::Svg(_)))
        && children
            .iter()
            .any(|child| matches!(child.content, SemanticContent::Text(_)))
        && children.iter().all(|child| {
            child.role == node.role
                && matches!(
                    child.content,
                    SemanticContent::Text(_) | SemanticContent::Svg(_)
                )
        })
}

fn text_ranges(text: &str) -> Vec<FragmentSlice> {
    const MAX_GROUP_BYTES: usize = 512;
    let mut preferred = Vec::new();
    let mut previous = '\0';
    for (offset, character) in text.char_indices() {
        let end = offset + character.len_utf8();
        if (matches!(previous, '.' | '?' | '!' | '。' | '？' | '！') && character.is_whitespace())
            || (previous == '\n' && character == '\n')
        {
            preferred.push(offset);
        }
        previous = character;
        if end == text.len() {
            preferred.push(end);
        }
    }
    if preferred.last().copied() != Some(text.len()) {
        preferred.push(text.len());
    }

    let uax = line_breaks(text, text.len()).unwrap_or_default();
    let mut ends = Vec::new();
    let mut start = 0usize;
    for preferred_end in preferred {
        while preferred_end.saturating_sub(start) > MAX_GROUP_BYTES {
            let limit = start + MAX_GROUP_BYTES;
            let split = uax
                .iter()
                .map(|opportunity| opportunity.offset as usize)
                .rfind(|offset| *offset > start && *offset <= limit)
                .unwrap_or_else(|| nearest_boundary(text, limit));
            if split <= start {
                break;
            }
            ends.push(split);
            start = split;
        }
        if preferred_end > start {
            ends.push(preferred_end);
            start = preferred_end;
        }
    }

    let mut start = 0u32;
    ends.into_iter()
        .filter_map(|end| {
            let end = u32::try_from(end).ok()?;
            let slice = (end > start).then_some(FragmentSlice::Text { start, end });
            start = end;
            slice
        })
        .collect()
}

fn nearest_boundary(text: &str, mut offset: usize) -> usize {
    offset = offset.min(text.len());
    while offset > 0 && !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

pub(crate) fn logical_line_count(text: &str) -> u32 {
    u32::try_from(wasmppt_deck::logical_code_lines(text).count()).unwrap_or(u32::MAX)
}

pub(crate) fn code_line(text: &str, index: u32) -> &str {
    text.split_inclusive('\n').nth(index as usize).unwrap_or("")
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasmppt_deck::{RichText, RichTextRun, SourceRange, StableId, SvgContent, TextMarks};

    fn test_node(id: u8, role: SemanticRole, content: SemanticContent) -> SemanticNode {
        SemanticNode {
            id: StableId::from_bytes([id; 16]),
            source: SourceRange::new("deck.md", u32::from(id), u32::from(id) + 1),
            role,
            split: SplitPolicy::Never,
            content,
        }
    }

    fn test_text(id: u8, role: SemanticRole, text: &str) -> SemanticNode {
        test_node(
            id,
            role,
            SemanticContent::Text(RichText {
                runs: vec![RichTextRun {
                    text: text.to_owned(),
                    marks: TextMarks::default(),
                    hyperlink: None,
                }],
            }),
        )
    }

    fn test_svg(id: u8, role: SemanticRole) -> SemanticNode {
        test_node(
            id,
            role,
            SemanticContent::Svg(SvgContent {
                resource_id: StableId::from_bytes([90; 16]),
                fallback_resource_id: None,
                source_text: Some("$E=mc^2$".to_owned()),
            }),
        )
    }

    #[test]
    fn never_figures_and_shared_captions_keep_every_panel_in_one_flow_group() {
        for split in [SplitPolicy::Never, SplitPolicy::Children] {
            let mut figure = test_node(
                1,
                SemanticRole::Figure,
                SemanticContent::Children(vec![
                    test_svg(2, SemanticRole::Figure),
                    test_svg(3, SemanticRole::Figure),
                    test_text(4, SemanticRole::Caption, "Both panels share this caption."),
                ]),
            );
            figure.split = split;
            let units = build_flow(std::slice::from_ref(&figure), &[], 100).unwrap();
            assert_eq!(units.len(), 3);
            assert!(units.iter().all(|unit| unit.group == units[0].group));
        }
        let figure = test_node(
            1,
            SemanticRole::Figure,
            SemanticContent::Children(vec![
                test_svg(2, SemanticRole::Figure),
                test_svg(3, SemanticRole::Figure),
            ]),
        );
        let units = build_flow(std::slice::from_ref(&figure), &[], 100).unwrap();
        assert_eq!(units[0].group, units[1].group);
    }

    #[test]
    fn text_ranges_cover_utf8_once_at_semantic_boundaries() {
        let text = "첫 문장. Second sentence!\n\nLast";
        let ranges = text_ranges(text);
        let mut cursor = 0;
        for slice in ranges {
            let FragmentSlice::Text { start, end } = slice else {
                panic!("unexpected slice");
            };
            assert_eq!(start, cursor);
            assert!(text.is_char_boundary(end as usize));
            cursor = end;
        }
        assert_eq!(cursor as usize, text.len());
    }

    #[test]
    fn inline_math_is_only_a_direct_text_container_contract() {
        let prose = test_node(
            1,
            SemanticRole::Prose,
            SemanticContent::Children(vec![
                test_text(2, SemanticRole::Prose, "Energy "),
                test_svg(3, SemanticRole::Prose),
                test_text(4, SemanticRole::Prose, " remains inline."),
            ]),
        );
        let section = test_node(
            5,
            SemanticRole::Section,
            SemanticContent::Children(vec![
                test_text(6, SemanticRole::Section, "Results"),
                test_text(7, SemanticRole::Prose, "Energy follows"),
                test_svg(8, SemanticRole::DisplayMath),
            ]),
        );

        let SemanticContent::Children(prose_children) = &prose.content else {
            unreachable!();
        };
        let SemanticContent::Children(section_children) = &section.content else {
            unreachable!();
        };
        assert!(is_inline_math_container(&prose, prose_children));
        assert!(!is_inline_math_container(&section, section_children));
    }
}
