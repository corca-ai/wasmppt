use wasmppt_opc::{CompressionMethod, EntryOptions, VecSink, ZipWriter};

use crate::{
    ChartData, GenerateError,
    inject::chart::{rewrite_chart_cache, rewrite_embedded_workbook, validate_chart_data},
};

/// Editable 2D chart families that the shared chart projector can create.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EditableChartKind {
    Bar,
    Column,
    Line,
    Area,
    Pie,
    Doughnut,
    Scatter,
}

/// The coordinated chart and embedded-workbook payload for a generated chart part.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EditableChartParts {
    pub chart_xml: Vec<u8>,
    pub workbook: Vec<u8>,
}

/// Build chart XML and its workbook as one validated projection.
///
/// Scatter charts use the category strings as numeric X values and reject non-numeric input.
pub fn build_editable_chart(
    kind: EditableChartKind,
    chart: &ChartData,
) -> Result<EditableChartParts, GenerateError> {
    validate_chart_data(chart)?;
    let numeric_categories = kind == EditableChartKind::Scatter;
    let chart_xml = rewrite_chart_cache(&chart_skeleton(kind, chart.series.len()), chart)?;
    let workbook = rewrite_embedded_workbook(&workbook_skeleton()?, chart, numeric_categories)?;
    Ok(EditableChartParts {
        chart_xml,
        workbook,
    })
}

fn chart_skeleton(kind: EditableChartKind, series_count: usize) -> Vec<u8> {
    let mut series = String::new();
    for index in 0..series_count {
        let category = if kind == EditableChartKind::Scatter {
            cache_reference("xVal", "numRef", "numCache")
        } else {
            cache_reference("cat", "strRef", "strCache")
        };
        let values = if kind == EditableChartKind::Scatter {
            cache_reference("yVal", "numRef", "numCache")
        } else {
            cache_reference("val", "numRef", "numCache")
        };
        series.push_str(&format!(
            "<c:ser><c:idx val=\"{index}\"/><c:order val=\"{index}\"/>{}{category}{values}</c:ser>",
            cache_reference("tx", "strRef", "strCache")
        ));
    }
    let axes = "<c:axId val=\"730001\"/><c:axId val=\"730002\"/>";
    let chart = match kind {
        EditableChartKind::Bar | EditableChartKind::Column => format!(
            "<c:barChart><c:barDir val=\"{}\"/><c:grouping val=\"clustered\"/>{series}{axes}</c:barChart>",
            if kind == EditableChartKind::Bar {
                "bar"
            } else {
                "col"
            }
        ),
        EditableChartKind::Line => {
            format!("<c:lineChart><c:grouping val=\"standard\"/>{series}{axes}</c:lineChart>")
        }
        EditableChartKind::Area => {
            format!("<c:areaChart><c:grouping val=\"standard\"/>{series}{axes}</c:areaChart>")
        }
        EditableChartKind::Pie => format!("<c:pieChart>{series}</c:pieChart>"),
        EditableChartKind::Doughnut => {
            format!("<c:doughnutChart>{series}<c:holeSize val=\"50\"/></c:doughnutChart>")
        }
        EditableChartKind::Scatter => format!(
            "<c:scatterChart><c:scatterStyle val=\"lineMarker\"/>{series}{axes}</c:scatterChart>"
        ),
    };
    let axis_xml = if matches!(kind, EditableChartKind::Pie | EditableChartKind::Doughnut) {
        String::new()
    } else if kind == EditableChartKind::Scatter {
        value_axes()
    } else {
        category_and_value_axes()
    };
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><c:chartSpace xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\" xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><c:date1904 val=\"0\"/><c:lang val=\"en-US\"/><c:chart><c:autoTitleDeleted val=\"1\"/><c:plotArea><c:layout/>{chart}{axis_xml}</c:plotArea><c:legend><c:legendPos val=\"r\"/><c:layout/></c:legend><c:plotVisOnly val=\"1\"/><c:dispBlanksAs val=\"gap\"/></c:chart><c:externalData r:id=\"rId1\"><c:autoUpdate val=\"0\"/></c:externalData></c:chartSpace>"
    )
    .into_bytes()
}

fn cache_reference(container: &str, reference: &str, cache: &str) -> String {
    format!(
        "<c:{container}><c:{reference}><c:f>Sheet1!$A$1</c:f><c:{cache}><c:ptCount val=\"1\"/><c:pt idx=\"0\"><c:v>0</c:v></c:pt></c:{cache}></c:{reference}></c:{container}>"
    )
}

fn category_and_value_axes() -> String {
    "<c:catAx><c:axId val=\"730001\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"0\"/><c:axPos val=\"b\"/><c:tickLblPos val=\"nextTo\"/><c:crossAx val=\"730002\"/><c:crosses val=\"autoZero\"/><c:auto val=\"1\"/><c:lblAlgn val=\"ctr\"/><c:lblOffset val=\"100\"/></c:catAx><c:valAx><c:axId val=\"730002\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"0\"/><c:axPos val=\"l\"/><c:numFmt formatCode=\"General\" sourceLinked=\"1\"/><c:tickLblPos val=\"nextTo\"/><c:crossAx val=\"730001\"/><c:crosses val=\"autoZero\"/><c:crossBetween val=\"between\"/></c:valAx>".to_owned()
}

