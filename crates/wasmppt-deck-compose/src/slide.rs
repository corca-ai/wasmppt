use std::collections::BTreeMap;

use wasmppt_deck::{
    ChartContent, ChartKind, EmuRect, EmuSize, FragmentSlice, HyperlinkKind, ListContent,
    PhysicalPage, PlannedFragment, RegionRole, RichText, RichTextRun, SemanticContent,
    SemanticNode, StableId, TableColumnAlignment, TableContent, TemplateLayout, TemplateRegion,
    TemplateTextLevel, TemplateTheme,
};
use wasmppt_template::{ChartData, ChartSeriesData, EditableChartKind, build_editable_chart};

use crate::{ComposeError, ComposeErrorCode, media::PreparedMedia, xml_attr, xml_text};

const OFFICE_REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

pub(crate) struct ComposedSlide {
    pub(crate) xml: Vec<u8>,
    pub(crate) relationships: Vec<u8>,
    pub(crate) parts: Vec<ComposedPart>,
}

pub(crate) struct ComposedPart {
    pub(crate) name: String,
    pub(crate) content_type: Option<&'static str>,
    pub(crate) bytes: Vec<u8>,
}

struct SlideWriter<'a> {
    xml: String,
    relationships: String,
    next_shape_id: u32,
    next_relationship_id: u32,
    parts: Vec<ComposedPart>,
    nodes: &'a BTreeMap<StableId, &'a SemanticNode>,
    media: &'a BTreeMap<StableId, PreparedMedia>,
    theme: &'a TemplateTheme,
}

pub(crate) fn compose_slide(
    page: &PhysicalPage,
    layout: &TemplateLayout,
    page_size: EmuSize,
    theme: &TemplateTheme,
    regions: &BTreeMap<StableId, &TemplateRegion>,
    nodes: &BTreeMap<StableId, &SemanticNode>,
    media: &BTreeMap<StableId, PreparedMedia>,
) -> Result<ComposedSlide, ComposeError> {
    let visibility = if page.hidden { " show=\"0\"" } else { "" };
    let mut writer = SlideWriter {
        xml: format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><p:sld xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:r=\"{OFFICE_REL}\" xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\"{visibility}><p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"0\" cy=\"0\"/><a:chOff x=\"0\" y=\"0\"/><a:chExt cx=\"0\" cy=\"0\"/></a:xfrm></p:grpSpPr>"
        ),
        relationships: format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"{OFFICE_REL}/slideLayout\" Target=\"../{}\"/>",
            xml_attr(
                layout
                    .source_part
                    .strip_prefix("ppt/")
                    .unwrap_or(&layout.source_part)
            )
        ),
        next_shape_id: 2,
        next_relationship_id: 2,
        parts: Vec::new(),
        nodes,
        media,
        theme,
    };

    if page.continuation.ordinal > 1 {
        if let Some(node_id) = page.continuation.repeated_heading_node_id {
            if let Some(node) = nodes.get(&node_id) {
                if let SemanticContent::Text(text) = &node.content {
                    let continuation_region = layout.region_ids.iter().find_map(|id| {
                        regions
                            .get(id)
                            .copied()
                            .filter(|region| region.role == RegionRole::Title)
                    });
                    let frame = continuation_region.map_or_else(
                        || EmuRect {
                            x: page_size.width / 20,
                            y: page_size.height / 25,
                            width: page_size.width / 10 * 9,
                            height: page_size.height / 8,
                        },
                        |region| region.frame,
                    );
                    writer.text_shape(
                        frame,
                        "Continuation heading",
                        &[Paragraph::rich(text.runs.clone(), 0)],
                        continuation_region,
                        Some(2_400),
                    )?;
                }
            }
        }
    }

    for planned_region in &page.regions {
        let region = regions
            .get(&planned_region.template_region_id)
            .copied()
            .ok_or_else(|| {
                ComposeError::new(
                    ComposeErrorCode::InvalidContract,
                    format!(
                        "planned region references missing template region {}",
                        planned_region.template_region_id
                    ),
                )
            })?;
        for fragment in &planned_region.fragments {
            writer.fragment(fragment, region)?;
        }
    }
    writer
        .xml
        .push_str("</p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sld>");
    writer.relationships.push_str("</Relationships>");
    Ok(ComposedSlide {
        xml: writer.xml.into_bytes(),
        relationships: writer.relationships.into_bytes(),
        parts: writer.parts,
    })
}

