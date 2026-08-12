use std::{env, fs, io::Write, path::PathBuf};

use flate2::{Compression, write::ZlibEncoder};
use wasmppt_opc::{CompressionMethod, EntryOptions, VecSink, ZipWriter};

fn main() {
    let mut args = env::args().skip(1);
    let scenario = args
        .next()
        .expect("usage: write_benchmark_fixture SCENARIO SLIDES OUTPUT.potx");
    let slides: usize = args
        .next()
        .expect("missing slide count")
        .parse()
        .expect("invalid slide count");
    let output = PathBuf::from(args.next().expect("missing output path"));
    assert!(matches!(scenario.as_str(), "text" | "image" | "mixed"));
    assert!(matches!(slides, 10 | 50 | 200));
    assert!(args.next().is_none());
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent).unwrap();
    }

    let options = EntryOptions::deterministic(CompressionMethod::Deflate);
    let mut writer = ZipWriter::new(VecSink::new());
    writer.write_entry(
        "[Content_Types].xml",
        br#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/><Default Extension="png" ContentType="image/png"/><Override PartName="/ppt/presentation.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.template.main+xml"/></Types>"#,
        &options,
    ).unwrap();
    writer.write_entry(
        "_rels/.rels",
        br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="ppt/presentation.xml"/></Relationships>"#,
        &options,
    ).unwrap();

    let slide_ids = (0..slides)
        .map(|index| {
            format!(
                "<p:sldId id=\"{}\" r:id=\"rId{}\"/>",
                256 + index,
                index + 1
            )
        })
        .collect::<String>();
    let presentation = format!(
        "<p:presentation xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><p:sldIdLst>{slide_ids}</p:sldIdLst><p:sldSz cx=\"12192000\" cy=\"6858000\"/></p:presentation>"
    );
    writer
        .write_entry("ppt/presentation.xml", presentation.as_bytes(), &options)
        .unwrap();
    let relationships = (0..slides).map(|index| format!("<Relationship Id=\"rId{}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide\" Target=\"slides/slide{}.xml\"/>", index + 1, index + 1)).collect::<String>();
    let relationships = format!(
        "<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">{relationships}</Relationships>"
    );
    writer
        .write_entry(
            "ppt/_rels/presentation.xml.rels",
            relationships.as_bytes(),
            &options,
        )
        .unwrap();

    for index in 0..slides {
        let mut shapes = String::new();
        if scenario != "image" {
            for field in 0..8 {
                let id = format!("text_{index}_{field}");
                shapes.push_str(&format!("<p:sp><p:nvSpPr><p:cNvPr id=\"{}\" name=\"Text {field}\"/></p:nvSpPr><p:spPr><a:xfrm><a:off x=\"500000\" y=\"{}\"/><a:ext cx=\"10500000\" cy=\"500000\"/></a:xfrm><a:prstGeom prst=\"rect\"/></p:spPr><p:txBody><a:p><a:r><a:t>{{{{{id}}}}}</a:t></a:r></a:p></p:txBody></p:sp>", field + 2, 300000 + field * 650000));
            }
        }
        if scenario != "text" {
            let id = format!("image_{index}");
            shapes.push_str(&format!("<p:pic><p:nvPicPr><p:cNvPr id=\"100\" name=\"Image\" descr=\"wasmppt:image:{id}\"/></p:nvPicPr><p:spPr><a:xfrm><a:off x=\"6500000\" y=\"1000000\"/><a:ext cx=\"4500000\" cy=\"4500000\"/></a:xfrm></p:spPr><p:blipFill><a:blip r:embed=\"rImg\"/><a:srcRect l=\"0\" t=\"0\" r=\"0\" b=\"0\"/></p:blipFill></p:pic>"));
        }
        let slide = format!(
            "<p:sld xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\" xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><p:cSld><p:spTree>{shapes}</p:spTree></p:cSld></p:sld>"
        );
        writer
            .write_entry(
                &format!("ppt/slides/slide{}.xml", index + 1),
                slide.as_bytes(),
                &options,
            )
            .unwrap();
        if scenario != "text" {
            let rels = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rImg" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="../media/original.png"/></Relationships>"#;
            writer
                .write_entry(
                    &format!("ppt/slides/_rels/slide{}.xml.rels", index + 1),
                    rels,
                    &options,
                )
                .unwrap();
        }
    }
    if scenario != "text" {
        writer
            .write_entry("ppt/media/original.png", &image_bytes(64 * 1024), &options)
            .unwrap();
    }
    fs::write(output, writer.finish().unwrap().0.into_inner()).unwrap();
}

fn image_bytes(size: usize) -> Vec<u8> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(&[0, 0, 0, 0, 0]).unwrap();
    let compressed = encoder.finish().unwrap();
    let fixed = 8 + (12 + 13) + 12 + (12 + compressed.len()) + 12;
    assert!(size >= fixed);
    let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
    png_chunk(
        &mut bytes,
        b"IHDR",
        &[0, 0, 0, 1, 0, 0, 0, 1, 8, 6, 0, 0, 0],
    );
    let padding = (0..size - fixed)
        .map(|index| (index % 251) as u8)
        .collect::<Vec<_>>();
    png_chunk(&mut bytes, b"vpAg", &padding);
    png_chunk(&mut bytes, b"IDAT", &compressed);
    png_chunk(&mut bytes, b"IEND", &[]);
    assert_eq!(bytes.len(), size);
    bytes
}

fn png_chunk(output: &mut Vec<u8>, kind: &[u8; 4], payload: &[u8]) {
    output.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    output.extend_from_slice(kind);
    output.extend_from_slice(payload);
    let mut crc = crc32fast::Hasher::new();
    crc.update(kind);
    crc.update(payload);
    output.extend_from_slice(&crc.finalize().to_be_bytes());
}
