use std::collections::BTreeMap;

use wasmppt_deck::{
    Emu, EmuRect, EmuSize, FragmentSlice, PixelSize, RegionRole, SemanticContent, SemanticNode,
    SemanticRole, StableId, TableColumnAlignment, TemplateRegion, inspect_media_size,
};
use wasmppt_shaper::{ShapeOptions, ShapedRun, shape};

use crate::{FontCatalog, PlannerLimits, flow::code_line};

const EMU_PER_POINT: i64 = 12_700;
const CSS_PIXEL_EMU: i64 = 9_525;
const MATH_SVG_BASE_FONT_SIZE: u32 = 1_200;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MeasureError {
    WorkLimit,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Measured {
    pub(crate) height: Emu,
    pub(crate) width: WidthDemand,
    pub(crate) font_size: u32,
    pub(crate) font_risk: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct WidthDemand {
    pub(crate) min: Emu,
    pub(crate) preferred: Emu,
    pub(crate) max: Emu,
}

pub(crate) struct Measurer<'a> {
    fonts: &'a FontCatalog,
    limits: &'a PlannerLimits,
    resources: BTreeMap<StableId, PixelSize>,
    cache: BTreeMap<MeasureKey, Measured>,
    measurements: usize,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct MeasureKey {
    node: [u8; 16],
    slice: SliceKey,
    role: RegionRole,
    region: [u8; 16],
    width: Emu,
    height: Emu,
    font_size: u32,
    repeat_table_header_rows: u32,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum SliceKey {
    Whole,
    Text(u32, u32),
    List(u32, u32),
    Table(u32, u32),
    Code(u32, u32),
}

impl<'a> Measurer<'a> {
    pub(crate) fn new(
        fonts: &'a FontCatalog,
        spec: &wasmppt_deck::DeckSpec,
        limits: &'a PlannerLimits,
    ) -> Self {
        Self {
            fonts,
            limits,
            resources: spec
                .resources
                .iter()
                .filter_map(|resource| inspect_media_size(resource).map(|size| (resource.id, size)))
                .collect(),
            cache: BTreeMap::new(),
            measurements: 0,
        }
    }

    pub(crate) fn measure(
        &mut self,
        node: &SemanticNode,
        slice: FragmentSlice,
        region: &TemplateRegion,
        frame: EmuRect,
        font_size: u32,
        repeat_table_header_rows: u32,
    ) -> Result<Measured, MeasureError> {
        let key = MeasureKey {
            node: *node.id.as_bytes(),
            slice: slice.into(),
            role: region.role,
            region: *region.id.as_bytes(),
            width: frame.width,
            height: frame.height,
            font_size,
            repeat_table_header_rows,
        };
        if let Some(measured) = self.cache.get(&key) {
            return Ok(*measured);
        }
        if self.measurements == self.limits.max_measurements {
            return Err(MeasureError::WorkLimit);
        }
        self.measurements += 1;

        let usable_width = frame
            .width
            .saturating_sub(region.margins.left)
            .saturating_sub(region.margins.right)
            .max(1);
        let default_size = region
            .text_levels
            .first()
            .and_then(|level| level.font_size)
            .unwrap_or(default_font_size(node.role));
        let font_size = if font_size == 0 {
            default_size
        } else {
            font_size
        };
        let (text, blocks, rows, aspect) = self.measure_content(node, slice);
        let requested_family = region
            .text_levels
            .first()
            .and_then(|level| level.latin_typeface.as_deref());
        let requested_font = requested_family.and_then(|family| self.fonts.font(family));
        let exact = requested_font.or_else(|| {
            self.fonts
                .default_family
                .as_deref()
                .and_then(|family| self.fonts.font(family))
        });
        let table_has_text = match &node.content {
            SemanticContent::Table(table) => table.rows.iter().any(|row| {
                row.cells
                    .iter()
                    .any(|cell| !cell.content.plain_text().is_empty())
            }),
            _ => false,
        };
        let missing_requested_font = requested_family.is_some() && requested_font.is_none();
        let font_risk =
            (missing_requested_font || exact.is_none()) && (!text.is_empty() || table_has_text);
        let line_height = font_size_to_emu(font_size).saturating_mul(6) / 5;
        let lines = if text.is_empty() {
            0
        } else if let Some(font) = exact {
            shaped_lines(
                font.bytes.as_ref(),
                font.face_index,
                &text,
                usable_width,
                font_size,
            )
            .unwrap_or_else(|| approximate_lines(&text, usable_width, font_size))
        } else {
            approximate_lines(&text, usable_width, font_size)
        };
        let text_height = line_height.saturating_mul(i64::from(lines));
        let block_height = line_height.saturating_mul(i64::from(blocks));
        let width = measure_width_demand(node, slice, exact, font_size, line_height, aspect);
        let table_height = match &node.content {
            SemanticContent::Table(table) => {
                let column_widths =
                    table_column_widths(table, slice, exact, usable_width, font_size, line_height);
                measure_table_rows(
                    table,
                    slice,
                    repeat_table_header_rows,
                    exact,
                    &column_widths,
                    font_size,
                    line_height,
                )
            }
            _ => line_height
                .saturating_mul(2)
                .saturating_mul(i64::from(rows)),
        };
        let media_height = aspect
            .map(|(width, height)| {
                if node.role == SemanticRole::DisplayMath {
                    return display_math_natural_size(PixelSize { width, height }, font_size)
                        .height
                        .min(frame.height);
                }
                let aspect_height = usable_width
                    .saturating_mul(i64::from(height))
                    .checked_div(i64::from(width).max(1))
                    .unwrap_or(frame.height);
                let usable_height = frame
                    .height
                    .saturating_sub(region.margins.top)
                    .saturating_sub(region.margins.bottom)
                    .max(1);
                let contained_width = usable_height
                    .saturating_mul(i64::from(width))
                    .checked_div(i64::from(height).max(1))
                    .unwrap_or(0);
                if aspect_height > usable_height && contained_width < line_height.saturating_mul(6)
                {
                    aspect_height
                } else {
                    aspect_height.min(usable_height)
                }
            })
            .unwrap_or(0);
        let height = text_height
            .max(block_height)
            .max(table_height)
            .max(media_height)
            .saturating_add(region.margins.top)
            .saturating_add(region.margins.bottom)
            .max(line_height);
        let measured = Measured {
            height,
            width,
            font_size,
            font_risk,
        };
        self.cache.insert(key, measured);
        Ok(measured)
    }

    pub(crate) fn intrinsic_size(&self, node: &SemanticNode) -> Option<PixelSize> {
        let resource_id = match &node.content {
            SemanticContent::Image(image) => image.resource_id,
            SemanticContent::Svg(svg) => svg.resource_id,
            _ => return None,
        };
        self.resources.get(&resource_id).copied()
    }

    fn measure_content(
        &self,
        node: &SemanticNode,
        slice: FragmentSlice,
    ) -> (String, u32, u32, Option<(u32, u32)>) {
        let mut measured = measure_content(node, slice);
        let resource_id = match &node.content {
            SemanticContent::Image(image) => Some(image.resource_id),
            SemanticContent::Svg(svg) => Some(svg.resource_id),
            _ => None,
        };
        if let Some(size) = resource_id.and_then(|id| self.resources.get(&id).copied()) {
            measured.3 = Some((size.width, size.height));
        }
        measured
    }
}

fn measure_table_rows(
    table: &wasmppt_deck::TableContent,
    slice: FragmentSlice,
    repeat_header_rows: u32,
    font: Option<&crate::FontFace>,
    column_widths: &[Emu],
    font_size: u32,
    line_height: Emu,
) -> Emu {
    let (start, end) = match slice {
        FragmentSlice::Whole => (0, table.rows.len() as u32),
        FragmentSlice::TableRows { start, end } => (start, end),
        _ => return 0,
    };
    let row_height = |row: &wasmppt_deck::TableRow| {
        let lines = row
            .cells
            .iter()
            .enumerate()
            .map(|(index, cell)| {
                let text = cell.content.plain_text();
                let column_width = column_widths.get(index).copied().unwrap_or(1).max(1);
                font.and_then(|face| {
                    shaped_lines(
                        face.bytes.as_ref(),
                        face.face_index,
                        &text,
                        column_width,
                        font_size,
                    )
                })
                .unwrap_or_else(|| approximate_lines(&text, column_width, font_size))
            })
            .max()
            .unwrap_or(1);
        line_height.saturating_mul(i64::from(lines.saturating_add(1)))
    };
    let repeated = table
        .rows
        .get(..repeat_header_rows.min(table.header_rows) as usize)
        .unwrap_or(&[])
        .iter()
        .map(&row_height)
        .fold(0, Emu::saturating_add);
    let body = table
        .rows
        .get(start as usize..end as usize)
        .unwrap_or(&[])
        .iter()
        .map(row_height)
        .fold(0, Emu::saturating_add);
    repeated.saturating_add(body)
}

fn measure_width_demand(
    node: &SemanticNode,
    slice: FragmentSlice,
    font: Option<&crate::FontFace>,
    font_size: u32,
    line_height: Emu,
    aspect: Option<(u32, u32)>,
) -> WidthDemand {
    if let SemanticContent::Table(table) = &node.content {
        return table_width_demand(table, slice, font, font_size, line_height);
    }
    if let Some((width, height)) = aspect {
        if node.role == SemanticRole::DisplayMath {
            let natural = display_math_natural_size(PixelSize { width, height }, font_size);
            return WidthDemand {
                min: natural.width,
                preferred: natural.width,
                max: natural.width,
            };
        }
        let min = line_height.saturating_mul(6);
        let preferred = min
            .saturating_mul(i64::from(width))
            .checked_div(i64::from(height).max(1))
            .unwrap_or(min)
            .max(min);
        return WidthDemand {
            min,
            preferred,
            max: preferred.saturating_mul(2),
        };
    }
    let (text, _, _, _) = measure_content(node, slice);
    text_width_demand(&text, font, font_size, line_height)
}

pub(crate) fn display_math_natural_size(size: PixelSize, font_size: u32) -> EmuSize {
    let scaled = |pixels: u32| {
        i64::from(pixels)
            .saturating_mul(CSS_PIXEL_EMU)
            .saturating_mul(i64::from(font_size))
            / i64::from(MATH_SVG_BASE_FONT_SIZE)
    };
    EmuSize {
        width: scaled(size.width).max(1),
        height: scaled(size.height).max(1),
    }
}

fn table_width_demand(
    table: &wasmppt_deck::TableContent,
    slice: FragmentSlice,
    font: Option<&crate::FontFace>,
    font_size: u32,
    line_height: Emu,
) -> WidthDemand {
    let rows = table_profile_rows(table, slice);
    let mut minimums = vec![line_height.saturating_mul(2); table.columns.len()];
    let mut preferred = minimums.clone();
    for row in rows {
        for (index, cell) in row.cells.iter().enumerate() {
            let Some(column) = table.columns.get(index) else {
                continue;
            };
            let text = cell.content.plain_text();
            let demand = text_width_demand(&text, font, font_size, line_height);
            let (min_numerator, preferred_numerator) = match column.alignment {
                TableColumnAlignment::Start => (10, 10),
                TableColumnAlignment::Center => (11, 11),
                TableColumnAlignment::End => (10, 11),
            };
            minimums[index] = minimums[index].max(demand.min.saturating_mul(min_numerator) / 10);
            preferred[index] =
                preferred[index].max(demand.preferred.saturating_mul(preferred_numerator) / 10);
        }
    }
    WidthDemand {
        min: minimums.into_iter().fold(0, Emu::saturating_add),
        preferred: preferred.iter().copied().fold(0, Emu::saturating_add),
        max: preferred
            .into_iter()
            .map(|width| width.saturating_mul(2))
            .fold(0, Emu::saturating_add),
    }
}

fn table_column_widths(
    table: &wasmppt_deck::TableContent,
    slice: FragmentSlice,
    font: Option<&crate::FontFace>,
    available: Emu,
    font_size: u32,
    line_height: Emu,
) -> Vec<Emu> {
    let rows = table_profile_rows(table, slice);
    let mut weights = vec![line_height.saturating_mul(2); table.columns.len()];
    for row in rows {
        for (index, cell) in row.cells.iter().enumerate() {
            let Some(column) = table.columns.get(index) else {
                continue;
            };
            let demand =
                text_width_demand(&cell.content.plain_text(), font, font_size, line_height);
            let alignment_weight = match column.alignment {
                TableColumnAlignment::Start => 10,
                TableColumnAlignment::Center => 11,
                TableColumnAlignment::End => 12,
            };
            weights[index] =
                weights[index].max(demand.preferred.saturating_mul(alignment_weight) / 10);
        }
    }
    proportional_widths(&weights, available)
}

fn table_rows(
    table: &wasmppt_deck::TableContent,
    slice: FragmentSlice,
) -> &[wasmppt_deck::TableRow] {
    let (start, end) = match slice {
        FragmentSlice::Whole => (0, table.rows.len()),
        FragmentSlice::TableRows { start, end } => (start as usize, end as usize),
        _ => return &[],
    };
    table.rows.get(start..end).unwrap_or(&[])
}

fn table_profile_rows(
    table: &wasmppt_deck::TableContent,
    slice: FragmentSlice,
) -> Vec<&wasmppt_deck::TableRow> {
    let header_end = usize::try_from(table.header_rows)
        .unwrap_or(usize::MAX)
        .min(table.rows.len());
    table.rows[..header_end]
        .iter()
        .chain(table_rows(table, slice))
        .collect()
}

fn proportional_widths(weights: &[Emu], available: Emu) -> Vec<Emu> {
    if weights.is_empty() {
        return Vec::new();
    }
    let total = weights.iter().copied().fold(0, Emu::saturating_add).max(1);
    let mut used = 0;
    weights
        .iter()
        .enumerate()
        .map(|(index, weight)| {
            let width = if index + 1 == weights.len() {
                available.saturating_sub(used)
            } else {
                available.saturating_mul(*weight) / total
            }
            .max(1);
            used = used.saturating_add(width);
            width
        })
        .collect()
}

fn text_width_demand(
    text: &str,
    font: Option<&crate::FontFace>,
    font_size: u32,
    line_height: Emu,
) -> WidthDemand {
    let mut min = line_height.saturating_mul(2);
    let mut preferred = min;
    for line in text.split('\n') {
        preferred = preferred.max(text_advance(line, font, font_size));
        for word in line.split_whitespace() {
            min = min.max(text_advance(word, font, font_size));
        }
    }
    WidthDemand {
        min,
        preferred: preferred.max(min),
        max: preferred.max(min).saturating_mul(2),
    }
}

fn text_advance(text: &str, font: Option<&crate::FontFace>, font_size: u32) -> Emu {
    font.and_then(|face| {
        let shaped = shape(
            face.bytes.as_ref(),
            text,
            ShapeOptions {
                face_index: face.face_index,
                ..ShapeOptions::default()
            },
        )
        .ok()?;
        let units = i64::from(shaped.units_per_em).max(1);
        Some(shaped.glyphs.iter().fold(0i64, |total, glyph| {
            total.saturating_add(
                i64::from(glyph.x_advance.abs()).saturating_mul(font_size_to_emu(font_size))
                    / units,
            )
        }))
    })
    .unwrap_or_else(|| {
        let average = font_size_to_emu(font_size).saturating_mul(11) / 20;
        average.saturating_mul(i64::try_from(text.chars().count()).unwrap_or(i64::MAX))
    })
}

impl From<FragmentSlice> for SliceKey {
    fn from(slice: FragmentSlice) -> Self {
        match slice {
            FragmentSlice::Whole => Self::Whole,
            FragmentSlice::Text { start, end } => Self::Text(start, end),
            FragmentSlice::ListItems { start, end } => Self::List(start, end),
            FragmentSlice::TableRows { start, end } => Self::Table(start, end),
            FragmentSlice::CodeLines { start, end } => Self::Code(start, end),
        }
    }
}

fn measure_content(
    node: &SemanticNode,
    slice: FragmentSlice,
) -> (String, u32, u32, Option<(u32, u32)>) {
    match (&node.content, slice) {
        (SemanticContent::Text(text), FragmentSlice::Whole) => (text.plain_text(), 0, 0, None),
        (SemanticContent::Text(text), FragmentSlice::Text { start, end }) => {
            let text = text.plain_text();
            (
                text.get(start as usize..end as usize)
                    .unwrap_or("")
                    .to_owned(),
                0,
                0,
                None,
            )
        }
        (SemanticContent::List(list), FragmentSlice::Whole) => (
            list_text(list, 0, list.items.len() as u32),
            list_block_count(list, 0, list.items.len() as u32),
            0,
            None,
        ),
        (SemanticContent::List(list), FragmentSlice::ListItems { start, end }) => (
            list_text(list, start, end),
            list_block_count(list, start, end),
            0,
            None,
        ),
        (SemanticContent::Table(table), FragmentSlice::Whole) => {
            (String::new(), 0, table.rows.len() as u32, None)
        }
        (SemanticContent::Table(_), FragmentSlice::TableRows { start, end }) => {
            (String::new(), 0, end.saturating_sub(start), None)
        }
        (SemanticContent::Code(code), FragmentSlice::Whole) => (code.code.clone(), 0, 0, None),
        (SemanticContent::Code(code), FragmentSlice::CodeLines { start, end }) => {
            let text = (start..end)
                .map(|line| code_line(&code.code, line))
                .collect::<String>();
            (text, 0, 0, None)
        }
        (SemanticContent::Image(_), _) | (SemanticContent::Svg(_), _) => {
            (String::new(), 0, 0, Some((16, 9)))
        }
        (SemanticContent::Chart(_), _) => (String::new(), 0, 0, Some((4, 3))),
        _ => (String::new(), 1, 0, None),
    }
}

fn list_text(list: &wasmppt_deck::ListContent, start: u32, end: u32) -> String {
    let mut lines = Vec::new();
    for item in list.items.get(start as usize..end as usize).unwrap_or(&[]) {
        collect_list_item_text(item, &mut lines);
    }
    lines.join("\n")
}

fn collect_list_item_text(item: &wasmppt_deck::ListItem, output: &mut Vec<String>) {
    output.extend(item.blocks.iter().filter_map(|node| match &node.content {
        SemanticContent::Text(text) => Some(text.plain_text()),
        _ => None,
    }));
    for children in &item.children {
        for child in &children.items {
            collect_list_item_text(child, output);
        }
    }
}

fn list_block_count(list: &wasmppt_deck::ListContent, start: u32, end: u32) -> u32 {
    list.items
        .get(start as usize..end as usize)
        .unwrap_or(&[])
        .iter()
        .map(list_item_block_count)
        .fold(0, u32::saturating_add)
}

fn list_item_block_count(item: &wasmppt_deck::ListItem) -> u32 {
    item.children.iter().fold(1u32, |count, children| {
        children.items.iter().fold(count, |count, child| {
            count.saturating_add(list_item_block_count(child))
        })
    })
}

fn shaped_lines(
    bytes: &[u8],
    face_index: u32,
    text: &str,
    width: Emu,
    font_size: u32,
) -> Option<u32> {
    let mut lines = 0u32;
    for line in text.split_terminator('\n') {
        if line.is_empty() {
            lines = lines.saturating_add(1);
            continue;
        }
        let shaped = shape(
            bytes,
            line,
            ShapeOptions {
                face_index,
                ..ShapeOptions::default()
            },
        )
        .ok()?;
        lines = lines.saturating_add(lines_from_shape(&shaped, width, font_size));
    }
    Some(lines.max(1))
}

fn lines_from_shape(shaped: &ShapedRun, width: Emu, font_size: u32) -> u32 {
    let scale = font_size_to_emu(font_size);
    let units = i64::from(shaped.units_per_em);
    let mut lines = 1u32;
    let mut advance = 0i64;
    for glyph in &shaped.glyphs {
        let glyph_width = i64::from(glyph.x_advance.abs()).saturating_mul(scale) / units;
        if advance > 0 && advance.saturating_add(glyph_width) > width {
            lines = lines.saturating_add(1);
            advance = 0;
        }
        advance = advance.saturating_add(glyph_width);
    }
    lines
}

fn approximate_lines(text: &str, width: Emu, font_size: u32) -> u32 {
    let average = font_size_to_emu(font_size).saturating_mul(11) / 20;
    let per_line = usize::try_from((width / average.max(1)).max(1)).unwrap_or(usize::MAX);
    text.split_terminator('\n')
        .map(|line| line.chars().count().max(1).div_ceil(per_line) as u32)
        .sum::<u32>()
        .max(1)
}

fn font_size_to_emu(font_size: u32) -> Emu {
    i64::from(font_size).saturating_mul(EMU_PER_POINT) / 100
}

fn default_font_size(role: SemanticRole) -> u32 {
    match role {
        SemanticRole::Title | SemanticRole::Statement => 3_600,
        SemanticRole::Subtitle | SemanticRole::Section => 2_800,
        SemanticRole::Code => 1_600,
        _ => 2_000,
    }
}

#[cfg(test)]
mod tests {
    use wasmppt_deck::{
        DeckResource, ListContent, ListItem, ResourceKind, RichText, RichTextRun, SourceRange,
        TableCell, TableColumn, TableContent, TableRow, TextMarks,
    };

    use super::*;

    fn id(value: u8) -> StableId {
        StableId::from_bytes([value; 16])
    }

    fn source() -> SourceRange {
        SourceRange::new("slides.md", 0, 1)
    }

    fn rich(text: &str) -> RichText {
        RichText {
            runs: vec![RichTextRun {
                text: text.to_owned(),
                marks: TextMarks::default(),
                hyperlink: None,
            }],
        }
    }

    fn resource(
        kind: ResourceKind,
        media_type: &str,
        bytes: Vec<u8>,
        hint: Option<PixelSize>,
    ) -> DeckResource {
        DeckResource {
            id: id(1),
            kind,
            media_type: media_type.to_owned(),
            bytes,
            intrinsic_size: hint,
        }
    }

    #[test]
    fn display_math_uses_css_pixels_at_the_template_type_scale() {
        let source = PixelSize {
            width: 96,
            height: 24,
        };

        assert_eq!(
            display_math_natural_size(source, 1_200),
            EmuSize {
                width: 914_400,
                height: 228_600,
            }
        );
        assert_eq!(
            display_math_natural_size(source, 2_400),
            EmuSize {
                width: 1_828_800,
                height: 457_200,
            }
        );
    }

    #[test]
    fn derives_portrait_square_and_landscape_dimensions_from_bounded_bytes() {
        let mut png = vec![0; 24];
        png[..8].copy_from_slice(b"\x89PNG\r\n\x1a\n");
        png[12..16].copy_from_slice(b"IHDR");
        png[16..20].copy_from_slice(&600u32.to_be_bytes());
        png[20..24].copy_from_slice(&900u32.to_be_bytes());
        assert_eq!(
            inspect_media_size(&resource(
                ResourceKind::RasterImage,
                "image/png",
                png,
                Some(PixelSize {
                    width: 16,
                    height: 9,
                }),
            )),
            Some(PixelSize {
                width: 600,
                height: 900,
            })
        );

        let mut gif = b"GIF89a".to_vec();
        gif.extend_from_slice(&512u16.to_le_bytes());
        gif.extend_from_slice(&512u16.to_le_bytes());
        assert_eq!(
            inspect_media_size(&resource(ResourceKind::RasterImage, "image/gif", gif, None,)),
            Some(PixelSize {
                width: 512,
                height: 512,
            })
        );

        let jpeg = vec![
            0xff, 0xd8, 0xff, 0xc0, 0x00, 0x07, 0x08, 0x01, 0x2c, 0x03, 0x20,
        ];
        assert_eq!(
            inspect_media_size(&resource(
                ResourceKind::RasterImage,
                "image/jpeg",
                jpeg,
                None,
            )),
            Some(PixelSize {
                width: 800,
                height: 300,
            })
        );

        assert_eq!(
            inspect_media_size(&resource(
                ResourceKind::Svg,
                "image/svg+xml",
                br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1600 900"/>"#.to_vec(),
                None,
            )),
            Some(PixelSize {
                width: 1600,
                height: 900,
            })
        );
    }

    #[test]
    fn table_demand_uses_cell_text_and_column_alignment() {
        let table = |alignment| TableContent {
            columns: vec![TableColumn {
                id: id(2),
                source: source(),
                alignment,
            }],
            header_rows: 0,
            rows: vec![TableRow {
                id: id(3),
                source: source(),
                cells: vec![TableCell {
                    id: id(4),
                    source: source(),
                    content: rich("a deliberately wide cell"),
                }],
            }],
        };
        let start = table_width_demand(
            &table(TableColumnAlignment::Start),
            FragmentSlice::Whole,
            None,
            2_000,
            300_000,
        );
        let end = table_width_demand(
            &table(TableColumnAlignment::End),
            FragmentSlice::Whole,
            None,
            2_000,
            300_000,
        );
        assert!(start.preferred > start.min);
        assert!(end.preferred > start.preferred);
    }

    #[test]
    fn one_list_slice_measures_its_nested_subtree_as_an_indivisible_item() {
        let nested = ListItem {
            id: id(6),
            source: source(),
            blocks: vec![SemanticNode {
                id: id(7),
                source: source(),
                role: SemanticRole::Prose,
                split: wasmppt_deck::SplitPolicy::Never,
                content: SemanticContent::Text(rich("child")),
            }],
            children: vec![],
        };
        let list = ListContent {
            ordered: false,
            start: 1,
            items: vec![ListItem {
                id: id(5),
                source: source(),
                blocks: vec![SemanticNode {
                    id: id(8),
                    source: source(),
                    role: SemanticRole::Prose,
                    split: wasmppt_deck::SplitPolicy::Never,
                    content: SemanticContent::Text(rich("parent")),
                }],
                children: vec![ListContent {
                    ordered: false,
                    start: 1,
                    items: vec![nested],
                }],
            }],
        };
        assert_eq!(list_text(&list, 0, 1), "parent\nchild");
        assert_eq!(list_block_count(&list, 0, 1), 2);
    }
}
