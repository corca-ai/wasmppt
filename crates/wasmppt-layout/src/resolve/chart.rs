use wasmppt_xml::{Attribute, TokenKind, XmlDocument, decode_entities};

use crate::{ChartGrouping, ChartKind, ChartSeries, ResolvedChart, RgbaColor};

pub(super) fn parse_chart(document: &XmlDocument) -> ResolvedChart {
    let mut kinds = Vec::new();
    let mut chart_ranges = Vec::new();
    for index in 0..document.tokens().len() {
        let Some(kind) = chart_kind_at(document, index) else {
            continue;
        };
        chart_ranges.push((
            index,
            element_end(document, index).unwrap_or(index),
            document.tokens()[index].depth,
            kind,
        ));
        if !kinds.contains(&kind) {
            kinds.push(kind);
        }
    }
    let kind = match kinds.as_slice() {
        [] => ChartKind::Other,
        [kind] => *kind,
        _ => ChartKind::Combination,
    };
    let palette = [
        RgbaColor {
            red: 68,
            green: 114,
            blue: 196,
            alpha: 255,
        },
        RgbaColor {
            red: 237,
            green: 125,
            blue: 49,
            alpha: 255,
        },
        RgbaColor {
            red: 165,
            green: 165,
            blue: 165,
            alpha: 255,
        },
        RgbaColor {
            red: 255,
            green: 192,
            blue: 0,
            alpha: 255,
        },
        RgbaColor {
            red: 91,
            green: 155,
            blue: 213,
            alpha: 255,
        },
    ];
    let grouping = document
        .tokens()
        .iter()
        .find_map(|token| {
            let TokenKind::Start {
                name, attributes, ..
            } = &token.kind
            else {
                return None;
            };
            (name.local == "grouping").then(|| match plain(attributes, "val") {
                Some("stacked") => ChartGrouping::Stacked,
                Some("percentStacked") => ChartGrouping::PercentStacked,
                _ => ChartGrouping::Standard,
            })
        })
        .unwrap_or_default();
    let title = document
        .tokens()
        .iter()
        .enumerate()
        .find_map(|(index, token)| {
            let TokenKind::Start { name, .. } = &token.kind else {
                return None;
            };
            if name.local != "title" {
                return None;
            }
            let end = element_end(document, index)?;
            let text = collect_text(document, index, end);
            (!text.is_empty()).then_some(text)
        });
    let show_legend = document.tokens().iter().any(
        |token| matches!(&token.kind, TokenKind::Start { name, .. } if name.local == "legend"),
    );
    let mut series = Vec::new();
    for (index, token) in document.tokens().iter().enumerate() {
        let TokenKind::Start { name, .. } = &token.kind else {
            continue;
        };
        if name.local != "ser" {
            continue;
        }
        let Some(series_kind) = chart_ranges
            .iter()
            .find(|(start, end, depth, _)| {
                *start < index && index <= *end && *depth + 1 == token.depth
            })
            .map(|(_, _, _, kind)| *kind)
        else {
            continue;
        };
        let Some(end) = element_end(document, index) else {
            continue;
        };
        let name = child_cache_values(document, index, end, &["tx"])
            .into_iter()
            .next()
            .unwrap_or_else(|| format!("Series {}", series.len() + 1));
        let categories = child_cache_values(document, index, end, &["cat"]);
        let x_values = child_cache_values(document, index, end, &["xVal"])
            .into_iter()
            .filter_map(|value| value.parse::<f64>().ok())
            .collect();
        let values = child_cache_values(document, index, end, &["val", "yVal"])
            .into_iter()
            .filter_map(|value| value.parse::<f64>().ok())
            .collect();
        let bubble_sizes = child_cache_values(document, index, end, &["bubbleSize"])
            .into_iter()
            .filter_map(|value| value.parse::<f64>().ok())
            .collect();
        series.push(ChartSeries {
            kind: series_kind,
            name,
            categories,
            x_values,
            values,
            bubble_sizes,
            color: palette[series.len() % palette.len()],
        });
    }
    ResolvedChart {
        kind,
        grouping,
        series,
        title,
        show_legend,
        embedded_workbook: None,
    }
}

