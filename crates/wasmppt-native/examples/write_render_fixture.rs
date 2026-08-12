use std::{env, fs, path::PathBuf};

use wasmppt_opc::{CompressionMethod, EntryOptions, VecSink, ZipWriter};

fn main() {
    let output = env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .expect("usage: write_render_fixture OUTPUT.pptx");
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent).expect("create fixture directory");
    }
    let options = EntryOptions::deterministic(CompressionMethod::Deflate);
    let mut writer = ZipWriter::new(VecSink::new());
    let entries: [(&str, &[u8]); 16] = [
        (
            "[Content_Types].xml",
            br#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/><Default Extension="png" ContentType="image/png"/><Override PartName="/ppt/presentation.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml"/></Types>"#,
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
            br#"<p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><p:cSld><p:spTree><p:grpSp><p:nvGrpSpPr><p:cNvPr id="10" name="Group"/></p:nvGrpSpPr><p:grpSpPr><a:xfrm rot="60000"><a:off x="100" y="200"/><a:ext cx="4000000" cy="2000000"/><a:chOff x="0" y="0"/><a:chExt cx="2000000" cy="1000000"/></a:xfrm></p:grpSpPr><p:sp><p:nvSpPr><p:cNvPr id="2" name="Title" descr="Quarterly report title"><a:hlinkClick r:id="rLink"/></p:cNvPr><p:nvPr><p:ph type="title" idx="1"/></p:nvPr></p:nvSpPr><p:spPr><a:prstGeom prst="rect"/></p:spPr><p:txBody><a:p><a:r><a:t>Actual title</a:t></a:r></a:p></p:txBody></p:sp></p:grpSp><p:pic><p:nvPicPr><p:cNvPr id="3" name="Photo" descr="Quarterly report photo"/></p:nvPicPr><p:spPr><a:xfrm><a:off x="5000000" y="1000000"/><a:ext cx="2000000" cy="2000000"/></a:xfrm></p:spPr><p:blipFill><a:blip r:embed="rImg"/><a:srcRect l="1000" t="2000" r="3000" b="4000"/></p:blipFill></p:pic><p:sp><p:nvSpPr><p:cNvPr id="4" name="Custom"/></p:nvSpPr><p:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="10" cy="10"/></a:xfrm><a:custGeom/></p:spPr></p:sp><p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id="5" name="Chart"/></p:nvGraphicFramePr></p:graphicFrame></p:spTree></p:cSld></p:sld>"#,
        ),
        (
            "ppt/slides/_rels/slide1.xml.rels",
            br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rLayout" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout" Target="../slideLayouts/slideLayout1.xml"/><Relationship Id="rImg" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="../media/image1.png"/><Relationship Id="rLink" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink" Target="https://example.com/report" TargetMode="External"/></Relationships>"#,
        ),
        (
            "ppt/slides/slide2.xml",
            br#"<p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" xmlns:dgm="http://schemas.openxmlformats.org/drawingml/2006/diagram" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><p:cSld><p:spTree><p:sp><p:nvSpPr><p:cNvPr id="2" name="Second"/></p:nvSpPr><p:spPr><a:xfrm><a:off x="1" y="2"/><a:ext cx="3" cy="4"/></a:xfrm><a:prstGeom prst="ellipse"/><a:solidFill><a:schemeClr val="accent1"/></a:solidFill><a:sp3d/></p:spPr></p:sp><p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id="6" name="Sales Table" descr="Quarterly sales table"/></p:nvGraphicFramePr><p:xfrm><a:off x="500000" y="500000"/><a:ext cx="5000000" cy="2500000"/></p:xfrm><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/table"><a:tbl><a:tblGrid><a:gridCol w="2500000"/><a:gridCol w="2500000"/></a:tblGrid><a:tr h="1000000"><a:tc><a:txBody><a:p><a:r><a:t>Quarter</a:t></a:r></a:p></a:txBody><a:tcPr><a:solidFill><a:srgbClr val="D9EAF7"/></a:solidFill></a:tcPr></a:tc><a:tc><a:txBody><a:p><a:r><a:t>Sales</a:t></a:r></a:p></a:txBody></a:tc></a:tr><a:tr h="1500000"><a:tc><a:txBody><a:p><a:r><a:t>Q1</a:t></a:r></a:p></a:txBody></a:tc><a:tc><a:txBody><a:p><a:r><a:t>42</a:t></a:r></a:p></a:txBody></a:tc></a:tr></a:tbl></a:graphicData></a:graphic></p:graphicFrame><p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id="7" name="Sales Chart" descr="Quarterly sales chart"/></p:nvGraphicFramePr><p:xfrm><a:off x="6000000" y="500000"/><a:ext cx="5500000" cy="4000000"/></p:xfrm><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/chart"><c:chart r:id="rChart"/></a:graphicData></a:graphic></p:graphicFrame><p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id="8" name="SmartArt"/></p:nvGraphicFramePr><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/diagram"><dgm:relIds r:dm="rDiagram"/></a:graphicData></a:graphic></p:graphicFrame><p:pic><p:nvPicPr><p:cNvPr id="9" name="Metafile"/></p:nvPicPr><p:spPr><a:xfrm><a:off x="8500000" y="4500000"/><a:ext cx="2500000" cy="1800000"/></a:xfrm></p:spPr><p:blipFill><a:blip r:embed="rEmf"/></p:blipFill></p:pic></p:spTree></p:cSld><p:transition/><p:timing><p:tnLst/></p:timing></p:sld>"#,
        ),
        (
            "ppt/slides/_rels/slide2.xml.rels",
            br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rLayout" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout" Target="../slideLayouts/slideLayout2.xml"/><Relationship Id="rChart" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/chart" Target="../charts/chart1.xml"/><Relationship Id="rEmf" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="../media/preview.emf"/></Relationships>"#,
        ),
        (
            "ppt/slideLayouts/slideLayout1.xml",
            br#"<p:sldLayout xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><p:cSld><p:spTree><p:sp><p:nvSpPr><p:cNvPr id="20" name="Layout decoration"/></p:nvSpPr><p:spPr><a:xfrm><a:off x="10" y="20"/><a:ext cx="30" cy="40"/></a:xfrm><a:prstGeom prst="diamond"/><a:solidFill><a:srgbClr val="010203"/></a:solidFill></p:spPr></p:sp><p:sp><p:nvSpPr><p:cNvPr id="21" name="Title placeholder"/><p:nvPr><p:ph type="title" idx="1"/></p:nvPr></p:nvSpPr><p:spPr><a:xfrm><a:off x="1000000" y="500000"/><a:ext cx="8000000" cy="1000000"/></a:xfrm><a:prstGeom prst="rect"/><a:solidFill><a:schemeClr val="accent1"><a:tint val="20000"/></a:schemeClr></a:solidFill><a:ln w="12700"><a:solidFill><a:srgbClr val="000000"/></a:solidFill></a:ln></p:spPr></p:sp></p:spTree></p:cSld></p:sldLayout>"#,
        ),
        (
            "ppt/slideLayouts/_rels/slideLayout1.xml.rels",
            br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rMaster" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster" Target="../slideMasters/slideMaster1.xml"/></Relationships>"#,
        ),
        (
            "ppt/slideLayouts/slideLayout2.xml",
            br#"<p:sldLayout xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"><p:cSld><p:spTree/></p:cSld></p:sldLayout>"#,
        ),
        (
            "ppt/slideLayouts/_rels/slideLayout2.xml.rels",
            br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rMaster" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster" Target="../slideMasters/slideMaster2.xml"/></Relationships>"#,
        ),
        (
            "ppt/slideMasters/slideMaster1.xml",
            br#"<p:sldMaster xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><p:cSld><p:bg><p:bgPr><a:solidFill><a:schemeClr val="bg1"/></a:solidFill></p:bgPr></p:bg><p:spTree><p:sp><p:nvSpPr><p:cNvPr id="30" name="Master decoration"/></p:nvSpPr><p:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="100" cy="100"/></a:xfrm><a:prstGeom prst="rect"/><a:solidFill><a:schemeClr val="accent1"><a:shade val="50000"/></a:schemeClr></a:solidFill></p:spPr></p:sp></p:spTree></p:cSld><p:clrMap accent1="accent1" bg1="lt1" tx1="dk1"/></p:sldMaster>"#,
        ),
        (
            "ppt/slideMasters/_rels/slideMaster1.xml.rels",
            br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rTheme" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme" Target="../theme/theme1.xml"/></Relationships>"#,
        ),
        (
            "ppt/slideMasters/slideMaster2.xml",
            br#"<p:sldMaster xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"><p:cSld><p:spTree/></p:cSld></p:sldMaster>"#,
        ),
        (
            "ppt/slideMasters/_rels/slideMaster2.xml.rels",
            br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rTheme" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme" Target="../theme/theme2.xml"/></Relationships>"#,
        ),
    ];
    for (name, bytes) in entries {
        writer
            .write_entry(name, bytes, &options)
            .expect("write fixture entry");
    }
    writer
        .write_entry(
            "ppt/theme/theme1.xml",
            br#"<a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><a:themeElements><a:clrScheme name="One"><a:dk1><a:srgbClr val="000000"/></a:dk1><a:lt1><a:srgbClr val="FFFFFF"/></a:lt1><a:accent1><a:srgbClr val="336699"/></a:accent1></a:clrScheme></a:themeElements></a:theme>"#,
            &options,
        )
        .expect("write theme one");
    writer
        .write_entry(
            "ppt/theme/theme2.xml",
            br#"<a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><a:themeElements><a:clrScheme name="Two"><a:accent1><a:srgbClr val="AA5500"/></a:accent1></a:clrScheme></a:themeElements></a:theme>"#,
            &options,
        )
        .expect("write theme two");
    writer
        .write_entry("ppt/media/image1.png", b"fixture image bytes", &options)
        .expect("write image");
    writer
        .write_entry(
            "ppt/charts/chart1.xml",
            br#"<c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><c:chart><c:plotArea><c:barChart><c:barDir val="col"/><c:ser><c:tx><c:strRef><c:f>Sheet1!$B$1</c:f><c:strCache><c:ptCount val="1"/><c:pt idx="0"><c:v>Sales</c:v></c:pt></c:strCache></c:strRef></c:tx><c:cat><c:strRef><c:f>Sheet1!$A$2:$A$4</c:f><c:strCache><c:ptCount val="3"/><c:pt idx="0"><c:v>Q1</c:v></c:pt><c:pt idx="1"><c:v>Q2</c:v></c:pt><c:pt idx="2"><c:v>Q3</c:v></c:pt></c:strCache></c:strRef></c:cat><c:val><c:numRef><c:f>Sheet1!$B$2:$B$4</c:f><c:numCache><c:formatCode>General</c:formatCode><c:ptCount val="3"/><c:pt idx="0"><c:v>42</c:v></c:pt><c:pt idx="1"><c:v>64</c:v></c:pt><c:pt idx="2"><c:v>53</c:v></c:pt></c:numCache></c:numRef></c:val></c:ser></c:barChart></c:plotArea></c:chart><c:externalData r:id="rWorkbook"/></c:chartSpace>"#,
            &options,
        )
        .expect("write chart");
    writer
        .write_entry(
            "ppt/charts/_rels/chart1.xml.rels",
            br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rWorkbook" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/package" Target="../embeddings/sales.xlsx"/></Relationships>"#,
            &options,
        )
        .expect("write chart relationships");
    writer
        .write_entry("ppt/embeddings/sales.xlsx", &embedded_workbook(), &options)
        .expect("write embedded workbook");
    writer
        .write_entry("ppt/media/preview.emf", &preview_emf(), &options)
        .expect("write metafile");
    let bytes = writer.finish().expect("finish fixture").0.into_inner();
    fs::write(output, bytes).expect("write fixture");
}

