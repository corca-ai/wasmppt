use std::{fs, sync::Arc};

use wasmppt_native::{FileSink, FileSource};
use wasmppt_opc::{PackageLimits, ReadAt, ZipArchive};
use wasmppt_template::{InjectionData, PreparedTemplate, TemplateCompiler};

#[test]
fn file_source_and_sink_execute_the_shared_host_fixture() {
    let Ok(path) = std::env::var("WASMPPT_HOST_FIXTURE") else {
        return;
    };
    let path = std::path::PathBuf::from(path);
    let path = if path.is_absolute() {
        path
    } else {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(path)
    };
    let source = FileSource::open(&path).unwrap();
    let archive = ZipArchive::open(source, PackageLimits::default()).unwrap();
    let plan = TemplateCompiler::new(Default::default())
        .compile(&archive)
        .unwrap()
        .plan;
    let bytes: Arc<[u8]> = fs::read(path).unwrap().into();
    let prepared = PreparedTemplate::new(bytes, plan).unwrap();

    let output = std::env::temp_dir().join(format!(
        "wasmppt-native-adapter-{}.pptx",
        std::process::id()
    ));
    let sink = FileSink::create(&output).unwrap();
    let (sink, _) = prepared.generate_to(&InjectionData::new(), sink).unwrap();
    let length = sink.finish().unwrap();
    assert!(length > 0);

    let generated = FileSource::open(&output).unwrap();
    assert_eq!(generated.len(), length);
    ZipArchive::open(generated, PackageLimits::default()).unwrap();
    fs::remove_file(output).unwrap();
}

#[test]
fn file_source_rejects_out_of_bounds_reads() {
    let path = std::env::temp_dir().join(format!("wasmppt-read-at-{}.bin", std::process::id()));
    fs::write(&path, b"host adapter").unwrap();
    let source = FileSource::open(&path).unwrap();
    let mut bytes = [0_u8; 4];
    source.read_at(5, &mut bytes).unwrap();
    assert_eq!(&bytes, b"adap");
    assert!(source.read_at(10, &mut bytes).is_err());
    fs::remove_file(path).unwrap();
}