impl SlideWriter<'_> {
    fn fragment(
        &mut self,
        fragment: &PlannedFragment,
        region: &TemplateRegion,
    ) -> Result<(), ComposeError> {
        let node = self
            .nodes
            .get(&fragment.source_node_id)
            .copied()
            .ok_or_else(|| {
                ComposeError::new(
                    ComposeErrorCode::InvalidContract,
                    format!(
                        "fragment references missing node {}",
                        fragment.source_node_id
                    ),
                )
            })?;
        match &node.content {
            SemanticContent::Text(text) => {
                let runs = slice_rich_text(text, fragment.slice)?;
                self.text_shape(
                    fragment.frame,
                    role_name(node),
                    &[Paragraph::rich(runs, 0)],
                    Some(region),
                    Some(fragment.type_choice.font_size),
                )
            }
            SemanticContent::Code(code) => {
                let lines = slice_code(&code.code, fragment.slice)?;
                let paragraphs = lines
                    .into_iter()
                    .map(|line| Paragraph {
                        runs: vec![RichTextRun {
                            text: line,
                            marks: wasmppt_deck::TextMarks {
                                inline_code: true,
                                ..Default::default()
                            },
                            hyperlink: None,
                        }],
                        level: 0,
                        bullet: Bullet::None,
                        alignment: None,
                    })
                    .collect::<Vec<_>>();
                self.text_shape(
                    fragment.frame,
                    role_name(node),
                    &paragraphs,
                    Some(region),
                    Some(fragment.type_choice.font_size),
                )
            }
            SemanticContent::List(list) => {
                let paragraphs = list_paragraphs(list, fragment.slice)?;
                self.text_shape(
                    fragment.frame,
                    role_name(node),
                    &paragraphs,
                    Some(region),
                    Some(fragment.type_choice.font_size),
                )
            }
            SemanticContent::Image(image) => {
                let media = self.media.get(&image.resource_id).ok_or_else(|| {
                    ComposeError::new(
                        ComposeErrorCode::InvalidContract,
                        "prepared image is missing",
                    )
                })?;
                let placement = fragment.media.ok_or_else(|| {
                    ComposeError::new(
                        ComposeErrorCode::InvalidContract,
                        "planned image placement is missing",
                    )
                })?;
                self.picture(&image.alt_text, media, placement, false)
            }
            SemanticContent::Svg(svg) => {
                let media = self.media.get(&svg.resource_id).ok_or_else(|| {
                    ComposeError::new(ComposeErrorCode::InvalidContract, "prepared SVG is missing")
                })?;
                let placement = fragment.media.ok_or_else(|| {
                    ComposeError::new(
                        ComposeErrorCode::InvalidContract,
                        "planned SVG placement is missing",
                    )
                })?;
                self.picture(
                    svg.source_text.as_deref().unwrap_or("Vector graphic"),
                    media,
                    placement,
                    true,
                )
            }
            SemanticContent::Children(_) => Err(ComposeError::new(
                ComposeErrorCode::InvalidContract,
                "container nodes do not own composed shapes",
            )),
            SemanticContent::Table(table) => self.table(fragment, table, region),
            SemanticContent::Chart(chart) => self.chart(fragment, chart),
        }
    }

    fn table(
        &mut self,
        fragment: &PlannedFragment,
        table: &TableContent,
        region: &TemplateRegion,
    ) -> Result<(), ComposeError> {
        let (start, end) = match fragment.slice {
            FragmentSlice::Whole => (0, table.rows.len()),
            FragmentSlice::TableRows { start, end } => (start as usize, end as usize),
            _ => {
                return Err(ComposeError::new(
                    ComposeErrorCode::InvalidContract,
                    "table fragment has a non-table slice",
                ));
            }
        };
        let selected = table
            .rows
            .get(start..end)
            .filter(|rows| !rows.is_empty())
            .ok_or_else(|| {
                ComposeError::new(
                    ComposeErrorCode::InvalidContract,
                    "table slice is empty or outside its source",
                )
            })?;
        let repeated = usize::try_from(fragment.repeat_table_header_rows).map_err(|_| {
            ComposeError::new(ComposeErrorCode::WorkLimit, "table header count overflow")
        })?;
        let header_count = usize::try_from(table.header_rows)
            .map_err(|_| {
                ComposeError::new(ComposeErrorCode::WorkLimit, "table header count overflow")
            })?
            .min(table.rows.len());
        if repeated > header_count || (start == 0 && repeated != 0) {
            return Err(ComposeError::new(
                ComposeErrorCode::InvalidContract,
                "planned repeated table headers do not match the source table",
            ));
        }
        let rows = table.rows[..repeated]
            .iter()
            .chain(selected.iter())
            .collect::<Vec<_>>();
        let column_count = table.columns.len();
        if column_count == 0 || rows.iter().any(|row| row.cells.len() != column_count) {
            return Err(ComposeError::new(
                ComposeErrorCode::InvalidContract,
                "table rows must match the declared non-empty column set",
            ));
        }

        let shape_id = self.take_shape_id()?;
        let frame = fragment.frame;
        self.xml.push_str(&format!(
            "<p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id=\"{shape_id}\" name=\"Table {shape_id}\"/><p:cNvGraphicFramePr/><p:nvPr/></p:nvGraphicFramePr><p:xfrm><a:off x=\"{}\" y=\"{}\"/><a:ext cx=\"{}\" cy=\"{}\"/></p:xfrm><a:graphic><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/table\"><a:tbl><a:tblPr firstRow=\"{}\" bandRow=\"1\"/><a:tblGrid>",
            frame.x,
            frame.y,
            frame.width,
            frame.height,
            u8::from(header_count > 0)
        ));
        let column_widths = table_column_lengths(table, &rows, frame.width)?;
        for width in &column_widths {
            self.xml.push_str(&format!("<a:gridCol w=\"{width}\"/>"));
        }
        self.xml.push_str("</a:tblGrid>");
        let row_heights = table_row_lengths(&rows, &column_widths, frame.height)?;
        let style = region.text_levels.first();
        for (rendered_index, (row, height)) in rows.iter().zip(row_heights).enumerate() {
            let source_index = if rendered_index < repeated {
                rendered_index
            } else {
                start + rendered_index - repeated
            };
            let header = source_index < header_count;
            self.xml.push_str(&format!("<a:tr h=\"{height}\">"));
            for (column_index, cell) in row.cells.iter().enumerate() {
                self.xml.push_str("<a:tc><a:txBody><a:bodyPr/><a:lstStyle>");
                self.xml.push_str("</a:lstStyle>");
                let paragraph = Paragraph::rich(cell.content.runs.clone(), 0)
                    .aligned(table.columns[column_index].alignment);
                self.paragraph(&paragraph, style, Some(fragment.type_choice.font_size))?;
                self.xml.push_str("</a:txBody><a:tcPr marL=\"91440\" marR=\"91440\" marT=\"45720\" marB=\"45720\">");
                let fill = if header {
                    theme_rgb(self.theme, "accent1", 0x0044_72c4)
                } else if rendered_index % 2 == 1 {
                    theme_rgb(self.theme, "lt2", 0x00e7_e6e6)
                } else {
                    theme_rgb(self.theme, "lt1", 0x00ff_ffff)
                };
                let border = theme_rgb(self.theme, "dk1", 0x007f_7f7f);
                // CT_TableCellProperties requires border lines before its fill choice.
                for side in ["L", "R", "T", "B"] {
                    self.xml.push_str(&format!("<a:ln{side} w=\"9525\"><a:solidFill><a:srgbClr val=\"{border:06X}\"/></a:solidFill><a:prstDash val=\"solid\"/></a:ln{side}>"));
                }
                self.xml.push_str(&format!(
                    "<a:solidFill><a:srgbClr val=\"{fill:06X}\"/></a:solidFill>"
                ));
                self.xml.push_str("</a:tcPr></a:tc>");
            }
            self.xml.push_str("</a:tr>");
        }
        self.xml
            .push_str("</a:tbl></a:graphicData></a:graphic></p:graphicFrame>");
        Ok(())
    }

    fn chart(
        &mut self,
        fragment: &PlannedFragment,
        chart: &ChartContent,
    ) -> Result<(), ComposeError> {
        if fragment.slice != FragmentSlice::Whole {
            return Err(ComposeError::new(
                ComposeErrorCode::InvalidContract,
                "charts are atomic and require a whole fragment",
            ));
        }
        let data = ChartData {
            categories: chart.categories.clone(),
            series: chart
                .series
                .iter()
                .map(|series| ChartSeriesData {
                    name: series.name.clone(),
                    values: series.values.clone(),
                })
                .collect(),
        };
        let parts = build_editable_chart(chart_kind(chart.kind), &data).map_err(|error| {
            ComposeError::new(ComposeErrorCode::InvalidContract, error.to_string())
        })?;
        let token = crate::stable_id_hex(fragment.id);
        let chart_part = format!("ppt/charts/deck-{token}.xml");
        let workbook_part = format!("ppt/embeddings/deck-{token}.xlsx");
        let chart_relationship_part = format!("ppt/charts/_rels/deck-{token}.xml.rels");
        let relationship_id = self.take_relationship_id()?;
        self.relationships.push_str(&format!(
            "<Relationship Id=\"{}\" Type=\"{OFFICE_REL}/chart\" Target=\"../charts/deck-{token}.xml\"/>",
            xml_attr(&relationship_id)
        ));
        self.parts.push(ComposedPart {
            name: chart_part,
            content_type: Some("application/vnd.openxmlformats-officedocument.drawingml.chart+xml"),
            bytes: parts.chart_xml,
        });
        self.parts.push(ComposedPart {
            name: workbook_part,
            content_type: Some("application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"),
            bytes: parts.workbook,
        });
        self.parts.push(ComposedPart {
            name: chart_relationship_part,
            content_type: None,
            bytes: format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"{OFFICE_REL}/package\" Target=\"../embeddings/deck-{token}.xlsx\"/></Relationships>"
            )
            .into_bytes(),
        });
        let shape_id = self.take_shape_id()?;
        let frame = fragment.frame;
        self.xml.push_str(&format!(
            "<p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id=\"{shape_id}\" name=\"Chart {shape_id}\"/><p:cNvGraphicFramePr/><p:nvPr/></p:nvGraphicFramePr><p:xfrm><a:off x=\"{}\" y=\"{}\"/><a:ext cx=\"{}\" cy=\"{}\"/></p:xfrm><a:graphic><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/chart\"><c:chart xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\" r:id=\"{}\"/></a:graphicData></a:graphic></p:graphicFrame>",
            frame.x,
            frame.y,
            frame.width,
            frame.height,
            xml_attr(&relationship_id)
        ));
        Ok(())
    }

    fn text_shape(
        &mut self,
        frame: EmuRect,
        name: &str,
        paragraphs: &[Paragraph],
        region: Option<&TemplateRegion>,
        requested_font_size: Option<u32>,
    ) -> Result<(), ComposeError> {
        let shape_id = self.take_shape_id()?;
        let style = region.and_then(|region| region.text_levels.first());
        self.xml.push_str(&format!(
            "<p:sp><p:nvSpPr><p:cNvPr id=\"{shape_id}\" name=\"{}\"/><p:cNvSpPr txBox=\"1\"/><p:nvPr/></p:nvSpPr><p:spPr><a:xfrm><a:off x=\"{}\" y=\"{}\"/><a:ext cx=\"{}\" cy=\"{}\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom><a:noFill/><a:ln><a:noFill/></a:ln></p:spPr><p:txBody><a:bodyPr{} wrap=\"square\"/><a:lstStyle>",
            xml_attr(name), frame.x, frame.y, frame.width, frame.height,
            margins(region)
        ));
        self.xml.push_str("</a:lstStyle>");
        for paragraph in paragraphs {
            self.paragraph(paragraph, style, requested_font_size)?;
        }
        self.xml.push_str("</p:txBody></p:sp>");
        Ok(())
    }

    fn paragraph(
        &mut self,
        paragraph: &Paragraph,
        style: Option<&TemplateTextLevel>,
        requested: Option<u32>,
    ) -> Result<(), ComposeError> {
        let level = paragraph.level.min(8);
        let margin = style
            .and_then(|level| level.margin_left)
            .unwrap_or(342_900 + i64::from(level) * 342_900);
        let indent = style.and_then(|level| level.indent).unwrap_or(-285_750);
        let alignment = paragraph.alignment.map_or("", |alignment| match alignment {
            TableColumnAlignment::Start => " algn=\"l\"",
            TableColumnAlignment::Center => " algn=\"ctr\"",
            TableColumnAlignment::End => " algn=\"r\"",
        });
        self.xml.push_str(&format!(
            "<a:p><a:pPr lvl=\"{level}\" marL=\"{margin}\" indent=\"{indent}\"{alignment}>"
        ));
        match paragraph.bullet {
            Bullet::None => self.xml.push_str("<a:buNone/>"),
            Bullet::Unordered => self.xml.push_str("<a:buChar char=\"•\"/>"),
            Bullet::Ordered(start) => self.xml.push_str(&format!(
                "<a:buAutoNum type=\"arabicPeriod\" startAt=\"{}\"/>",
                start.max(1)
            )),
        }
        self.xml.push_str("</a:pPr>");
        for run in &paragraph.runs {
            let hyperlink = run
                .hyperlink
                .as_ref()
                .map(|link| self.hyperlink(link))
                .transpose()?
                .flatten();
            self.xml.push_str("<a:r><a:rPr lang=\"en-US\"");
            let font_size = requested
                .filter(|size| *size > 0)
                .or_else(|| style.and_then(|style| style.font_size))
                .unwrap_or(1_800);
            self.xml.push_str(&format!(" sz=\"{font_size}\""));
            if run.marks.bold || style.and_then(|style| style.bold).unwrap_or(false) {
                self.xml.push_str(" b=\"1\"");
            }
            if run.marks.italic || style.and_then(|style| style.italic).unwrap_or(false) {
                self.xml.push_str(" i=\"1\"");
            }
            if run.marks.strikethrough {
                self.xml.push_str(" strike=\"sngStrike\"");
            }
            self.xml.push('>');
            if let Some(color) = style.and_then(|style| style.color.as_ref()) {
                if let Some(scheme) = &color.scheme {
                    self.xml.push_str(&format!(
                        "<a:solidFill><a:schemeClr val=\"{}\"/></a:solidFill>",
                        xml_attr(scheme)
                    ));
                } else {
                    self.xml.push_str(&format!(
                        "<a:solidFill><a:srgbClr val=\"{:06X}\"/></a:solidFill>",
                        color.rgb & 0x00ff_ffff
                    ));
                }
            }
            let typeface = if run.marks.inline_code {
                Some("Courier New")
            } else {
                style.and_then(|style| style.latin_typeface.as_deref())
            };
            if let Some(typeface) = typeface {
                self.xml
                    .push_str(&format!("<a:latin typeface=\"{}\"/>", xml_attr(typeface)));
            }
            if let Some(id) = hyperlink {
                self.xml
                    .push_str(&format!("<a:hlinkClick r:id=\"{}\"/>", xml_attr(&id)));
            }
            self.xml.push_str("</a:rPr><a:t>");
            self.xml.push_str(&xml_text(&run.text));
            self.xml.push_str("</a:t></a:r>");
        }
        self.xml.push_str("<a:endParaRPr/></a:p>");
        Ok(())
    }

    fn hyperlink(
        &mut self,
        link: &wasmppt_deck::SafeHyperlink,
    ) -> Result<Option<String>, ComposeError> {
        let target = match link.kind {
            HyperlinkKind::Web => link.target.clone(),
            HyperlinkKind::Email | HyperlinkKind::Telephone => link.target.clone(),
            HyperlinkKind::SourceAnchor => return Ok(None),
        };
        let id = self.take_relationship_id()?;
        self.relationships.push_str(&format!(
            "<Relationship Id=\"{}\" Type=\"{OFFICE_REL}/hyperlink\" Target=\"{}\" TargetMode=\"External\"/>",
            xml_attr(&id), xml_attr(&target)
        ));
        Ok(Some(id))
    }

    fn picture(
        &mut self,
        alt: &str,
        media: &PreparedMedia,
        placement: wasmppt_deck::MediaPlacement,
        svg: bool,
    ) -> Result<(), ComposeError> {
        if media.size != Some(placement.source_size) || !placement.is_canonical() {
            return Err(ComposeError::new(
                ComposeErrorCode::InvalidContract,
                "prepared media dimensions differ from the resolved plan",
            ));
        }
        let shape_id = self.take_shape_id()?;
        let relationship_id = self.take_relationship_id()?;
        let target = media
            .part_name
            .strip_prefix("ppt/")
            .unwrap_or(&media.part_name);
        self.relationships.push_str(&format!(
            "<Relationship Id=\"{}\" Type=\"{OFFICE_REL}/image\" Target=\"../{}\"/>",
            xml_attr(&relationship_id),
            xml_attr(target)
        ));
        let crop = crop_xml(placement.crop);
        let visible_frame = placement.visible_frame;
        let svg_extension = if svg {
            format!(
                "<a:extLst><a:ext uri=\"{{96DAC541-7B7A-43D3-8B79-37D633B846F1}}\"><asvg:svgBlip xmlns:asvg=\"http://schemas.microsoft.com/office/drawing/2016/SVG/main\" r:embed=\"{}\"/></a:ext></a:extLst>",
                xml_attr(&relationship_id)
            )
        } else {
            String::new()
        };
        self.xml.push_str(&format!(
            "<p:pic><p:nvPicPr><p:cNvPr id=\"{shape_id}\" name=\"Media {shape_id}\" descr=\"{}\"/><p:cNvPicPr><a:picLocks noChangeAspect=\"1\"/></p:cNvPicPr><p:nvPr/></p:nvPicPr><p:blipFill><a:blip r:embed=\"{}\">{svg_extension}</a:blip>{crop}<a:stretch><a:fillRect/></a:stretch></p:blipFill><p:spPr><a:xfrm><a:off x=\"{}\" y=\"{}\"/><a:ext cx=\"{}\" cy=\"{}\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></p:spPr></p:pic>",
            xml_attr(alt),
            xml_attr(&relationship_id),
            visible_frame.x,
            visible_frame.y,
            visible_frame.width,
            visible_frame.height
        ));
        Ok(())
    }

    fn take_shape_id(&mut self) -> Result<u32, ComposeError> {
        let id = self.next_shape_id;
        self.next_shape_id = self.next_shape_id.checked_add(1).ok_or_else(|| {
            ComposeError::new(ComposeErrorCode::WorkLimit, "shape identifier overflow")
        })?;
        Ok(id)
    }

    fn take_relationship_id(&mut self) -> Result<String, ComposeError> {
        let id = self.next_relationship_id;
        self.next_relationship_id = self.next_relationship_id.checked_add(1).ok_or_else(|| {
            ComposeError::new(
                ComposeErrorCode::WorkLimit,
                "relationship identifier overflow",
            )
        })?;
        Ok(format!("rId{id}"))
    }
}

