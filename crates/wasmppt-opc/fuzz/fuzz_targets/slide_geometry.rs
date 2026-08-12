#![no_main]

use libfuzzer_sys::fuzz_target;
use wasmppt_layout::PresentationDocument;

fuzz_target!(|bytes: &[u8]| {
    if let Ok(document) = PresentationDocument::open(bytes) {
        for index in 0..document.slide_count().min(2) {
            let _ = document.resolve_slide(index);
        }
    }
});