fn value_axes() -> String {
    "<c:valAx><c:axId val=\"730001\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"0\"/><c:axPos val=\"b\"/><c:numFmt formatCode=\"General\" sourceLinked=\"1\"/><c:tickLblPos val=\"nextTo\"/><c:crossAx val=\"730002\"/><c:crosses val=\"autoZero\"/></c:valAx><c:valAx><c:axId val=\"730002\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"0\"/><c:axPos val=\"l\"/><c:numFmt formatCode=\"General\" sourceLinked=\"1\"/><c:tickLblPos val=\"nextTo\"/><c:crossAx val=\"730001\"/><c:crosses val=\"autoZero\"/></c:valAx>".to_owned()
}

fn workbook_skeleton() -> Result<Vec<u8>, GenerateError> {
    let options = EntryOptions::deterministic(CompressionMethod::Deflate);
    let mut writer = ZipWriter::new(VecSink::new());
    let entries: [(&str, &[u8]); 5] = [
        (
            "[Content_Types].xml",
            br#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/></Types>"#,
        ),
        (
            "_rels/.rels",
            br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#,
        ),
        (
            "xl/workbook.xml",
            br#"<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Sheet1" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
        ),
        (
            "xl/_rels/workbook.xml.rels",
            br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/></Relationships>"#,
        ),
        (
            "xl/worksheets/sheet1.xml",
            br#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData></sheetData></worksheet>"#,
        ),
    ];
    for (name, bytes) in entries {
        writer.write_entry(name, bytes, &options).map_err(|error| {
            GenerateError::new(crate::GenerateErrorCode::Package, error.to_string())
        })?;
    }
    writer
        .finish()
        .map(|(sink, _)| sink.into_inner())
        .map_err(|error| GenerateError::new(crate::GenerateErrorCode::Package, error.to_string()))
}

#[cfg(test)]
mod tests {
    use wasmppt_opc::{PackagePartSource, ZipArchive};

    use super::*;
    use crate::ChartSeriesData;

    fn data(categories: &[&str]) -> ChartData {
        ChartData {
            categories: categories.iter().map(|value| (*value).to_owned()).collect(),
            series: vec![ChartSeriesData {
                name: "Revenue".to_owned(),
                values: vec![12.5; categories.len()],
            }],
        }
    }

    #[test]
    fn projects_category_chart_cache_and_workbook_together() {
        let parts = build_editable_chart(EditableChartKind::Column, &data(&["Q1", "Q2"])).unwrap();
        let chart = String::from_utf8(parts.chart_xml).unwrap();
        assert!(chart.contains("<c:barDir val=\"col\"/>"));
        assert!(chart.contains("Sheet1!$A$2:$A$3"));
        let workbook = ZipArchive::from_bytes(parts.workbook).unwrap();
        let sheet = workbook.read_part("xl/worksheets/sheet1.xml").unwrap();
        assert!(String::from_utf8(sheet).unwrap().contains("Q2"));
    }

    #[test]
    fn every_advertised_two_dimensional_family_has_native_chart_xml() {
        for (kind, element) in [
            (EditableChartKind::Bar, "barChart"),
            (EditableChartKind::Column, "barChart"),
            (EditableChartKind::Line, "lineChart"),
            (EditableChartKind::Area, "areaChart"),
            (EditableChartKind::Pie, "pieChart"),
            (EditableChartKind::Doughnut, "doughnutChart"),
        ] {
            let parts = build_editable_chart(kind, &data(&["Q1", "Q2"])).unwrap();
            let chart = String::from_utf8(parts.chart_xml).unwrap();
            assert!(chart.contains(&format!("<c:{element}>")), "{kind:?}");
        }
        let scatter = build_editable_chart(EditableChartKind::Scatter, &data(&["1", "2"])).unwrap();
        assert!(
            String::from_utf8(scatter.chart_xml)
                .unwrap()
                .contains("<c:scatterChart>")
        );
    }

    #[test]
    fn scatter_projection_requires_numeric_x_values() {
        let error = build_editable_chart(EditableChartKind::Scatter, &data(&["Q1"])).unwrap_err();
        assert_eq!(error.code(), crate::GenerateErrorCode::InvalidChart);
        let parts = build_editable_chart(EditableChartKind::Scatter, &data(&["1.5", "2"])).unwrap();
        let chart = String::from_utf8(parts.chart_xml).unwrap();
        assert!(chart.contains("<c:xVal>") && chart.contains("<c:yVal>"));
        let workbook = ZipArchive::from_bytes(parts.workbook).unwrap();
        let sheet =
            String::from_utf8(workbook.read_part("xl/worksheets/sheet1.xml").unwrap()).unwrap();
        assert!(sheet.contains("<c r=\"A2\"><v>1.5</v></c>"));
    }

    #[test]
    fn rejects_invalid_series_before_returning_any_parts() {
        let mut mismatched = data(&["Q1", "Q2"]);
        mismatched.series[0].values.pop();
        assert_eq!(
            build_editable_chart(EditableChartKind::Line, &mismatched)
                .unwrap_err()
                .code(),
            crate::GenerateErrorCode::InvalidChart
        );

        let mut non_finite = data(&["Q1"]);
        non_finite.series[0].values[0] = f64::NAN;
        assert_eq!(
            build_editable_chart(EditableChartKind::Line, &non_finite)
                .unwrap_err()
                .code(),
            crate::GenerateErrorCode::InvalidChart
        );
    }
}