#[derive(Clone)]
struct Paragraph {
    runs: Vec<RichTextRun>,
    level: u8,
    bullet: Bullet,
    alignment: Option<TableColumnAlignment>,
}

impl Paragraph {
    fn rich(runs: Vec<RichTextRun>, level: u8) -> Self {
        Self {
            runs,
            level,
            bullet: Bullet::None,
            alignment: None,
        }
    }
    fn aligned(mut self, alignment: TableColumnAlignment) -> Self {
        self.alignment = Some(alignment);
        self
    }
}

#[derive(Clone, Copy)]
enum Bullet {
    None,
    Unordered,
    Ordered(u32),
}

fn slice_rich_text(
    text: &RichText,
    slice: FragmentSlice,
) -> Result<Vec<RichTextRun>, ComposeError> {
    let (start, end) = match slice {
        FragmentSlice::Whole => (0, text.plain_text().len()),
        FragmentSlice::Text { start, end } => (start as usize, end as usize),
        _ => {
            return Err(ComposeError::new(
                ComposeErrorCode::InvalidContract,
                "text fragment has a non-text slice",
            ));
        }
    };
    let mut output = Vec::new();
    let mut offset = 0usize;
    for run in &text.runs {
        let run_end = offset.checked_add(run.text.len()).ok_or_else(|| {
            ComposeError::new(ComposeErrorCode::WorkLimit, "rich-text byte count overflow")
        })?;
        let overlap_start = start.max(offset);
        let overlap_end = end.min(run_end);
        if overlap_start < overlap_end {
            let local_start = overlap_start - offset;
            let local_end = overlap_end - offset;
            let selected = run.text.get(local_start..local_end).ok_or_else(|| {
                ComposeError::new(
                    ComposeErrorCode::InvalidContract,
                    "text slice is not on UTF-8 boundaries",
                )
            })?;
            let mut selected_run = run.clone();
            selected_run.text = selected.to_owned();
            output.push(selected_run);
        }
        offset = run_end;
    }
    if start > end || end > offset || output.is_empty() {
        return Err(ComposeError::new(
            ComposeErrorCode::InvalidContract,
            "text slice is empty or outside its source",
        ));
    }
    Ok(output)
}

