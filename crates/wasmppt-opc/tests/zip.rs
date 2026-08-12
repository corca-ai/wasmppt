use std::io::{self, Write};

use proptest::prelude::*;
use wasmppt_opc::{
    CompressionMethod, EntryOptions, ErrorCode, PackageLimits, RewriteMode, StreamingZipWriter,
    VecSink, WriteSink, ZipArchive, ZipWriter, rewrite_archive, rewrite_archive_to_vec,
};

fn fixture() -> Vec<u8> {
    let mut writer = ZipWriter::new(VecSink::new());
    writer
        .write_entry(
            "ppt/slides/slide1.xml",
            b"<p:sld>Hello</p:sld>",
            &EntryOptions::deterministic(CompressionMethod::Deflate),
        )
        .unwrap();
    writer
        .write_entry(
            "[Content_Types].xml",
            b"<Types/>",
            &EntryOptions::deterministic(CompressionMethod::Stored),
        )
        .unwrap();
    writer.finish().unwrap().0.into_inner()
}

fn push_u16(bytes: &mut Vec<u8>, value: u16) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn push_u32(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn overwrite_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn data_descriptor_fixture() -> Vec<u8> {
    let name = b"doc.xml";
    let payload = b"descriptor payload";
    let crc32 = crc32fast::hash(payload);
    let size = u32::try_from(payload.len()).unwrap();
    let flags = (1 << 11) | (1 << 3);
    let mut bytes = Vec::new();

    push_u32(&mut bytes, 0x0403_4b50);
    push_u16(&mut bytes, 20);
    push_u16(&mut bytes, flags);
    push_u16(&mut bytes, 0);
    push_u16(&mut bytes, 0);
    push_u16(&mut bytes, 0x21);
    push_u32(&mut bytes, 0);
    push_u32(&mut bytes, 0);
    push_u32(&mut bytes, 0);
    push_u16(&mut bytes, u16::try_from(name.len()).unwrap());
    push_u16(&mut bytes, 0);
    bytes.extend_from_slice(name);
    bytes.extend_from_slice(payload);
    push_u32(&mut bytes, 0x0807_4b50);
    push_u32(&mut bytes, crc32);
    push_u32(&mut bytes, size);
    push_u32(&mut bytes, size);

    let central_offset = u32::try_from(bytes.len()).unwrap();
    push_u32(&mut bytes, 0x0201_4b50);
    push_u16(&mut bytes, 20);
    push_u16(&mut bytes, 20);
    push_u16(&mut bytes, flags);
    push_u16(&mut bytes, 0);
    push_u16(&mut bytes, 0);
    push_u16(&mut bytes, 0x21);
    push_u32(&mut bytes, crc32);
    push_u32(&mut bytes, size);
    push_u32(&mut bytes, size);
    push_u16(&mut bytes, u16::try_from(name.len()).unwrap());
    push_u16(&mut bytes, 0);
    push_u16(&mut bytes, 0);
    push_u16(&mut bytes, 0);
    push_u16(&mut bytes, 0);
    push_u32(&mut bytes, 0);
    push_u32(&mut bytes, 0);
    bytes.extend_from_slice(name);
    let central_size = u32::try_from(bytes.len()).unwrap() - central_offset;

    push_u32(&mut bytes, 0x0605_4b50);
    push_u16(&mut bytes, 0);
    push_u16(&mut bytes, 0);
    push_u16(&mut bytes, 1);
    push_u16(&mut bytes, 1);
    push_u32(&mut bytes, central_size);
    push_u32(&mut bytes, central_offset);
    push_u16(&mut bytes, 0);
    bytes
}

#[test]
fn indexes_and_inflates_stored_and_deflated_entries() {
    let bytes = fixture();
    let archive = ZipArchive::from_bytes(bytes).unwrap();
    assert_eq!(archive.entries().len(), 2);
    assert_eq!(
        archive
            .read_entry(archive.entry("ppt/slides/slide1.xml").unwrap())
            .unwrap(),
        b"<p:sld>Hello</p:sld>"
    );
    assert_eq!(
        archive
            .read_entry(archive.entry("[Content_Types].xml").unwrap())
            .unwrap(),
        b"<Types/>"
    );
}

#[test]
fn no_op_rewrite_copies_every_compressed_payload_verbatim() {
    let source = fixture();
    let archive = ZipArchive::from_bytes(source).unwrap();
    let original_payloads = archive
        .entries()
        .iter()
        .map(|entry| (entry.name.clone(), archive.read_compressed(entry).unwrap()))
        .collect::<Vec<_>>();

    let (output, stats) = rewrite_archive_to_vec(&archive, RewriteMode::Preserve).unwrap();
    assert_eq!(stats.entries, 2);
    assert_eq!(stats.raw_copied_entries, 2);
    assert_eq!(stats.inflated_entries, 0);
    assert_eq!(stats.recompressed_entries, 0);

    let rewritten = ZipArchive::from_bytes(output).unwrap();
    for (name, original) in original_payloads {
        assert_eq!(
            rewritten
                .read_compressed(rewritten.entry(&name).unwrap())
                .unwrap(),
            original
        );
    }
}

#[derive(Debug, Default)]
struct AppendOnly(Vec<u8>);

impl Write for AppendOnly {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn rewrite_accepts_a_sink_that_cannot_seek() {
    let archive = ZipArchive::from_bytes(fixture()).unwrap();
    let sink = WriteSink::new(AppendOnly::default());
    let (sink, _) = rewrite_archive(&archive, sink, RewriteMode::Preserve).unwrap();
    let output = sink.into_inner().0;
    let result = ZipArchive::from_bytes(output).unwrap();
    assert_eq!(result.entries().len(), 2);
}

#[test]
fn pull_writer_matches_the_forward_only_writer_for_tiny_chunks() {
    let source = fixture();
    let archive = ZipArchive::from_bytes(source.clone()).unwrap();
    let first = archive.entry("ppt/slides/slide1.xml").unwrap().clone();
    let second = archive.entry("[Content_Types].xml").unwrap().clone();

    let mut expected = ZipWriter::new(VecSink::new());
    expected
        .raw_copy(&archive, &first, RewriteMode::Preserve)
        .unwrap();
    expected
        .write_entry(
            &second.name,
            b"<Types><Default/></Types>",
            &EntryOptions::deterministic(CompressionMethod::Deflate),
        )
        .unwrap();
    let expected = expected.finish().unwrap().0.into_inner();

    let mut writer = StreamingZipWriter::new(archive.source().clone());
    let mut output = Vec::new();
    writer
        .start_raw_copy(&first, RewriteMode::Preserve)
        .unwrap();
    while writer.entry_active() {
        output.extend(writer.pull(7).unwrap());
    }
    writer
        .start_entry(
            second.name,
            b"<Types><Default/></Types>".to_vec(),
            EntryOptions::deterministic(CompressionMethod::Deflate),
        )
        .unwrap();
    while writer.entry_active() {
        output.extend(writer.pull(7).unwrap());
    }
    writer.start_finish().unwrap();
    while !writer.is_done() {
        output.extend(writer.pull(7).unwrap());
    }

    assert_eq!(output, expected);
    assert_eq!(writer.stats().entries, 2);
    assert_eq!(writer.stats().raw_copied_entries, 1);
    assert_eq!(writer.stats().recompressed_entries, 1);
}

#[test]
fn validates_source_data_descriptors_and_rebuilds_headers_without_them() {
    let archive = ZipArchive::from_bytes(data_descriptor_fixture()).unwrap();
    let entry = archive.entry("doc.xml").unwrap();
    assert_ne!(entry.flags & (1 << 3), 0);
    assert_eq!(archive.read_entry(entry).unwrap(), b"descriptor payload");

    let output = rewrite_archive_to_vec(&archive, RewriteMode::Preserve)
        .unwrap()
        .0;
    let rewritten = ZipArchive::from_bytes(output).unwrap();
    let entry = rewritten.entry("doc.xml").unwrap();
    assert_eq!(entry.flags & (1 << 3), 0);
    assert_eq!(rewritten.read_entry(entry).unwrap(), b"descriptor payload");
}

#[test]
fn deterministic_rewrite_is_byte_identical_and_name_sorted() {
    let archive = ZipArchive::from_bytes(fixture()).unwrap();
    let first = rewrite_archive_to_vec(&archive, RewriteMode::Deterministic)
        .unwrap()
        .0;
    let second = rewrite_archive_to_vec(&archive, RewriteMode::Deterministic)
        .unwrap()
        .0;
    assert_eq!(first, second);

    let output = ZipArchive::from_bytes(first).unwrap();
    let names = output
        .entries()
        .iter()
        .map(|entry| entry.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(names, ["[Content_Types].xml", "ppt/slides/slide1.xml"]);
}

#[test]
fn package_limits_are_checked_before_entry_inflation() {
    let limits = PackageLimits {
        max_entries: 1,
        ..PackageLimits::default()
    };
    let error = ZipArchive::from_bytes_with_limits(fixture(), limits).unwrap_err();
    assert_eq!(error.code(), ErrorCode::LimitExceeded);
}

#[test]
fn unsafe_paths_are_rejected_on_write() {
    let mut writer = ZipWriter::new(VecSink::new());
    let error = writer
        .write_entry(
            "../escape.xml",
            b"bad",
            &EntryOptions::deterministic(CompressionMethod::Stored),
        )
        .unwrap_err();
    assert_eq!(error.code(), ErrorCode::InvalidPath);
}

#[test]
fn duplicate_names_are_rejected_on_write() {
    let options = EntryOptions::deterministic(CompressionMethod::Stored);
    let mut writer = ZipWriter::new(VecSink::new());
    writer.write_entry("same.xml", b"first", &options).unwrap();
    let error = writer
        .write_entry("same.xml", b"second", &options)
        .unwrap_err();
    assert_eq!(error.code(), ErrorCode::DuplicateEntry);
}

#[test]
fn overlapping_local_records_are_rejected() {
    let mut bytes = fixture();
    let local_offsets = bytes
        .windows(4)
        .enumerate()
        .filter_map(|(offset, window)| (window == b"PK\x03\x04").then_some(offset))
        .collect::<Vec<_>>();
    assert_eq!(local_offsets.len(), 2);
    let first_name_len = u16::from_le_bytes([bytes[26], bytes[27]]) as usize;
    let first_extra_len = u16::from_le_bytes([bytes[28], bytes[29]]) as usize;
    let first_data_offset = 30 + first_name_len + first_extra_len;
    let overlapping_size = u32::try_from(local_offsets[1] - first_data_offset + 1).unwrap();
    overwrite_u32(&mut bytes, 18, overlapping_size);

    let eocd = bytes
        .windows(4)
        .rposition(|window| window == b"PK\x05\x06")
        .unwrap();
    let central_offset = u32::from_le_bytes([
        bytes[eocd + 16],
        bytes[eocd + 17],
        bytes[eocd + 18],
        bytes[eocd + 19],
    ]) as usize;
    overwrite_u32(&mut bytes, central_offset + 20, overlapping_size);

    let error = ZipArchive::from_bytes(bytes).unwrap_err();
    assert_eq!(error.code(), ErrorCode::OverlappingEntries);
}

#[test]
fn crc_is_checked_when_an_entry_is_read() {
    let mut bytes = fixture();
    let archive = ZipArchive::from_bytes(bytes.clone()).unwrap();
    let entry = archive.entry("[Content_Types].xml").unwrap();
    let payload_offset = usize::try_from(entry.compressed_range().start).unwrap();
    bytes[payload_offset] ^= 0xff;

    let corrupted = ZipArchive::from_bytes(bytes).unwrap();
    let error = corrupted
        .read_entry(corrupted.entry("[Content_Types].xml").unwrap())
        .unwrap_err();
    assert_eq!(error.code(), ErrorCode::ChecksumMismatch);
}

proptest! {
    #[test]
    fn arbitrary_input_never_panics(bytes in proptest::collection::vec(any::<u8>(), 0..8192)) {
        let _ = ZipArchive::from_bytes(bytes);
    }

    #[test]
    fn valid_entry_round_trips(name_suffix in "[a-zA-Z0-9_-]{1,32}", payload in proptest::collection::vec(any::<u8>(), 0..4096)) {
        let name = format!("ppt/media/{name_suffix}.bin");
        let mut writer = ZipWriter::new(VecSink::new());
        writer.write_entry(
            &name,
            &payload,
            &EntryOptions::deterministic(CompressionMethod::Deflate),
        ).unwrap();
        let bytes = writer.finish().unwrap().0.into_inner();
        let archive = ZipArchive::from_bytes(bytes).unwrap();
        prop_assert_eq!(archive.read_entry(archive.entry(&name).unwrap()).unwrap(), payload);
    }
}
