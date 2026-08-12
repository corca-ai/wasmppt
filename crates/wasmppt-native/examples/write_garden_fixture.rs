use std::{env, fs, path::PathBuf};

use wasmppt_opc::{CompressionMethod, EntryOptions, VecSink, ZipWriter};

fn main() {
    let output = env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .expect("usage: write_garden_fixture OUTPUT.potx");
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
            br#"<p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><p:cSld><p:bg><p:bgPr><a:solidFill><a:srgbClr val="F5F0E6"/></a:solidFill></p:bgPr></p:bg><p:spTree><p:sp><p:nvSpPr><p:cNvPr id="2" name="Navy panel"/></p:nvSpPr><p:spPr><a:xfrm><a:off x="7900000" y="0"/><a:ext cx="4292000" cy="6858000"/></a:xfrm><a:prstGeom prst="rect"/><a:solidFill><a:srgbClr val="10233D"/></a:solidFill><a:ln><a:noFill/></a:ln></p:spPr></p:sp><p:sp><p:nvSpPr><p:cNvPr id="3" name="Coral accent"/></p:nvSpPr><p:spPr><a:xfrm><a:off x="7300000" y="650000"/><a:ext cx="1350000" cy="1350000"/></a:xfrm><a:prstGeom prst="ellipse"/><a:solidFill><a:srgbClr val="FF6B4A"/></a:solidFill><a:ln><a:noFill/></a:ln></p:spPr></p:sp><p:sp><p:nvSpPr><p:cNvPr id="4" name="Garden label"/></p:nvSpPr><p:spPr><a:xfrm><a:off x="650000" y="650000"/><a:ext cx="3600000" cy="450000"/></a:xfrm><a:prstGeom prst="rect"/><a:noFill/></p:spPr><p:txBody><a:bodyPr/><a:p><a:r><a:rPr lang="en-US" sz="1100" b="1"><a:solidFill><a:srgbClr val="FF6B4A"/></a:solidFill></a:rPr><a:t>PARALLEL TEMPLATE GARDEN</a:t></a:r></a:p></p:txBody></p:sp><p:sp><p:nvSpPr><p:cNvPr id="5" name="Garden title" descr="wasmppt:text:title"/></p:nvSpPr><p:spPr><a:xfrm><a:off x="650000" y="1450000"/><a:ext cx="6650000" cy="2200000"/></a:xfrm><a:prstGeom prst="rect"/><a:noFill/></p:spPr><p:txBody><a:bodyPr anchor="ctr"/><a:p><a:r><a:rPr lang="ko-KR" sz="3900" b="1"><a:solidFill><a:srgbClr val="10233D"/></a:solidFill></a:rPr><a:t>One story, another garden</a:t></a:r></a:p></p:txBody></p:sp><p:sp><p:nvSpPr><p:cNvPr id="6" name="Garden subtitle"/></p:nvSpPr><p:spPr><a:xfrm><a:off x="650000" y="4050000"/><a:ext cx="6200000" cy="1200000"/></a:xfrm><a:prstGeom prst="rect"/><a:noFill/></p:spPr><p:txBody><a:bodyPr/><a:p><a:r><a:rPr lang="ko-KR" sz="1700"><a:solidFill><a:srgbClr val="526171"/></a:solidFill></a:rPr><a:t>{{subtitle}}</a:t></a:r></a:p></p:txBody></p:sp><p:pic><p:nvPicPr><p:cNvPr id="7" name="Garden artwork" descr="wasmppt:image:hero"/></p:nvPicPr><p:spPr><a:xfrm><a:off x="8450000" y="1550000"/><a:ext cx="3000000" cy="3850000"/></a:xfrm></p:spPr><p:blipFill><a:blip r:embed="rImg"/><a:srcRect l="0" t="0" r="0" b="0"/></p:blipFill></p:pic></p:spTree></p:cSld></p:sld>"#,
        ),
        (
            "ppt/slides/_rels/slide1.xml.rels",
            br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rImg" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="../media/original.png"/></Relationships>"#,
        ),
        (
            "ppt/slides/slide2.xml",
            br#"<p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
  <p:cSld>
    <p:bg><p:bgPr><a:solidFill><a:srgbClr val="10233D"/></a:solidFill></p:bgPr></p:bg>
    <p:spTree>
      <p:sp>
        <p:nvSpPr><p:cNvPr id="2" name="Metric eyebrow"/></p:nvSpPr>
        <p:spPr><a:xfrm><a:off x="800000" y="850000"/><a:ext cx="5000000" cy="650000"/></a:xfrm><a:prstGeom prst="rect"/><a:noFill/></p:spPr>
        <p:txBody><a:bodyPr/><a:p><a:r><a:rPr sz="1200" b="1"><a:solidFill><a:srgbClr val="FF6B4A"/></a:solidFill></a:rPr><a:t>LIVE METRIC</a:t></a:r></a:p></p:txBody>
      </p:sp>
      <p:graphicFrame>
        <p:nvGraphicFramePr><p:cNvPr id="3" name="Metrics table"/></p:nvGraphicFramePr>
        <p:xfrm><a:off x="800000" y="1800000"/><a:ext cx="6500000" cy="3000000"/></p:xfrm>
        <a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/table"><a:tbl>
          <a:tblGrid><a:gridCol w="3250000"/><a:gridCol w="3250000"/></a:tblGrid>
          <a:tr h="3000000">
            <a:tc><a:txBody><a:bodyPr/><a:p><a:r><a:rPr sz="2100"><a:solidFill><a:srgbClr val="10233D"/></a:solidFill></a:rPr><a:t>{{metrics.label}}</a:t></a:r></a:p></a:txBody><a:tcPr><a:solidFill><a:srgbClr val="F5F0E6"/></a:solidFill></a:tcPr></a:tc>
            <a:tc><a:txBody><a:bodyPr/><a:p><a:r><a:rPr sz="3000" b="1"><a:solidFill><a:srgbClr val="FFFFFF"/></a:solidFill></a:rPr><a:t>{{metrics.value}}</a:t></a:r></a:p></a:txBody><a:tcPr><a:solidFill><a:srgbClr val="FF6B4A"/></a:solidFill></a:tcPr></a:tc>
          </a:tr>
        </a:tbl></a:graphicData></a:graphic>
      </p:graphicFrame>
      <p:sp>
        <p:nvSpPr><p:cNvPr id="4" name="Metric line"/></p:nvSpPr>
        <p:spPr><a:xfrm><a:off x="8700000" y="900000"/><a:ext cx="2600000" cy="5000000"/></a:xfrm><a:prstGeom prst="roundRect"/><a:solidFill><a:srgbClr val="FF6B4A"/></a:solidFill><a:ln><a:noFill/></a:ln></p:spPr>
      </p:sp>
    </p:spTree>
  </p:cSld>
</p:sld>"#,
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
            b"\x89PNG\r\n\x1a\nwasmppt-garden-placeholder",
            &EntryOptions::deterministic(CompressionMethod::Stored),
        )
        .expect("write placeholder image");
    fs::write(output, writer.finish().unwrap().0.into_inner()).expect("write fixture");
}
