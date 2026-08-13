use std::{collections::BTreeMap, env, fs, path::PathBuf, sync::Arc};

use wasmppt_opc::ZipArchive;
use wasmppt_template::{
    ImageData, InjectionData, PreparedTemplate, TableOverflowPolicy, TablePolicyData,
    TemplateCompiler,
};

const DOGFOOD: &[u8] = include_bytes!("../../../fixtures/dogfood/report.potx");

fn main() {
    let output = env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .expect("usage: write_table_continuation_fixture OUTPUT.pptx");
    let source = ZipArchive::from_bytes(DOGFOOD.to_vec()).expect("open dogfood template");
    let original_image = source
        .read_entry(
            source
                .entry("ppt/media/original.png")
                .expect("dogfood image part"),
        )
        .expect("read dogfood image");
    let plan = TemplateCompiler::new(Default::default())
        .compile(&source)
        .expect("compile dogfood template")
        .plan;
    let prepared = PreparedTemplate::new(DOGFOOD.to_vec(), plan).expect("prepare dogfood template");

    let rows = (0..5)
        .map(|index| {
            BTreeMap::from([
                ("label".to_owned(), format!("Metric {}", index + 1)),
                ("value".to_owned(), format!("{} ms", 10 + index)),
            ])
        })
        .collect();
    let data = InjectionData::new()
        .with_text("title", "Continued table compatibility")
        .with_text(
            "subtitle",
            "Rows are partitioned into authored slide copies",
        )
        .with_image(
            "hero",
            ImageData {
                bytes: Arc::from(original_image),
                extension: "png".to_owned(),
                content_type: "image/png".to_owned(),
                crop: None,
                fit: Default::default(),
            },
        )
        .with_table_rows("metrics", rows)
        .with_table_policy(
            "metrics",
            TablePolicyData {
                maximum_rows: 2,
                overflow: TableOverflowPolicy::Continue,
            },
        );
    let generated = prepared.generate(&data).expect("generate continued table");
    fs::write(output, generated.bytes).expect("write continued-table fixture");
}