fn chart_kind_at(document: &XmlDocument, index: usize) -> Option<ChartKind> {
    let token = document.tokens().get(index)?;
    let TokenKind::Start { name, .. } = &token.kind else {
        return None;
    };
    Some(match name.local.as_str() {
        "lineChart" => ChartKind::Line,
        "pieChart" => ChartKind::Pie,
        "doughnutChart" => ChartKind::Doughnut,
        "areaChart" => ChartKind::Area,
        "scatterChart" => ChartKind::Scatter,
        "bubbleChart" => ChartKind::Bubble,
        "barChart" => {
            let end = element_end(document, index).unwrap_or(index);
            let bar_direction = document.tokens()[index..=end].iter().find_map(|candidate| {
                let TokenKind::Start {
                    name, attributes, ..
                } = &candidate.kind
                else {
                    return None;
                };
                (candidate.depth == token.depth + 1 && name.local == "barDir")
                    .then(|| plain(attributes, "val"))
                    .flatten()
            });
            if bar_direction == Some("bar") {
                ChartKind::Bar
            } else {
                ChartKind::Column
            }
        }
        _ => return None,
    })
}

fn child_cache_values(
    document: &XmlDocument,
    start: usize,
    end: usize,
    container_names: &[&str],
) -> Vec<String> {
    for index in start..=end {
        let TokenKind::Start { name, .. } = &document.tokens()[index].kind else {
            continue;
        };
        if !container_names.contains(&name.local.as_str()) {
            continue;
        }
        let container_end = element_end(document, index).unwrap_or(index);
        let values = cache_values(document, index, container_end);
        if !values.is_empty() {
            return values;
        }
    }
    Vec::new()
}

fn cache_values(document: &XmlDocument, start: usize, end: usize) -> Vec<String> {
    let mut indexed = Vec::<(u32, String)>::new();
    for index in start..=end {
        let TokenKind::Start {
            name, attributes, ..
        } = &document.tokens()[index].kind
        else {
            continue;
        };
        if name.local != "pt" {
            continue;
        }
        let point_end = element_end(document, index).unwrap_or(index);
        let value = (index..=point_end).find_map(|value_index| {
            let TokenKind::Start { name, .. } = &document.tokens()[value_index].kind else {
                return None;
            };
            (name.local == "v").then(|| {
                element_end(document, value_index)
                    .map(|value_end| collect_raw_text(document, value_index, value_end))
                    .unwrap_or_default()
            })
        });
        if let Some(value) = value {
            indexed.push((plain_u32(attributes, "idx").unwrap_or(index as u32), value));
        }
    }
    if indexed.is_empty() {
        let direct = collect_raw_text(document, start, end);
        return (!direct.is_empty()).then_some(direct).into_iter().collect();
    }
    indexed.sort_unstable_by_key(|(index, _)| *index);
    indexed.into_iter().map(|(_, value)| value).collect()
}

fn collect_text(document: &XmlDocument, start: usize, end: usize) -> String {
    let mut output = String::new();
    for index in start..=end {
        let TokenKind::Start { name, .. } = &document.tokens()[index].kind else {
            continue;
        };
        if name.local == "t" {
            let text_end = element_end(document, index).unwrap_or(index);
            output.push_str(&collect_raw_text(document, index, text_end));
        }
    }
    output
}

fn collect_raw_text(document: &XmlDocument, start: usize, end: usize) -> String {
    let mut output = String::new();
    for token in &document.tokens()[start..=end] {
        if !matches!(token.kind, TokenKind::Text | TokenKind::Cdata) {
            continue;
        }
        let range = if matches!(token.kind, TokenKind::Cdata) {
            token.range.start + 9..token.range.end - 3
        } else {
            token.range.clone()
        };
        let raw = std::str::from_utf8(document.source_range(range)).unwrap_or_default();
        if matches!(token.kind, TokenKind::Cdata) {
            output.push_str(raw);
        } else if let Ok(decoded) = decode_entities(raw, token.range.start) {
            output.push_str(&decoded);
        }
    }
    output
}

