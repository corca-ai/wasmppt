use std::ops::Range;

use wasmppt_opc::{
    CompressionMethod, Entry, EntryOptions, RewriteMode, VecSink, ZipArchive, ZipWriter,
};
use wasmppt_xml::{TokenKind, XmlDocument};

use super::{ChartData, GenerateError, GenerateErrorCode};
use crate::inject::patch::{Patch, apply_patches, escape_xml_text};

pub(crate) fn validate_chart_data(chart: &ChartData) -> Result<(), GenerateError> {
    if chart.categories.is_empty() || chart.series.is_empty() {
        return Err(GenerateError::new(
            GenerateErrorCode::InvalidChart,
            "chart categories and series must not be empty",
        ));
    }
    for series in &chart.series {
        if series.values.len() != chart.categories.len() {
            return Err(GenerateError::new(
                GenerateErrorCode::InvalidChart,
                format!(
                    "chart series {:?} has {} values for {} categories",
                    series.name,
                    series.values.len(),
                    chart.categories.len()
                ),
            ));
        }
        if series.values.iter().any(|value| !value.is_finite()) {
            return Err(GenerateError::new(
                GenerateErrorCode::InvalidChart,
                format!(
                    "chart series {:?} contains a non-finite number",
                    series.name
                ),
            ));
        }
    }
    Ok(())
}

pub(crate) fn rewrite_chart_cache(
    source: &[u8],
    chart: &ChartData,
) -> Result<Vec<u8>, GenerateError> {
    let document = XmlDocument::parse(source).map_err(GenerateError::xml)?;
    let series_ranges = document
        .tokens()
        .iter()
        .enumerate()
        .filter_map(|(index, token)| {
            matches!(&token.kind, TokenKind::Start { name, .. } if name.local == "ser")
                .then(|| element_token_end(&document, index).map(|end| (index, end)))
                .flatten()
        })
        .collect::<Vec<_>>();
    if series_ranges.len() != chart.series.len() {
        return Err(GenerateError::new(
            GenerateErrorCode::InvalidChart,
            format!(
                "chart has {} source series but {} replacements",
                series_ranges.len(),
                chart.series.len()
            ),
        ));
    }
    let mut patches = Vec::new();
    for (series_index, ((start, end), series)) in
        series_ranges.into_iter().zip(&chart.series).enumerate()
    {
        let column = spreadsheet_column(series_index + 2);
        replace_chart_container(
            source,
            &document,
            start,
            end,
            "tx",
            &["strCache"],
            std::slice::from_ref(&series.name),
            false,
            &format!("Sheet1!${column}$1"),
            &mut patches,
        )?;
        let numeric_categories = find_element(&document, start, end, &["xVal"]).is_some();
        let category_values = if numeric_categories {
            chart
                .categories
                .iter()
                .map(|category| {
                    category
                        .parse::<f64>()
                        .map(|value| value.to_string())
                        .map_err(|_| {
                            GenerateError::new(
                                GenerateErrorCode::InvalidChart,
                                format!("scatter chart category {category:?} is not numeric"),
                            )
                        })
                })
                .collect::<Result<Vec<_>, _>>()?
        } else {
            chart.categories.clone()
        };
        replace_chart_container(
            source,
            &document,
            start,
            end,
            if numeric_categories { "xVal" } else { "cat" },
            &["strCache", "numCache"],
            &category_values,
            numeric_categories,
            &format!("Sheet1!$A$2:$A${}", chart.categories.len() + 1),
            &mut patches,
        )?;
        let values = series
            .values
            .iter()
            .map(|value| value.to_string())
            .collect::<Vec<_>>();
        replace_chart_container(
            source,
            &document,
            start,
            end,
            if numeric_categories { "yVal" } else { "val" },
            &["numCache"],
            &values,
            true,
            &format!(
                "Sheet1!${column}$2:${column}${}",
                chart.categories.len() + 1
            ),
            &mut patches,
        )?;
    }
    apply_patches(source, patches)
}

pub(crate) fn chart_uses_numeric_categories(source: &[u8]) -> Result<bool, GenerateError> {
    let document = XmlDocument::parse(source).map_err(GenerateError::xml)?;
    Ok(document
        .tokens()
        .iter()
        .any(|token| matches!(&token.kind, TokenKind::Start { name, .. } if name.local == "xVal")))
}