fn preview_emf() -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&1_u32.to_le_bytes());
    bytes.extend_from_slice(&88_u32.to_le_bytes());
    for value in [0_i32, 0, 200, 120, 0, 0, 200, 120] {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes.extend_from_slice(&0x464D_4520_u32.to_le_bytes());
    bytes.extend_from_slice(&0x0001_0000_u32.to_le_bytes());
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&0_u16.to_le_bytes());
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    for value in [100_u32, 100, 100, 100] {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes.extend_from_slice(&emf_record(0x2B, &[10, 10, 190, 110]));
    bytes.extend_from_slice(&emf_record(0x0E, &[0, 0]));
    let total = u32::try_from(bytes.len()).expect("fixture EMF fits u32");
    bytes[48..52].copy_from_slice(&total.to_le_bytes());
    bytes
}

fn emf_record(record_type: u32, params: &[i32]) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&record_type.to_le_bytes());
    bytes.extend_from_slice(
        &u32::try_from(8 + params.len() * 4)
            .expect("fixture record fits u32")
            .to_le_bytes(),
    );
    for param in params {
        bytes.extend_from_slice(&param.to_le_bytes());
    }
    bytes
}

fn embedded_workbook() -> Vec<u8> {
    let options = EntryOptions::deterministic(CompressionMethod::Deflate);
    let mut writer = ZipWriter::new(VecSink::new());
    writer
        .write_entry(
            "[Content_Types].xml",
            br#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/></Types>"#,
            &options,
        )
        .unwrap();
    writer
        .write_entry(
            "xl/worksheets/sheet1.xml",
            br#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>Quarter</t></is></c><c r="B1" t="inlineStr"><is><t>Sales</t></is></c></row><row r="2"><c r="A2" t="inlineStr"><is><t>Q1</t></is></c><c r="B2"><v>42</v></c></row><row r="3"><c r="A3" t="inlineStr"><is><t>Q2</t></is></c><c r="B3"><v>64</v></c></row><row r="4"><c r="A4" t="inlineStr"><is><t>Q3</t></is></c><c r="B4"><v>53</v></c></row></sheetData></worksheet>"#,
            &options,
        )
        .unwrap();
    writer.finish().unwrap().0.into_inner()
}
