use std::{
    env, fs,
    hint::black_box,
    io::{Read, Write},
    sync::Arc,
    time::Instant,
};

use flate2::{Compression, read::ZlibDecoder, write::ZlibEncoder};
use wasmppt_display::DisplayList;
use wasmppt_layout::PresentationDocument;
use wasmppt_opc::ZipArchive;
use wasmppt_template::{
    GenerateStats, ImageData, InjectionData, PreparedTemplate, TemplateCompiler,
};

fn main() {
    let mut args = env::args().skip(1);
    let path = args
        .next()
        .expect("usage: benchmark FIXTURE SCENARIO SLIDES ITERATIONS");
    let scenario = args.next().expect("missing scenario");
    let slides: usize = args.next().expect("missing slides").parse().unwrap();
    let iterations: usize = args.next().expect("missing iterations").parse().unwrap();
    assert!(iterations >= 3 && args.next().is_none());
    let bytes: Arc<[u8]> = fs::read(path).unwrap().into();
    let data = injection_data(&scenario, slides);

    let mut cold = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        let start = Instant::now();
        let archive = ZipArchive::from_bytes(bytes.clone()).unwrap();
        let plan = TemplateCompiler::new(Default::default())
            .compile(&archive)
            .unwrap()
            .plan;
        black_box(PreparedTemplate::new(bytes.clone(), plan).unwrap());
        cold.push(start.elapsed().as_nanos() as u64);
    }
    let archive = ZipArchive::from_bytes(bytes.clone()).unwrap();
    let plan = TemplateCompiler::new(Default::default())
        .compile(&archive)
        .unwrap()
        .plan;
    let prepared = PreparedTemplate::new(bytes.clone(), plan).unwrap();
    let mut warm = Vec::with_capacity(iterations);
    let mut output = None;
    for _ in 0..iterations {
        let start = Instant::now();
        let generated = prepared
            .generate_to(&data, wasmppt_opc::VecSink::new())
            .unwrap();
        warm.push(start.elapsed().as_nanos() as u64);
        output = Some(generated);
    }
    let (sink, generation_stats) = output.unwrap();
    let generated_bytes = sink.into_inner();
    verify_generated_output(&generated_bytes, &scenario, slides);
    let deck = PresentationDocument::open(generated_bytes.clone()).unwrap();
    assert_eq!(deck.slide_count(), slides);
    assert_eq!(&generated_bytes[..2], b"PK");

    let mut cursor = prepared.generate_cursor(&data).unwrap();
    let mut streamed = Vec::new();
    while !cursor.is_done() {
        streamed.extend(cursor.pull(64 * 1024).unwrap());
    }
    assert_eq!(streamed, generated_bytes);
    let streaming_stats = cursor.stats().unwrap();

    let mut first = Vec::with_capacity(iterations);
    let mut visible = Vec::with_capacity(iterations);
    let mut all = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        first.push(measure_resolution(&generated_bytes, 1));
        visible.push(measure_resolution(&generated_bytes, slides.min(3)));
        all.push(measure_resolution(&generated_bytes, slides));
    }
    let display = DisplayList::from_resolve(&deck.resolve_slide(0).unwrap()).encode();
    assert!(display.starts_with(b"WPDL"));
    print_result(
        &scenario,
        slides,
        bytes.len(),
        generated_bytes.len(),
        prepared.estimated_resident_bytes(),
        generation_stats,
        streaming_stats.maximum_output_chunk_bytes,
        &cold,
        &warm,
        &first,
        &visible,
        &all,
    );
}

fn verify_generated_output(bytes: &[u8], scenario: &str, slides: usize) {
    let archive = ZipArchive::from_bytes(bytes.to_vec()).unwrap();
    if scenario != "image" {
        let first_slide = archive.entry("ppt/slides/slide1.xml").unwrap();
        let source = archive.read_entry(first_slide).unwrap();
        assert!(
            String::from_utf8(source)
                .unwrap()
                .contains("Slide 0 field 0")
        );
    }
    if scenario != "text" {
        let images = archive
            .entries()
            .iter()
            .filter(|entry| entry.name.starts_with("ppt/media/wasmppt-image_"))
            .collect::<Vec<_>>();
        assert_eq!(images.len(), slides);
        for entry in images {
            validate_png(&archive.read_entry(entry).unwrap());
        }
    }
}

fn injection_data(scenario: &str, slides: usize) -> InjectionData {
    let mut data = InjectionData::new();
    for slide in 0..slides {
        if scenario != "image" {
            for field in 0..8 {
                data.insert_text(
                    format!("text_{slide}_{field}"),
                    format!(
                        "Slide {slide} field {field}: 한국어 العربية 👨🏽‍💻 {}",
                        "benchmark payload ".repeat(24)
                    ),
                );
            }
        }
        if scenario != "text" {
            data.insert_image(
                format!("image_{slide}"),
                ImageData {
                    bytes: image_bytes(64 * 1024),
                    extension: "png".into(),
                    content_type: "image/png".into(),
                    crop: None,
                    fit: Default::default(),
                },
            );
        }
    }
    data
}

