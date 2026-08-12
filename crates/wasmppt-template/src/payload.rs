use std::collections::BTreeMap;

use crate::{
    ChartData, ChartSeriesData, ImageCrop, ImageData, ImageFitPolicy, InjectionData,
    RichTextRunData, SemanticShapeData,
};

pub const INJECTION_SCHEMA_VERSION: u32 = 2;

const MAGIC: &[u8; 4] = b"WPPD";
const MAX_COLLECTION_ITEMS: usize = 100_000;
const MAX_STRING_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InjectionDecodeError {
    message: String,
}

impl InjectionDecodeError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for InjectionDecodeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for InjectionDecodeError {}

impl InjectionData {
    /// Decode the host-neutral versioned injection payload used by Wasm adapters.
    pub fn decode(bytes: &[u8]) -> Result<Self, InjectionDecodeError> {
        Reader::new(bytes).injection_data()
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    cursor: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, cursor: 0 }
    }

    fn injection_data(mut self) -> Result<InjectionData, InjectionDecodeError> {
        if self.take(MAGIC.len())? != MAGIC {
            return Err(InjectionDecodeError::new("invalid injection payload magic"));
        }
        let schema_version = self.u32()?;
        if schema_version != 1 && schema_version != INJECTION_SCHEMA_VERSION {
            return Err(InjectionDecodeError::new(
                "unsupported injection payload schema",
            ));
        }

        let mut data = InjectionData::new();
        for _ in 0..self.count("text bindings")? {
            data.insert_text(self.string()?, self.string()?);
        }
        for _ in 0..self.count("image bindings")? {
            let id = self.string()?;
            let extension = self.string()?;
            let content_type = self.string()?;
            let crop = match self.byte()? {
                0 => None,
                1 => Some(ImageCrop {
                    left: self.i32()?,
                    top: self.i32()?,
                    right: self.i32()?,
                    bottom: self.i32()?,
                }),
                _ => return Err(InjectionDecodeError::new("invalid image crop marker")),
            };
            let bytes = self.byte_vec()?;
            let fit = if schema_version >= 2 {
                match self.byte()? {
                    0 => ImageFitPolicy::Preserve,
                    1 => ImageFitPolicy::Cover,
                    2 => ImageFitPolicy::Contain,
                    _ => return Err(InjectionDecodeError::new("invalid image fit policy")),
                }
            } else {
                ImageFitPolicy::Preserve
            };
            data.insert_image(
                id,
                ImageData {
                    bytes,
                    extension,
                    content_type,
                    crop,
                    fit,
                },
            );
        }
        for _ in 0..self.count("table bindings")? {
            let id = self.string()?;
            let mut rows = Vec::with_capacity(self.peek_count("table rows")?);
            for _ in 0..self.count("table rows")? {
                let mut row = BTreeMap::new();
                for _ in 0..self.count("table fields")? {
                    row.insert(self.string()?, self.string()?);
                }
                rows.push(row);
            }
            data.set_table_rows(id, rows);
        }
        for _ in 0..self.count("slide copy bindings")? {
            let part_name = self.string()?;
            data.set_slide_copies(part_name, self.u32()? as usize);
        }
        for _ in 0..self.count("chart bindings")? {
            let part_name = self.string()?;
            let mut categories = Vec::with_capacity(self.peek_count("chart categories")?);
            for _ in 0..self.count("chart categories")? {
                categories.push(self.string()?);
            }
            let mut series = Vec::with_capacity(self.peek_count("chart series")?);
            for _ in 0..self.count("chart series")? {
                let name = self.string()?;
                let mut values = Vec::with_capacity(self.peek_count("chart values")?);
                for _ in 0..self.count("chart values")? {
                    values.push(self.f64()?);
                }
                series.push(ChartSeriesData { name, values });
            }
            data.set_chart(part_name, ChartData { categories, series });
        }
        if schema_version >= 2 {
            for _ in 0..self.count("semantic shape bindings")? {
                let id = self.string()?;
                let visible = self.optional_bool("shape visibility")?;
                let copies = self.optional_u32("shape copies")?;
                let rich_text = match self.byte()? {
                    0 => None,
                    1 => {
                        let mut runs = Vec::with_capacity(self.peek_count("rich text runs")?);
                        for _ in 0..self.count("rich text runs")? {
                            runs.push(RichTextRunData {
                                text: self.string()?,
                                bold: self.optional_bool("rich text bold")?,
                                italic: self.optional_bool("rich text italic")?,
                                underline: self.optional_bool("rich text underline")?,
                                font_size: self.optional_i32("rich text font size")?,
                                color: self.optional_string("rich text color")?,
                            });
                        }
                        Some(runs)
                    }
                    _ => return Err(InjectionDecodeError::new("invalid rich text marker")),
                };
                let hyperlink = self.optional_string("shape hyperlink")?;
                let fill_color = self.optional_string("shape fill color")?;
                data.set_semantic_shape(
                    id,
                    SemanticShapeData {
                        visible,
                        copies,
                        rich_text,
                        hyperlink,
                        fill_color,
                    },
                );
            }
            for _ in 0..self.count("notes bindings")? {
                data.set_notes(self.string()?, self.string()?);
            }
        }
        if self.cursor != self.bytes.len() {
            return Err(InjectionDecodeError::new(
                "injection payload contains trailing bytes",
            ));
        }
        Ok(data)
    }

    fn peek_count(&self, label: &str) -> Result<usize, InjectionDecodeError> {
        let bytes = self
            .bytes
            .get(self.cursor..self.cursor + 4)
            .ok_or_else(|| InjectionDecodeError::new(format!("truncated {label} count")))?;
        let value = u32::from_le_bytes(bytes.try_into().expect("count is four bytes")) as usize;
        validate_count(label, value)
    }

    fn count(&mut self, label: &str) -> Result<usize, InjectionDecodeError> {
        validate_count(label, self.u32()? as usize)
    }

    fn string(&mut self) -> Result<String, InjectionDecodeError> {
        let length = self.u32()? as usize;
        if length > MAX_STRING_BYTES {
            return Err(InjectionDecodeError::new("injection string is too large"));
        }
        let bytes = self.take(length)?;
        std::str::from_utf8(bytes)
            .map(str::to_owned)
            .map_err(|_| InjectionDecodeError::new("injection string is not UTF-8"))
    }

    fn byte_vec(&mut self) -> Result<Vec<u8>, InjectionDecodeError> {
        let length = self.u32()? as usize;
        Ok(self.take(length)?.to_vec())
    }

    fn byte(&mut self) -> Result<u8, InjectionDecodeError> {
        Ok(self.take(1)?[0])
    }

    fn i32(&mut self) -> Result<i32, InjectionDecodeError> {
        Ok(i32::from_le_bytes(
            self.take(4)?.try_into().expect("i32 is four bytes"),
        ))
    }

    fn optional_bool(&mut self, label: &str) -> Result<Option<bool>, InjectionDecodeError> {
        match self.byte()? {
            0 => Ok(None),
            1 => Ok(Some(false)),
            2 => Ok(Some(true)),
            _ => Err(InjectionDecodeError::new(format!("invalid {label} marker"))),
        }
    }

    fn optional_u32(&mut self, label: &str) -> Result<Option<u32>, InjectionDecodeError> {
        match self.byte()? {
            0 => Ok(None),
            1 => Ok(Some(self.u32()?)),
            _ => Err(InjectionDecodeError::new(format!("invalid {label} marker"))),
        }
    }

    fn optional_i32(&mut self, label: &str) -> Result<Option<i32>, InjectionDecodeError> {
        match self.byte()? {
            0 => Ok(None),
            1 => Ok(Some(self.i32()?)),
            _ => Err(InjectionDecodeError::new(format!("invalid {label} marker"))),
        }
    }

    fn optional_string(&mut self, label: &str) -> Result<Option<String>, InjectionDecodeError> {
        match self.byte()? {
            0 => Ok(None),
            1 => Ok(Some(self.string()?)),
            _ => Err(InjectionDecodeError::new(format!("invalid {label} marker"))),
        }
    }

    fn u32(&mut self) -> Result<u32, InjectionDecodeError> {
        Ok(u32::from_le_bytes(
            self.take(4)?.try_into().expect("u32 is four bytes"),
        ))
    }

    fn f64(&mut self) -> Result<f64, InjectionDecodeError> {
        Ok(f64::from_le_bytes(
            self.take(8)?.try_into().expect("f64 is eight bytes"),
        ))
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], InjectionDecodeError> {
        let end = self
            .cursor
            .checked_add(length)
            .ok_or_else(|| InjectionDecodeError::new("injection payload range overflows"))?;
        let bytes = self
            .bytes
            .get(self.cursor..end)
            .ok_or_else(|| InjectionDecodeError::new("truncated injection payload"))?;
        self.cursor = end;
        Ok(bytes)
    }
}