fn element_end(document: &XmlDocument, start: usize) -> Option<usize> {
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

fn plain<'a>(attributes: &'a [Attribute], local: &str) -> Option<&'a str> {
    attributes
        .iter()
        .find(|attribute| attribute.name.local == local)
        .map(|attribute| attribute.value.as_str())
}

fn plain_u32(attributes: &[Attribute], local: &str) -> Option<u32> {
    plain(attributes, local)?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_every_supported_two_dimensional_chart_family() {
        let cases = [
            ("barChart", "<c:barDir val=\"col\"/>", ChartKind::Column),
            ("barChart", "<c:barDir val=\"bar\"/>", ChartKind::Bar),
            ("lineChart", "", ChartKind::Line),
            ("pieChart", "", ChartKind::Pie),
            ("doughnutChart", "", ChartKind::Doughnut),
            ("areaChart", "", ChartKind::Area),
            ("scatterChart", "", ChartKind::Scatter),
            ("bubbleChart", "", ChartKind::Bubble),
        ];
        for (element, properties, expected) in cases {
            let source = format!(
                r#"<c:chartSpace xmlns:c="c" xmlns:a="a"><c:chart><c:title><a:p><a:r><a:t>Revenue</a:t></a:r></a:p></c:title><c:legend/><c:plotArea><c:{element}>{properties}<c:grouping val="stacked"/><c:ser><c:tx><c:v>Actual</c:v></c:tx><c:xVal><c:numRef><c:numCache><c:pt idx="0"><c:v>1</c:v></c:pt></c:numCache></c:numRef></c:xVal><c:yVal><c:numRef><c:numCache><c:pt idx="0"><c:v>2</c:v></c:pt></c:numCache></c:numRef></c:yVal><c:bubbleSize><c:numRef><c:numCache><c:pt idx="0"><c:v>3</c:v></c:pt></c:numCache></c:numRef></c:bubbleSize></c:ser></c:{element}></c:plotArea></c:chart></c:chartSpace>"#,
            );
            let document = XmlDocument::parse(source.into_bytes()).unwrap();
            let chart = parse_chart(&document);
            assert_eq!(chart.kind, expected);
            assert_eq!(chart.grouping, ChartGrouping::Stacked);
            assert_eq!(chart.title.as_deref(), Some("Revenue"));
            assert!(chart.show_legend);
            assert_eq!(chart.series[0].x_values, [1.0]);
            assert_eq!(chart.series[0].values, [2.0]);
            assert_eq!(chart.series[0].bubble_sizes, [3.0]);
        }
    }

    #[test]
    fn leaves_three_dimensional_charts_explicitly_unsupported() {
        let document = XmlDocument::parse(
            br#"<c:chartSpace xmlns:c="c"><c:chart><c:plotArea><c:pie3DChart/></c:plotArea></c:chart></c:chartSpace>"#
                .as_slice(),
        )
        .unwrap();
        assert_eq!(parse_chart(&document).kind, ChartKind::Other);
    }

    #[test]
    fn recognizes_two_dimensional_combination_charts() {
        let document = XmlDocument::parse(
            br#"<c:chartSpace xmlns:c="c"><c:chart><c:plotArea><c:lineChart><c:ser><c:val><c:numLit><c:pt idx="0"><c:v>2</c:v></c:pt></c:numLit></c:val></c:ser></c:lineChart><c:barChart><c:barDir val="col"/><c:ser><c:val><c:numLit><c:pt idx="0"><c:v>10</c:v></c:pt></c:numLit></c:val></c:ser></c:barChart></c:plotArea></c:chart></c:chartSpace>"#.as_slice(),
        )
        .unwrap();
        let chart = parse_chart(&document);
        assert_eq!(chart.kind, ChartKind::Combination);
        assert_eq!(chart.series[0].kind, ChartKind::Line);
        assert_eq!(chart.series[1].kind, ChartKind::Column);
    }
}