#[allow(clippy::too_many_arguments)]
fn replace_chart_container(
    source: &[u8],
    document: &XmlDocument,
    series_start: usize,
    series_end: usize,
    container_name: &str,
    cache_names: &[&str],
    values: &[String],
    numeric: bool,
    formula: &str,
    patches: &mut Vec<Patch>,
) -> Result<(), GenerateError> {
    let (container_start, container_end) =
        find_element(document, series_start, series_end, &[container_name]).ok_or_else(|| {
            GenerateError::new(
                GenerateErrorCode::InvalidChart,
                format!("chart series has no {container_name} container"),
            )
        })?;
    let (cache_start, cache_end) =
        find_element(document, container_start, container_end, cache_names).ok_or_else(|| {
            GenerateError::new(
                GenerateErrorCode::InvalidChart,
                format!("chart {container_name} has no supported cache"),
            )
        })?;
    let prefix = xml_prefix(source, &document.tokens()[cache_start]);
    let mut replacement = Vec::new();
    if numeric {
        replacement.extend_from_slice(
            format!("<{prefix}:formatCode>General</{prefix}:formatCode>").as_bytes(),
        );
    }
    replacement
        .extend_from_slice(format!("<{prefix}:ptCount val=\"{}\"/>", values.len()).as_bytes());
    for (index, value) in values.iter().enumerate() {
        replacement.extend_from_slice(
            format!(
                "<{prefix}:pt idx=\"{index}\"><{prefix}:v>{}</{prefix}:v></{prefix}:pt>",
                escape_xml_text(value)
            )
            .as_bytes(),
        );
    }
    patches.push(Patch {
        range: element_inner_range(document, cache_start, cache_end)?,
        replacement,
    });
    if let Some((formula_start, formula_end)) =
        find_element(document, container_start, container_end, &["f"])
    {
        patches.push(Patch {
            range: element_inner_range(document, formula_start, formula_end)?,
            replacement: escape_xml_text(formula).into_bytes(),
        });
    }
    Ok(())
}

pub(crate) fn rewrite_embedded_workbook(
    source: &[u8],
    chart: &ChartData,
    numeric_categories: bool,
) -> Result<Vec<u8>, GenerateError> {
    let archive = ZipArchive::from_bytes(source.to_vec()).map_err(GenerateError::package)?;
    let sheet = archive.entry("xl/worksheets/sheet1.xml").ok_or_else(|| {
        GenerateError::new(
            GenerateErrorCode::InvalidChart,
            "embedded workbook has no xl/worksheets/sheet1.xml",
        )
    })?;
    let sheet_source = archive.read_entry(sheet).map_err(GenerateError::package)?;
    let document = XmlDocument::parse(sheet_source.clone()).map_err(GenerateError::xml)?;
    let (sheet_data_start, sheet_data_end) = find_element(
        &document,
        0,
        document.tokens().len().saturating_sub(1),
        &["sheetData"],
    )
    .ok_or_else(|| {
        GenerateError::new(
            GenerateErrorCode::InvalidChart,
            "embedded workbook sheet has no sheetData",
        )
    })?;
    let mut rows = String::new();
    rows.push_str("<row r=\"1\"><c r=\"A1\" t=\"inlineStr\"><is><t>Category</t></is></c>");
    for (series_index, series) in chart.series.iter().enumerate() {
        let column = spreadsheet_column(series_index + 2);
        rows.push_str(&format!(
            "<c r=\"{column}1\" t=\"inlineStr\"><is><t>{}</t></is></c>",
            escape_xml_text(&series.name)
        ));
    }
    rows.push_str("</row>");
    for (category_index, category) in chart.categories.iter().enumerate() {
        let row = category_index + 2;
        if numeric_categories {
            let category = category.parse::<f64>().map_err(|_| {
                GenerateError::new(
                    GenerateErrorCode::InvalidChart,
                    format!("scatter chart category {category:?} is not numeric"),
                )
            })?;
            rows.push_str(&format!(
                "<row r=\"{row}\"><c r=\"A{row}\"><v>{category}</v></c>"
            ));
        } else {
            rows.push_str(&format!(
                "<row r=\"{row}\"><c r=\"A{row}\" t=\"inlineStr\"><is><t>{}</t></is></c>",
                escape_xml_text(category)
            ));
        }
        for (series_index, series) in chart.series.iter().enumerate() {
            let column = spreadsheet_column(series_index + 2);
            rows.push_str(&format!(
                "<c r=\"{column}{row}\"><v>{}</v></c>",
                series.values[category_index]
            ));
        }
        rows.push_str("</row>");
    }
    let rewritten_sheet = apply_patches(
        &sheet_source,
        vec![Patch {
            range: element_inner_range(&document, sheet_data_start, sheet_data_end)?,
            replacement: rows.into_bytes(),
        }],
    )?;
    let mut writer = ZipWriter::new(VecSink::new());
    for entry in archive.entries() {
        if entry.name == "xl/worksheets/sheet1.xml" {
            writer
                .write_entry(&entry.name, &rewritten_sheet, &entry_options(entry))
                .map_err(GenerateError::package)?;
        } else {
            writer
                .raw_copy(&archive, entry, RewriteMode::Preserve)
                .map_err(GenerateError::package)?;
        }
    }
    Ok(writer
        .finish()
        .map_err(GenerateError::package)?
        .0
        .into_inner())
}