fn slice_code(code: &str, slice: FragmentSlice) -> Result<Vec<String>, ComposeError> {
    let lines = code.split('\n').map(ToOwned::to_owned).collect::<Vec<_>>();
    let (start, end) = match slice {
        FragmentSlice::Whole => (0, lines.len()),
        FragmentSlice::CodeLines { start, end } => (start as usize, end as usize),
        _ => {
            return Err(ComposeError::new(
                ComposeErrorCode::InvalidContract,
                "code fragment has a non-code slice",
            ));
        }
    };
    lines
        .get(start..end)
        .map(<[String]>::to_vec)
        .filter(|lines| !lines.is_empty())
        .ok_or_else(|| {
            ComposeError::new(
                ComposeErrorCode::InvalidContract,
                "code slice is empty or outside its source",
            )
        })
}

fn list_paragraphs(
    list: &ListContent,
    slice: FragmentSlice,
) -> Result<Vec<Paragraph>, ComposeError> {
    let (start, end) = match slice {
        FragmentSlice::Whole => (0, list.items.len()),
        FragmentSlice::ListItems { start, end } => (start as usize, end as usize),
        _ => {
            return Err(ComposeError::new(
                ComposeErrorCode::InvalidContract,
                "list fragment has a non-list slice",
            ));
        }
    };
    let items = list
        .items
        .get(start..end)
        .filter(|items| !items.is_empty())
        .ok_or_else(|| {
            ComposeError::new(
                ComposeErrorCode::InvalidContract,
                "list slice is empty or outside its source",
            )
        })?;
    let mut output = Vec::new();
    append_list(
        items,
        list.ordered,
        list.start.saturating_add(start as u32),
        0,
        &mut output,
    );
    Ok(output)
}

