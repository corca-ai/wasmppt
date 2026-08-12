use std::{collections::BTreeMap, env, hint::black_box, sync::Arc, time::Instant};

use wasmppt_display::DisplayList;
use wasmppt_layout::PresentationDocument;
use wasmppt_opc::ZipArchive;
use wasmppt_template::{
    BindingKind, ChartData, ChartSeriesData, ImageData, InjectionData, PreparedTemplate,
    TemplateCompiler,
};

const DOGFOOD: &[u8] = include_bytes!("../../../fixtures/dogfood/report.potx");
const ADVANCED: &[u8] = include_bytes!("../../../fixtures/render/basic.pptx");

fn main() {
    let iterations: usize = env::args()
        .nth(1)
        .expect("usage: benchmark_live_operations ITERATIONS")
        .parse()
        .unwrap();
    assert!(iterations >= 3);
    let dogfood = prepare(DOGFOOD);
    let advanced = prepare(ADVANCED);
    let table = measure(&dogfood, iterations, dogfood_data, |iteration| {
        let mut delta = InjectionData::new();
        delta.set_table_rows("metrics", rows(&format!("table {iteration}")));
        delta
    });
    let chart = measure(&advanced, iterations, advanced_data, |iteration| {
        InjectionData::new()
            .with_chart("ppt/charts/chart1.xml", chart_data(iteration as f64 + 20.0))
    });
    let topology = measure(&dogfood, iterations, dogfood_data, |_| {
        let mut delta = InjectionData::new();
        delta.set_slide_copies("ppt/slides/slide2.xml", 2);
        delta
    });
    println!(
        "{{\"schema\":1,\"iterations\":{iterations},\"operations\":{{\"table\":{},\"chart\":{},\"slideTopology\":{}}}}}",
        table.json(),
        chart.json(),
        topology.json(),
    );
}

struct Measurement {
    apply: Vec<u64>,
    render_ready: Vec<u64>,
    maximum_invalidated_slides: usize,
    minimum_reused_parts: u64,
    maximum_resident_bytes: u64,
}

impl Measurement {
    fn json(&self) -> String {
        format!(
            "{{\"applyDeltaNs\":[{}],\"inputToRenderReadyNs\":[{}],\"maximumInvalidatedSlides\":{},\"minimumReusedMaterializedParts\":{},\"maximumResidentBytes\":{}}}",
            array(&self.apply),
            array(&self.render_ready),
            self.maximum_invalidated_slides,
            self.minimum_reused_parts,
            self.maximum_resident_bytes,
        )
    }
}

fn measure(
    prepared: &Arc<PreparedTemplate>,
    iterations: usize,
    initial: fn(&PreparedTemplate) -> InjectionData,
    delta: impl Fn(usize) -> InjectionData,
) -> Measurement {
    let mut measurement = Measurement {
        apply: Vec::with_capacity(iterations),
        render_ready: Vec::with_capacity(iterations),
        maximum_invalidated_slides: 0,
        minimum_reused_parts: u64::MAX,
        maximum_resident_bytes: 0,
    };
    for iteration in 0..iterations {
        let mut session = prepared.start_live_session(initial(prepared)).unwrap();
        let old = PresentationDocument::open_source(session.overlay()).unwrap();
        let total_started = Instant::now();
        let apply_started = Instant::now();
        let update = session.apply_delta(0, 1, delta(iteration)).unwrap();
        measurement
            .apply
            .push(apply_started.elapsed().as_nanos() as u64);
        let next = PresentationDocument::open_source(session.overlay()).unwrap();
        let mut invalidated =
            old.invalidated_slides_for_parts(update.changed_parts.iter().map(String::as_str));
        invalidated.extend(
            next.invalidated_slides_for_parts(update.changed_parts.iter().map(String::as_str)),
        );
        invalidated.sort_unstable();
        invalidated.dedup();
        if old.slide_part_names() != next.slide_part_names() {
            invalidated = (0..next.slide_count()).collect();
        }
        for index in &invalidated {
            black_box(DisplayList::from_resolve(&next.resolve_slide(*index).unwrap()).encode());
        }
        measurement
            .render_ready
            .push(total_started.elapsed().as_nanos() as u64);
        measurement.maximum_invalidated_slides = measurement
            .maximum_invalidated_slides
            .max(invalidated.len());
        measurement.minimum_reused_parts = measurement
            .minimum_reused_parts
            .min(update.reused_materialized_parts);
        measurement.maximum_resident_bytes = measurement
            .maximum_resident_bytes
            .max(session.estimated_resident_bytes());
    }
    measurement
}

fn prepare(bytes: &[u8]) -> Arc<PreparedTemplate> {
    let bytes: Arc<[u8]> = Arc::from(bytes);
    let archive = ZipArchive::from_bytes(bytes.clone()).unwrap();
    let plan = TemplateCompiler::new(Default::default())
        .compile(&archive)
        .unwrap()
        .plan;
    Arc::new(PreparedTemplate::new(bytes, plan).unwrap())
}

fn complete_binding_data(prepared: &PreparedTemplate) -> InjectionData {
    let mut data = InjectionData::new();
    for binding in &prepared.plan().bindings {
        match binding.kind {
            BindingKind::Text => data.insert_text(&binding.id, format!("initial {}", binding.id)),
            BindingKind::Image => data.insert_image(
                &binding.id,
                ImageData {
                    bytes: Arc::from(b"benchmark image".as_slice()),
                    extension: "png".into(),
                    content_type: "image/png".into(),
                    crop: None,
                    fit: Default::default(),
                },
            ),
            BindingKind::Chart => data.set_chart(&binding.id, chart_data(10.0)),
        }
    }
    data
}

fn dogfood_data(prepared: &PreparedTemplate) -> InjectionData {
    let mut data = complete_binding_data(prepared);
    data.set_table_rows("metrics", rows("initial"));
    data.set_slide_copies("ppt/slides/slide2.xml", 1);
    data
}

fn advanced_data(prepared: &PreparedTemplate) -> InjectionData {
    complete_binding_data(prepared)
}

fn rows(value: &str) -> Vec<BTreeMap<String, String>> {
    ["Latency", "Throughput"]
        .into_iter()
        .map(|label| {
            BTreeMap::from([
                ("label".to_owned(), label.to_owned()),
                ("value".to_owned(), value.to_owned()),
            ])
        })
        .collect()
}

fn chart_data(value: f64) -> ChartData {
    ChartData {
        categories: vec!["Q1".to_owned(), "Q2".to_owned()],
        series: vec![ChartSeriesData {
            name: "Revenue".to_owned(),
            values: vec![value, value * 2.0],
        }],
    }
}

fn array(values: &[u64]) -> String {
    values
        .iter()
        .map(u64::to_string)
        .collect::<Vec<_>>()
        .join(",")
}
