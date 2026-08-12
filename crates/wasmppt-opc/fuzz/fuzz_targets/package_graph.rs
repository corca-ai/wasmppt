#![no_main]

use libfuzzer_sys::fuzz_target;
use wasmppt_opc::{
    CompressionMethod, EntryOptions, PackageGraph, VecSink, ZipArchive, ZipWriter,
};

fuzz_target!(|bytes: &[u8]| {
    if let Ok(archive) = ZipArchive::from_bytes(bytes) {
        let _ = PackageGraph::build(&archive);
    }

    let stored = EntryOptions::deterministic(CompressionMethod::Stored);
    let mut writer = ZipWriter::new(VecSink::new());
    writer
        .write_entry(
            "[Content_Types].xml",
            br#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/></Types>"#,
            &stored,
        )
        .unwrap();
    writer.write_entry("_rels/.rels", bytes, &stored).unwrap();
    writer
        .write_entry(
            "ppt/presentation.xml",
            br#"<p:presentation xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006"><mc:AlternateContent><mc:Choice Requires="future"><p:extLst/></mc:Choice></mc:AlternateContent></p:presentation>"#,
            &stored,
        )
        .unwrap();
    let package = writer.finish().unwrap().0.into_inner();
    let archive = ZipArchive::from_bytes(package).unwrap();
    let _ = PackageGraph::build(&archive);
});
