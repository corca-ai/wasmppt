use std::{env, fs, path::PathBuf};

use wasmppt_opc::{CompressionMethod, EntryOptions, VecSink, ZipWriter};

const GEOMETRIES: [&str; 10] = [
    "rect",
    "roundRect",
    "ellipse",
    "triangle",
    "diamond",
    "hexagon",
    "star5",
    "chevron",
    "rightArrow",
    "plus",
];

fn main() {
    let directory = env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .expect("usage: write_compat_corpus OUTPUT_DIRECTORY");
    fs::create_dir_all(&directory).expect("create corpus directory");
    for case in 1..=50 {
        let path = directory.join(format!("generated-{case:02}.pptx"));
        fs::write(path, presentation(case)).expect("write corpus fixture");
    }
}

fn presentation(case: usize) -> Vec<u8> {
    let options = EntryOptions::deterministic(CompressionMethod::Deflate);
    let mut writer = ZipWriter::new(VecSink::new());
    let geometry = GEOMETRIES[(case - 1) % GEOMETRIES.len()];
    let direction = if case % 5 == 0 { " rtl=\"1\"" } else { "" };
    let vertical = if case % 7 == 0 {
        " vert=\"vert270\""
    } else {
        ""
    };
    let underline = if case % 3 == 0 { " u=\"sng\"" } else { "" };
    let fill = if case % 4 == 0 {
        r#"<a:gradFill><a:gsLst><a:gs pos="0"><a:srgbClr val="4472C4"/></a:gs><a:gs pos="100000"><a:srgbClr val="FFFFFF"/></a:gs></a:gsLst><a:lin ang="5400000"/></a:gradFill>"#
    } else {
        r#"<a:solidFill><a:schemeClr val="accent1"/></a:solidFill>"#
    };
    let slide = format!(
        r#"<p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><p:cSld><p:spTree><p:sp><p:nvSpPr><p:cNvPr id="2" name="Generated case {case}" descr="wasmppt:text:case_{case:02}"/></p:nvSpPr><p:spPr><a:xfrm rot="{}"><a:off x="914400" y="685800"/><a:ext cx="7315200" cy="2057400"/></a:xfrm><a:prstGeom prst="{geometry}"/>{fill}</p:spPr><p:txBody><a:bodyPr anchor="ctr"{vertical}/><a:lstStyle/><a:p><a:pPr algn="ctr"{direction}/><a:r><a:rPr sz="2400"{} spc="{}"/><a:t>Compatibility case {case}: 한국어 العربية 漢字</a:t></a:r></a:p></p:txBody></p:sp></p:spTree></p:cSld></p:sld>"#,
        (case as i32 % 9) * 300_000,
        underline,
        case * 10,
    );
    let custom = format!(
        r#"<Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/custom-properties" xmlns:vt="http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes"><property fmtid="{{D5CDD505-2E9C-101B-9397-08002B2CF9AE}}" pid="2" name="case"><vt:i4>{case}</vt:i4></property><property fmtid="{{D5CDD505-2E9C-101B-9397-08002B2CF9AE}}" pid="3" name="generator"><vt:lpwstr>wasmppt compatibility corpus v1</vt:lpwstr></property></Properties>"#,
    );
    let entries = [
        (
            "[Content_Types].xml",
            r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/><Override PartName="/ppt/presentation.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml"/><Override PartName="/ppt/slides/slide1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slide+xml"/><Override PartName="/docProps/custom.xml" ContentType="application/vnd.openxmlformats-officedocument.custom-properties+xml"/></Types>"#,
        ),
        (
            "_rels/.rels",
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="ppt/presentation.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/custom-properties" Target="docProps/custom.xml"/></Relationships>"#,
        ),
        (
            "ppt/_rels/presentation.xml.rels",
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide" Target="slides/slide1.xml"/></Relationships>"#,
        ),
        (
            "ppt/presentation.xml",
            r#"<p:presentation xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><p:sldIdLst><p:sldId id="256" r:id="rId1"/></p:sldIdLst><p:sldSz cx="9144000" cy="5143500"/></p:presentation>"#,
        ),
        ("ppt/slides/slide1.xml", slide.as_str()),
        ("docProps/custom.xml", custom.as_str()),
    ];
    for (name, value) in entries {
        writer
            .write_entry(name, value.as_bytes(), &options)
            .expect("write fixture entry");
    }
    writer.finish().expect("finish fixture").0.into_inner()
}
