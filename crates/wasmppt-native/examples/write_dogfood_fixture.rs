use std::{env, fs, path::PathBuf};

use wasmppt_opc::{CompressionMethod, EntryOptions, VecSink, ZipWriter};

fn main() {
    let output = env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .expect("usage: write_dogfood_fixture OUTPUT.potx");
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent).expect("create fixture directory");
    }
    let options = EntryOptions::deterministic(CompressionMethod::Deflate);
    let mut writer = ZipWriter::new(VecSink::new());
    let entries: [(&str, &[u8]); 7] = [
        (
            "[Content_Types].xml",
            br#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/><Default Extension="png" ContentType="image/png"/><Override PartName="/ppt/presentation.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.template.main+xml"/><Override PartName="/ppt/slides/slide1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slide+xml"/><Override PartName="/ppt/slides/slide2.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slide+xml"/></Types>"#,
        ),
        (
            "_rels/.rels",
            br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="ppt/presentation.xml"/></Relationships>"#,
        ),
        (
            "ppt/presentation.xml",
            br#"<p:presentation xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><p:sldIdLst><p:sldId id="256" r:id="rId1"/><p:sldId id="257" r:id="rId2"/></p:sldIdLst><p:sldSz cx="12192000" cy="6858000"/></p:presentation>"#,
        ),
        (
            "ppt/_rels/presentation.xml.rels",
            br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide" Target="slides/slide1.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide" Target="slides/slide2.xml"/></Relationships>"#,
        ),
        (
            "ppt/slides/slide1.xml",
            br#"<p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><p:cSld><p:spTree><p:sp><p:nvSpPr><p:cNvPr id="2" name="Report title" descr="wasmppt:text:title"/></p:nvSpPr><p:spPr><a:xfrm><a:off x="600000" y="500000"/><a:ext cx="10800000" cy="1100000"/></a:xfrm><a:prstGeom prst="rect"/></p:spPr><p:txBody><a:p><a:r><a:rPr lang="ko-KR" sz="3200" b="1"/><a:t>Quarterly report</a:t></a:r></a:p></p:txBody></p:sp><p:sp><p:nvSpPr><p:cNvPr id="3" name="Subtitle"/></p:nvSpPr><p:spPr><a:xfrm><a:off x="600000" y="1800000"/><a:ext cx="5600000" cy="800000"/></a:xfrm><a:prstGeom prst="rect"/></p:spPr><p:txBody><a:p><a:r><a:rPr lang="ko-KR" sz="1800"/><a:t>{{subtitle}}</a:t></a:r></a:p></p:txBody></p:sp><p:pic><p:nvPicPr><p:cNvPr id="4" name="Hero image" descr="wasmppt:image:hero"/></p:nvPicPr><p:spPr><a:xfrm><a:off x="6800000" y="1800000"/><a:ext cx="4200000" cy="3600000"/></a:xfrm></p:spPr><p:blipFill><a:blip r:embed="rImg"/><a:srcRect l="0" t="0" r="0" b="0"/></p:blipFill></p:pic></p:spTree></p:cSld></p:sld>"#,
        ),
        (
            "ppt/slides/_rels/slide1.xml.rels",
            br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rImg" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="../media/original.png"/></Relationships>"#,
        ),
        (
            "ppt/slides/slide2.xml",
            br#"<p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><p:cSld><p:spTree><p:sp><p:nvSpPr><p:cNvPr id="2" name="Metrics table"/></p:nvSpPr><p:txBody><a:tbl><a:tr h="600000"><a:tc><a:p><a:r><a:t>{{metrics.label}}</a:t></a:r></a:p></a:tc><a:tc><a:p><a:r><a:t>{{metrics.value}}</a:t></a:r></a:p></a:tc></a:tr></a:tbl></p:txBody></p:sp></p:spTree></p:cSld></p:sld>"#,
        ),
    ];
    for (name, bytes) in entries {
        writer
            .write_entry(name, bytes, &options)
            .expect("write entry");
    }
    writer
        .write_entry(
            "ppt/media/original.png",
            b"\x89PNG\r\n\x1a\nwasmppt-dogfood-placeholder",
            &EntryOptions::deterministic(CompressionMethod::Stored),
        )
        .expect("write placeholder image");
    fs::write(output, writer.finish().unwrap().0.into_inner()).expect("write fixture");
}