fn find_element(
    document: &XmlDocument,
    start: usize,
    end: usize,
    names: &[&str],
) -> Option<(usize, usize)> {
    (start..=end).find_map(|index| {
        let TokenKind::Start { name, .. } = &document.tokens()[index].kind else {
            return None;
        };
        names
            .contains(&name.local.as_str())
            .then(|| element_token_end(document, index).map(|element_end| (index, element_end)))
            .flatten()
    })
}

fn element_token_end(document: &XmlDocument, start: usize) -> Option<usize> {
    let TokenKind::Start { name, empty, .. } = &document.tokens()[start].kind else {
        return None;
    };
    if *empty {
        return Some(start);
    }
    document.tokens()[start + 1..]
        .iter()
        .position(|token| {
            token.depth == document.tokens()[start].depth
                && matches!(&token.kind, TokenKind::End { name: end } if end == name)
        })
        .map(|offset| start + offset + 1)
}

fn element_inner_range(
    document: &XmlDocument,
    start: usize,
    end: usize,
) -> Result<Range<usize>, GenerateError> {
    if start == end {
        return Err(GenerateError::new(
            GenerateErrorCode::InvalidChart,
            "cannot replace the contents of an empty XML element",
        ));
    }
    Ok(document.tokens()[start].range.end..document.tokens()[end].range.start)
}

fn xml_prefix(source: &[u8], token: &wasmppt_xml::Token) -> String {
    let raw = std::str::from_utf8(&source[token.range.clone()]).unwrap_or("<c:");
    raw.trim_start_matches('<')
        .split([':', ' ', '>'])
        .next()
        .filter(|prefix| !prefix.is_empty())
        .unwrap_or("c")
        .to_owned()
}

fn spreadsheet_column(mut number: usize) -> String {
    let mut output = String::new();
    while number > 0 {
        number -= 1;
        output.insert(0, (b'A' + (number % 26) as u8) as char);
        number /= 26;
    }
    output
}

fn entry_options(entry: &Entry) -> EntryOptions {
    EntryOptions {
        compression: match entry.compression {
            CompressionMethod::Stored => CompressionMethod::Stored,
            _ => CompressionMethod::Deflate,
        },
        modified_time: entry.modified_time,
        modified_date: entry.modified_date,
        local_extra: entry.local_extra.clone(),
        central_extra: entry.central_extra.clone(),
        comment: entry.comment.clone(),
        internal_attributes: entry.internal_attributes,
        external_attributes: entry.external_attributes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ChartSeriesData;

    #[test]
    fn chart_cache_rewrite_preserves_unrelated_extension_markup() {
        let source = br#"<c:chartSpace xmlns:c="c" xmlns:x="extension"><c:chart><c:plotArea><c:barChart><c:ser><c:tx><c:strRef><c:f>Old!$B$1</c:f><c:strCache><c:ptCount val="1"/><c:pt idx="0"><c:v>Old</c:v></c:pt></c:strCache></c:strRef></c:tx><c:cat><c:strRef><c:f>Old!$A$2</c:f><c:strCache><c:ptCount val="1"/><c:pt idx="0"><c:v>Old</c:v></c:pt></c:strCache></c:strRef></c:cat><c:val><c:numRef><c:f>Old!$B$2</c:f><c:numCache><c:formatCode>General</c:formatCode><c:ptCount val="1"/><c:pt idx="0"><c:v>1</c:v></c:pt></c:numCache></c:numRef></c:val><c:extLst><x:opaque value="keep-me"/></c:extLst></c:ser></c:barChart></c:plotArea></c:chart></c:chartSpace>"#;
        let chart = ChartData {
            categories: vec!["North & East".to_owned()],
            series: vec![ChartSeriesData {
                name: "Revenue <final>".to_owned(),
                values: vec![42.5],
            }],
        };

        let rewritten = String::from_utf8(rewrite_chart_cache(source, &chart).unwrap()).unwrap();

        assert!(rewritten.contains(r#"<x:opaque value="keep-me"/>"#));
        assert!(rewritten.contains("Revenue &lt;final&gt;"));
        assert!(rewritten.contains("North &amp; East"));
        assert!(rewritten.contains("Sheet1!$B$2:$B$2"));
        assert!(!rewritten.contains("Old!"));
    }
}
