use std::collections::BTreeMap;

use wasmppt_deck::{
    Emu, EmuRect, FragmentSlice, RegionRole, SemanticContent, SemanticNode, SemanticRole, StableId,
    TemplateRegion,
};
use wasmppt_shaper::{ShapeOptions, ShapedRun, shape};

use crate::{FontCatalog, PlannerLimits, flow::code_line};

const EMU_PER_POINT: i64 = 12_700;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MeasureError {
    WorkLimit,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Measured {
    pub(crate) height: Emu,
    pub(crate) font_size: u32,
    pub(crate) font_risk: bool,
}

pub(crate) struct Measurer<'a> {
    fonts: &'a FontCatalog,
    limits: &'a PlannerLimits,
    resources: BTreeMap<StableId, (u32, u32)>,
    cache: BTreeMap<MeasureKey, Measured>,
    measurements: usize,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct MeasureKey {
    node: [u8; 16],
    slice: SliceKey,
    role: RegionRole,
    width: Emu,
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
                .filter_map(|resource| {
                    resource
                        .intrinsic_size
                        .map(|size| (resource.id, (size.width, size.height)))
                })
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
            width: frame.width,
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
        let table_height = match &node.content {
            SemanticContent::Table(table) => measure_table_rows(
                table,
                slice,
                repeat_table_header_rows,
                exact,
                usable_width,
                font_size,
                line_height,
            ),
            _ => line_height
                .saturating_mul(2)
                .saturating_mul(i64::from(rows)),
        };
        let media_height = aspect
            .map(|(width, height)| {
                usable_width
                    .saturating_mul(i64::from(height))
                    .checked_div(i64::from(width).max(1))
                    .unwrap_or(frame.height)
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
            font_size,
            font_risk,
        };
        self.cache.insert(key, measured);
        Ok(measured)
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
            measured.3 = Some(size);
        }
        measured
    }
}

fn measure_table_rows(
    table: &wasmppt_deck::TableContent,
    slice: FragmentSlice,
    repeat_header_rows: u32,
    font: Option<&crate::FontFace>,
    width: Emu,
    font_size: u32,
    line_height: Emu,
) -> Emu {
    let (start, end) = match slice {
        FragmentSlice::Whole => (0, table.rows.len() as u32),
        FragmentSlice::TableRows { start, end } => (start, end),
        _ => return 0,
    };
    let column_width = width / i64::try_from(table.columns.len().max(1)).unwrap_or(i64::MAX);
    let row_height = |row: &wasmppt_deck::TableRow| {
        let lines = row
            .cells
            .iter()
            .map(|cell| {
                let text = cell.content.plain_text();
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
            list.items.len() as u32,
            0,
            None,
        ),
        (SemanticContent::List(list), FragmentSlice::ListItems { start, end }) => (
            list_text(list, start, end),
            end.saturating_sub(start),
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
    list.items
        .get(start as usize..end as usize)
        .unwrap_or(&[])
        .iter()
        .flat_map(|item| item.blocks.iter())
        .filter_map(|node| match &node.content {
            SemanticContent::Text(text) => Some(text.plain_text()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
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