fn append_list(
    items: &[wasmppt_deck::ListItem],
    ordered: bool,
    start: u32,
    level: u8,
    output: &mut Vec<Paragraph>,
) {
    for (index, item) in items.iter().enumerate() {
        let mut first = true;
        if item.blocks.is_empty() {
            output.push(Paragraph {
                runs: vec![],
                level,
                bullet: if ordered {
                    Bullet::Ordered(start.saturating_add(index as u32))
                } else {
                    Bullet::Unordered
                },
                alignment: None,
            });
        }
        for block in &item.blocks {
            if let SemanticContent::Text(text) = &block.content {
                output.push(Paragraph {
                    runs: text.runs.clone(),
                    level,
                    bullet: if first {
                        if ordered {
                            Bullet::Ordered(start.saturating_add(index as u32))
                        } else {
                            Bullet::Unordered
                        }
                    } else {
                        Bullet::None
                    },
                    alignment: None,
                });
                first = false;
            }
        }
        for child in &item.children {
            append_list(
                &child.items,
                child.ordered,
                child.start,
                level.saturating_add(1),
                output,
            );
        }
    }
}

fn crop_xml(crop: Option<wasmppt_deck::SourceCrop>) -> String {
    crop.map_or_else(
        || "<a:srcRect/>".to_owned(),
        |crop| {
            format!(
                "<a:srcRect l=\"{}\" t=\"{}\" r=\"{}\" b=\"{}\"/>",
                crop.left, crop.top, crop.right, crop.bottom
            )
        },
    )
}

