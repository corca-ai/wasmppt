#![no_main]

use libfuzzer_sys::fuzz_target;
use wasmppt_opc::ZipArchive;

fuzz_target!(|bytes: &[u8]| {
    if let Ok(archive) = ZipArchive::from_bytes(bytes) {
        for entry in archive.entries().iter().take(8) {
            let _ = archive.read_entry(entry);
        }
    }
});