fn validate_count(label: &str, value: usize) -> Result<usize, InjectionDecodeError> {
    if value > MAX_COLLECTION_ITEMS {
        return Err(InjectionDecodeError::new(format!(
            "{label} exceeds {MAX_COLLECTION_ITEMS} items"
        )));
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_every_injection_kind() {
        let mut bytes = Vec::from(MAGIC.as_slice());
        put_u32(&mut bytes, INJECTION_SCHEMA_VERSION);
        put_u32(&mut bytes, 1);
        put_string(&mut bytes, "title");
        put_string(&mut bytes, "분기 보고서");
        put_u32(&mut bytes, 1);
        put_string(&mut bytes, "hero");
        put_string(&mut bytes, "png");
        put_string(&mut bytes, "image/png");
        bytes.push(1);
        for value in [1i32, 2, 3, 4] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        put_bytes(&mut bytes, &[1, 2, 3]);
        bytes.push(0);
        put_u32(&mut bytes, 1);
        put_string(&mut bytes, "revenue");
        put_u32(&mut bytes, 1);
        put_u32(&mut bytes, 1);
        put_string(&mut bytes, "region");
        put_string(&mut bytes, "서울");
        put_u32(&mut bytes, 1);
        put_string(&mut bytes, "ppt/slides/slide2.xml");
        put_u32(&mut bytes, 3);
        put_u32(&mut bytes, 1);
        put_string(&mut bytes, "ppt/charts/chart1.xml");
        put_u32(&mut bytes, 2);
        put_string(&mut bytes, "Q1");
        put_string(&mut bytes, "Q2");
        put_u32(&mut bytes, 1);
        put_string(&mut bytes, "Sales");
        put_u32(&mut bytes, 2);
        bytes.extend_from_slice(&1.5f64.to_le_bytes());
        bytes.extend_from_slice(&2.5f64.to_le_bytes());
        put_u32(&mut bytes, 0);
        put_u32(&mut bytes, 0);

        let golden_hex = bytes
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        assert_eq!(golden_hex, GOLDEN_HEX);

        let decoded = InjectionData::decode(&bytes).expect("payload decodes");
        let mut expected = InjectionData::new();
        expected.insert_text("title", "분기 보고서");
        expected.insert_image(
            "hero",
            ImageData {
                bytes: vec![1, 2, 3],
                extension: "png".to_owned(),
                content_type: "image/png".to_owned(),
                crop: Some(ImageCrop {
                    left: 1,
                    top: 2,
                    right: 3,
                    bottom: 4,
                }),
                fit: ImageFitPolicy::Preserve,
            },
        );
        expected.set_table_rows(
            "revenue",
            vec![BTreeMap::from([("region".to_owned(), "서울".to_owned())])],
        );
        expected.set_slide_copies("ppt/slides/slide2.xml", 3);
        expected.set_chart(
            "ppt/charts/chart1.xml",
            ChartData {
                categories: vec!["Q1".to_owned(), "Q2".to_owned()],
                series: vec![ChartSeriesData {
                    name: "Sales".to_owned(),
                    values: vec![1.5, 2.5],
                }],
            },
        );
        assert_eq!(decoded, expected);
    }

    #[test]
    fn rejects_truncation_and_trailing_bytes() {
        assert!(InjectionData::decode(b"WPPD").is_err());
        let mut empty = Vec::from(MAGIC.as_slice());
        put_u32(&mut empty, INJECTION_SCHEMA_VERSION);
        for _ in 0..7 {
            put_u32(&mut empty, 0);
        }
        assert!(InjectionData::decode(&empty).is_ok());
        empty.push(0);
        assert!(InjectionData::decode(&empty).is_err());
    }

    #[test]
    fn decodes_legacy_v1_payloads() {
        let mut bytes = Vec::from(MAGIC.as_slice());
        put_u32(&mut bytes, 1);
        for _ in 0..5 {
            put_u32(&mut bytes, 0);
        }
        assert_eq!(InjectionData::decode(&bytes).unwrap(), InjectionData::new());
    }

    fn put_u32(bytes: &mut Vec<u8>, value: u32) {
        bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn put_string(bytes: &mut Vec<u8>, value: &str) {
        put_bytes(bytes, value.as_bytes());
    }

    fn put_bytes(bytes: &mut Vec<u8>, value: &[u8]) {
        put_u32(bytes, value.len() as u32);
        bytes.extend_from_slice(value);
    }

    const GOLDEN_HEX: &str = "575050440200000001000000050000007469746c6510000000ebb684eab8b020ebb3b4eab3a0ec849c01000000040000006865726f03000000706e6709000000696d6167652f706e67010100000002000000030000000400000003000000010203000100000007000000726576656e7565010000000100000006000000726567696f6e06000000ec849cec9ab801000000150000007070742f736c696465732f736c696465322e786d6c0300000001000000150000007070742f6368617274732f6368617274312e786d6c02000000020000005131020000005132010000000500000053616c657302000000000000000000f83f00000000000004400000000000000000";
}