fn margins(region: Option<&TemplateRegion>) -> String {
    region
        .map(|region| {
            format!(
                " lIns=\"{}\" tIns=\"{}\" rIns=\"{}\" bIns=\"{}\"",
                region.margins.left,
                region.margins.top,
                region.margins.right,
                region.margins.bottom
            )
        })
        .unwrap_or_default()
}

fn role_name(node: &SemanticNode) -> &'static str {
    match node.role {
        wasmppt_deck::SemanticRole::Title => "Title",
        wasmppt_deck::SemanticRole::Subtitle => "Subtitle",
        wasmppt_deck::SemanticRole::List => "List",
        wasmppt_deck::SemanticRole::Figure => "Figure",
        wasmppt_deck::SemanticRole::Code => "Code",
        wasmppt_deck::SemanticRole::Diagram | wasmppt_deck::SemanticRole::DisplayMath => {
            "Vector graphic"
        }
        _ => "Content",
    }
}

fn table_column_lengths(
    table: &TableContent,
    rows: &[&wasmppt_deck::TableRow],
    total: i64,
) -> Result<Vec<i64>, ComposeError> {
    let mut weights = vec![4u64; table.columns.len()];
    for row in rows {
        for (index, cell) in row.cells.iter().enumerate() {
            let text = cell.content.plain_text();
            let longest_word = text
                .split_whitespace()
                .map(|word| word.chars().count())
                .max()
                .unwrap_or(1);
            let preferred = u64::try_from(text.chars().count().clamp(longest_word, 48).max(4))
                .unwrap_or(u64::MAX);
            let alignment_weight = match table.columns[index].alignment {
                TableColumnAlignment::Start => 10,
                TableColumnAlignment::Center => 11,
                TableColumnAlignment::End => 12,
            };
            weights[index] = weights[index].max(preferred.saturating_mul(alignment_weight) / 10);
        }
    }
    // A wide table keeps its leading key column comfortably identifiable while all columns remain
    // part of the same native table rather than becoming horizontally duplicated fragments.
    if weights.len() >= 6 {
        let other_count = u64::try_from(weights.len() - 1).unwrap_or(u64::MAX);
        let other_average = weights.iter().skip(1).sum::<u64>() / other_count;
        weights[0] = weights[0].max(other_average.saturating_mul(3) / 2);
    }
    weighted_lengths(total, &weights)
}

