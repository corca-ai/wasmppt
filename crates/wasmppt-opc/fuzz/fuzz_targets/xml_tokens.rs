#![no_main]

use libfuzzer_sys::fuzz_target;
use wasmppt_xml::XmlDocument;

fuzz_target!(|bytes: &[u8]| {
    let _ = XmlDocument::parse(bytes);
});