fn image_bytes(size: usize) -> Vec<u8> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(&[0, 0, 0, 0, 0]).unwrap();
    let compressed = encoder.finish().unwrap();
    let fixed = 8 + (12 + 13) + 12 + (12 + compressed.len()) + 12;
    assert!(size >= fixed);
    let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
    png_chunk(
        &mut bytes,
        b"IHDR",
        &[0, 0, 0, 1, 0, 0, 0, 1, 8, 6, 0, 0, 0],
    );
    let padding = (0..size - fixed)
        .map(|index| (index % 251) as u8)
        .collect::<Vec<_>>();
    png_chunk(&mut bytes, b"vpAg", &padding);
    png_chunk(&mut bytes, b"IDAT", &compressed);
    png_chunk(&mut bytes, b"IEND", &[]);
    assert_eq!(bytes.len(), size);
    bytes
}

fn png_chunk(output: &mut Vec<u8>, kind: &[u8; 4], payload: &[u8]) {
    output.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    output.extend_from_slice(kind);
    output.extend_from_slice(payload);
    let mut crc = crc32fast::Hasher::new();
    crc.update(kind);
    crc.update(payload);
    output.extend_from_slice(&crc.finalize().to_be_bytes());
}

fn validate_png(bytes: &[u8]) {
    assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"));
    let mut cursor = 8;
    let mut compressed = Vec::new();
    let mut saw_header = false;
    let mut saw_end = false;
    while cursor < bytes.len() {
        let length = u32::from_be_bytes(bytes[cursor..cursor + 4].try_into().unwrap()) as usize;
        let kind: &[u8; 4] = bytes[cursor + 4..cursor + 8].try_into().unwrap();
        let payload = &bytes[cursor + 8..cursor + 8 + length];
        let expected = u32::from_be_bytes(
            bytes[cursor + 8 + length..cursor + 12 + length]
                .try_into()
                .unwrap(),
        );
        let mut crc = crc32fast::Hasher::new();
        crc.update(kind);
        crc.update(payload);
        assert_eq!(crc.finalize(), expected);
        match kind {
            b"IHDR" => saw_header = payload == [0, 0, 0, 1, 0, 0, 0, 1, 8, 6, 0, 0, 0],
            b"IDAT" => compressed.extend_from_slice(payload),
            b"IEND" => saw_end = true,
            _ => {}
        }
        cursor += 12 + length;
    }
    assert!(saw_header && saw_end && cursor == bytes.len());
    let mut pixels = Vec::new();
    ZlibDecoder::new(compressed.as_slice())
        .read_to_end(&mut pixels)
        .unwrap();
    assert_eq!(pixels, [0, 0, 0, 0, 0]);
}

fn measure_resolution(bytes: &[u8], count: usize) -> u64 {
    let start = Instant::now();
    let deck = PresentationDocument::open(bytes.to_vec()).unwrap();
    for index in 0..count {
        black_box(DisplayList::from_resolve(&deck.resolve_slide(index).unwrap()).encode());
    }
    start.elapsed().as_nanos() as u64
}

fn array(values: &[u64]) -> String {
    values
        .iter()
        .map(u64::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

#[allow(clippy::too_many_arguments)]
fn print_result(
    scenario: &str,
    slides: usize,
    input: usize,
    output: usize,
    resident: u64,
    stats: GenerateStats,
    maximum_output_chunk_bytes: u64,
    cold: &[u64],
    warm: &[u64],
    first: &[u64],
    visible: &[u64],
    all: &[u64],
) {
    println!(
        "{{\"schema\":1,\"scenario\":\"{scenario}\",\"slides\":{slides},\"iterations\":{},\"inputBytes\":{input},\"outputBytes\":{output},\"estimatedResidentBytes\":{resident},\"copies\":{{\"input\":1,\"output\":1}},\"zip\":{{\"entries\":{},\"rawCopiedEntries\":{},\"rawCopiedBytes\":{},\"inflatedEntries\":{},\"recompressedEntries\":{}}},\"generation\":{{\"rewrittenEntries\":{},\"removedEntries\":{},\"dirtyUncompressedBytes\":{},\"peakDirtyEntryBytes\":{},\"maximumOutputChunkBytes\":{maximum_output_chunk_bytes}}},\"samplesNs\":{{\"coldTemplateCompile\":[{}],\"warmInjection\":[{}],\"firstSlide\":[{}],\"visibleSlides\":[{}],\"allSlides\":[{}]}}}}",
        cold.len(),
        stats.zip.entries,
        stats.zip.raw_copied_entries,
        stats.zip.raw_copied_bytes,
        stats.zip.inflated_entries,
        stats.zip.recompressed_entries,
        stats.rewritten_entries,
        stats.removed_entries,
        stats.dirty_uncompressed_bytes,
        stats.peak_dirty_entry_bytes,
        array(cold),
        array(warm),
        array(first),
        array(visible),
        array(all)
    );
}