fn table_row_lengths(
    rows: &[&wasmppt_deck::TableRow],
    column_widths: &[i64],
    total: i64,
) -> Result<Vec<i64>, ComposeError> {
    let weights = rows
        .iter()
        .map(|row| {
            row.cells
                .iter()
                .zip(column_widths)
                .map(|(cell, width)| {
                    let capacity = u64::try_from(*width / 110_000).unwrap_or(1).max(1);
                    let characters =
                        u64::try_from(cell.content.plain_text().chars().count().max(1))
                            .unwrap_or(u64::MAX);
                    characters.div_ceil(capacity).max(1)
                })
                .max()
                .unwrap_or(1)
        })
        .collect::<Vec<_>>();
    weighted_lengths(total, &weights)
}

fn weighted_lengths(total: i64, weights: &[u64]) -> Result<Vec<i64>, ComposeError> {
    if weights.is_empty() || total <= 0 || weights.contains(&0) {
        return Err(ComposeError::new(
            ComposeErrorCode::InvalidContract,
            "table geometry must have positive dimensions and members",
        ));
    }
    let count_i64 = i64::try_from(weights.len())
        .map_err(|_| ComposeError::new(ComposeErrorCode::WorkLimit, "table size overflow"))?;
    if total < count_i64 {
        return Err(ComposeError::new(
            ComposeErrorCode::InvalidContract,
            "table frame is too small for its rows or columns",
        ));
    }
    let total_weight = weights
        .iter()
        .try_fold(0u64, |sum, weight| sum.checked_add(*weight))
        .filter(|sum| *sum > 0)
        .ok_or_else(|| ComposeError::new(ComposeErrorCode::WorkLimit, "table weight overflow"))?;
    let distributable = total.saturating_sub(count_i64);
    let mut output = Vec::with_capacity(weights.len());
    let mut assigned = 0i64;
    for (index, weight) in weights.iter().enumerate() {
        let length = if index + 1 == weights.len() {
            total.saturating_sub(assigned)
        } else {
            let weighted = u128::try_from(distributable)
                .unwrap_or_default()
                .saturating_mul(u128::from(*weight))
                / u128::from(total_weight);
            i64::try_from(weighted)
                .unwrap_or(i64::MAX)
                .saturating_add(1)
        };
        if length <= 0 {
            return Err(ComposeError::new(
                ComposeErrorCode::InvalidContract,
                "table weighted geometry collapsed a member",
            ));
        }
        assigned = assigned.saturating_add(length);
        output.push(length);
    }
    Ok(output)
}

fn theme_rgb(theme: &TemplateTheme, slot: &str, fallback: u32) -> u32 {
    theme
        .colors
        .iter()
        .find(|color| color.slot == slot)
        .map_or(fallback, |color| color.rgb & 0x00ff_ffff)
}

const fn chart_kind(kind: ChartKind) -> EditableChartKind {
    match kind {
        ChartKind::Bar => EditableChartKind::Bar,
        ChartKind::Column => EditableChartKind::Column,
        ChartKind::Line => EditableChartKind::Line,
        ChartKind::Area => EditableChartKind::Area,
        ChartKind::Pie => EditableChartKind::Pie,
        ChartKind::Doughnut => EditableChartKind::Doughnut,
        ChartKind::Scatter => EditableChartKind::Scatter,
    }
}
