use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use wasmppt_opc::{
    CompressionMethod, EntryOptions, ErrorCode, OverlayLimits, OverlayPart, PackageOverlay,
    PackagePartSource, VecSink, ZipArchive, ZipWriter,
};

fn source() -> Vec<u8> {
    let mut writer = ZipWriter::new(VecSink::new());
    let options = EntryOptions::deterministic(CompressionMethod::Deflate);
    writer.write_entry("a.xml", b"unchanged", &options).unwrap();
    writer.write_entry("b.xml", b"old", &options).unwrap();
    writer
        .write_entry("removed.xml", b"gone", &options)
        .unwrap();
    writer.finish().unwrap().0.into_inner()
}

#[test]
fn exposes_one_complete_immutable_logical_revision() {
    let overlay = PackageOverlay::new(
        source(),
        BTreeMap::from([
            (
                "b.xml".to_owned(),
                OverlayPart::deflated(Arc::<[u8]>::from(&b"new"[..])),
            ),
            (
                "new.xml".to_owned(),
                OverlayPart::deflated(Arc::<[u8]>::from(&b"added"[..])),
            ),
        ]),
        BTreeSet::from(["removed.xml".to_owned()]),
        &OverlayLimits::default(),
    )
    .unwrap();

    assert_eq!(overlay.part_names(), ["a.xml", "b.xml", "new.xml"]);
    assert_eq!(overlay.read_part("a.xml").unwrap(), b"unchanged");
    assert_eq!(overlay.read_part("b.xml").unwrap(), b"new");
    assert_eq!(overlay.read_part("new.xml").unwrap(), b"added");
    assert!(!overlay.contains_part("removed.xml"));
    assert!(overlay.is_modified("b.xml"));
    assert!(!overlay.is_modified("a.xml"));
    assert_eq!(overlay.stats().materialized_parts, 2);
    assert_eq!(overlay.stats().removed_parts, 1);
}

#[test]
fn pull_export_is_exact_with_one_byte_output_chunks_and_raw_reuse() {
    let source_bytes = source();
    let source_archive = ZipArchive::from_bytes(source_bytes.clone()).unwrap();
    let source_compressed = source_archive
        .read_compressed(source_archive.entry("a.xml").unwrap())
        .unwrap();
    let overlay = PackageOverlay::new(
        source_bytes,
        BTreeMap::from([(
            "b.xml".to_owned(),
            OverlayPart::deflated(Arc::<[u8]>::from(&b"new"[..])),
        )]),
        BTreeSet::from(["removed.xml".to_owned()]),
        &OverlayLimits::default(),
    )
    .unwrap();
    let mut cursor = overlay.generation_cursor();
    let mut bytes = Vec::new();
    while !cursor.is_done() {
        bytes.extend(cursor.pull(1).unwrap());
    }
    let archive = ZipArchive::from_bytes(bytes).unwrap();
    assert_eq!(
        archive.read_entry(archive.entry("b.xml").unwrap()).unwrap(),
        b"new"
    );
    assert_eq!(
        archive
            .read_compressed(archive.entry("a.xml").unwrap())
            .unwrap(),
        source_compressed
    );
    let (stats, maximum_chunk) = cursor.stats().unwrap();
    assert_eq!(maximum_chunk, 1);
    assert_eq!(stats.raw_copied_entries, 1);
}

#[test]
fn changed_parts_compare_logical_bytes_and_name_sets() {
    let first = PackageOverlay::new(
        source(),
        BTreeMap::from([(
            "b.xml".to_owned(),
            OverlayPart::deflated(Arc::<[u8]>::from(&b"one"[..])),
        )]),
        BTreeSet::new(),
        &OverlayLimits::default(),
    )
    .unwrap();
    let second = PackageOverlay::new(
        source(),
        BTreeMap::from([
            (
                "b.xml".to_owned(),
                OverlayPart::deflated(Arc::<[u8]>::from(&b"two"[..])),
            ),
            (
                "new.xml".to_owned(),
                OverlayPart::deflated(Arc::<[u8]>::from(&b"new"[..])),
            ),
        ]),
        BTreeSet::from(["removed.xml".to_owned()]),
        &OverlayLimits::default(),
    )
    .unwrap();
    assert_eq!(
        second.changed_parts_since(&first),
        ["b.xml", "new.xml", "removed.xml"]
    );
}

#[test]
fn changed_parts_include_untouched_parts_when_physical_sources_differ() {
    let first = PackageOverlay::new(
        source(),
        BTreeMap::new(),
        BTreeSet::new(),
        &OverlayLimits::default(),
    )
    .unwrap();
    let archive = ZipArchive::from_bytes(source()).unwrap();
    let mut writer = ZipWriter::new(VecSink::new());
    let options = EntryOptions::deterministic(CompressionMethod::Deflate);
    for entry in archive.entries() {
        let bytes = if entry.name == "a.xml" {
            b"different".to_vec()
        } else {
            archive.read_entry(entry).unwrap()
        };
        writer.write_entry(&entry.name, &bytes, &options).unwrap();
    }
    let changed_source = writer.finish().unwrap().0.into_inner();
    let second = PackageOverlay::new(
        changed_source,
        BTreeMap::new(),
        BTreeSet::new(),
        &OverlayLimits::default(),
    )
    .unwrap();
    assert_eq!(second.changed_parts_since(&first), ["a.xml"]);
}

#[test]
fn rejects_unsafe_names_and_materialization_overruns() {
    let unsafe_name = PackageOverlay::new(
        source(),
        BTreeMap::from([(
            "../escape".to_owned(),
            OverlayPart::deflated(Arc::<[u8]>::from(&b"x"[..])),
        )]),
        BTreeSet::new(),
        &OverlayLimits::default(),
    )
    .unwrap_err();
    assert_eq!(unsafe_name.code(), ErrorCode::InvalidPath);

    let overrun = PackageOverlay::new(
        source(),
        BTreeMap::from([(
            "new.xml".to_owned(),
            OverlayPart::deflated(Arc::<[u8]>::from(&b"xx"[..])),
        )]),
        BTreeSet::new(),
        &OverlayLimits {
            max_materialized_parts: 1,
            max_materialized_bytes: 1,
        },
    )
    .unwrap_err();
    assert_eq!(overrun.code(), ErrorCode::LimitExceeded);
}
