#![no_main]

use libfuzzer_sys::fuzz_target;
use wasmppt_opc::ZipArchive;
use wasmppt_template::TemplateCompiler;

fuzz_target!(|bytes: &[u8]| {
    if let Ok(archive) = ZipArchive::from_bytes(bytes) {
        let _ = TemplateCompiler::new(Default::default()).compile(&archive);
    }
});
