use std::{collections::HashSet, io::Read};

use flate2::read::DeflateDecoder;

use crate::{Error, ErrorCode, MemorySource, PackageLimits, ReadAt, Result};

const EOCD_SIGNATURE: u32 = 0x0605_4b50;
const CENTRAL_SIGNATURE: u32 = 0x0201_4b50;
const LOCAL_SIGNATURE: u32 = 0x0403_4b50;
const EOCD_FIXED: usize = 22;
const CENTRAL_FIXED: usize = 46;
const LOCAL_FIXED: usize = 30;
const MAX_EOCD_SEARCH: u64 = EOCD_FIXED as u64 + u16::MAX as u64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompressionMethod {
    Stored,
    Deflate,
    Unsupported(u16),
}

impl CompressionMethod {
    pub const fn code(self) -> u16 {
        match self {
            Self::Stored => 0,
            Self::Deflate => 8,
            Self::Unsupported(code) => code,
        }
    }
}

impl From<u16> for CompressionMethod {
    fn from(code: u16) -> Self {
        match code {
            0 => Self::Stored,
            8 => Self::Deflate,
            other => Self::Unsupported(other),
        }
    }
}

/// Indexed ZIP entry. Compressed payload bytes remain in the original source.
#[derive(Clone, Debug)]
pub struct Entry {
    pub name: String,
    pub compression: CompressionMethod,
    pub flags: u16,
    pub crc32: u32,
    pub compressed_size: u64,
    pub uncompressed_size: u64,
    pub modified_time: u16,
    pub modified_date: u16,
    pub version_made_by: u16,
    pub version_needed: u16,
    pub internal_attributes: u16,
    pub external_attributes: u32,
    pub central_extra: Vec<u8>,
    pub local_extra: Vec<u8>,
    pub comment: Vec<u8>,
    pub(crate) local_header_offset: u64,
    pub(crate) data_offset: u64,
    pub(crate) local_record_end: u64,
}

impl Entry {
    pub fn compressed_range(&self) -> std::ops::Range<u64> {
        self.data_offset..self.data_offset + self.compressed_size
    }
}

/// A lazily indexed ZIP archive backed by a random-access source.
#[derive(Debug)]
pub struct ZipArchive<S> {
    source: S,
    entries: Vec<Entry>,
    comment: Vec<u8>,
    limits: PackageLimits,
}

impl ZipArchive<MemorySource> {
    pub fn from_bytes(bytes: impl Into<std::sync::Arc<[u8]>>) -> Result<Self> {
        Self::open(MemorySource::new(bytes), PackageLimits::default())
    }

    pub fn from_bytes_with_limits(
        bytes: impl Into<std::sync::Arc<[u8]>>,
        limits: PackageLimits,
    ) -> Result<Self> {
        Self::open(MemorySource::new(bytes), limits)
    }
}

impl<S: ReadAt> ZipArchive<S> {
    pub fn open(source: S, limits: PackageLimits) -> Result<Self> {
        let source_len = source.len();
        if source_len < EOCD_FIXED as u64 {
            return Err(Error::new(ErrorCode::Truncated, "ZIP is shorter than EOCD"));
        }

        let tail_len = source_len.min(MAX_EOCD_SEARCH);
        let tail_offset = source_len - tail_len;
        let tail = read_vec(&source, tail_offset, tail_len)?;
        let eocd_in_tail = find_eocd(&tail)?;
        let eocd_offset = tail_offset + eocd_in_tail as u64;
        let eocd = &tail[eocd_in_tail..];

        let disk = le_u16(eocd, 4)?;
        let central_disk = le_u16(eocd, 6)?;
        let entries_on_disk = le_u16(eocd, 8)?;
        let entry_count = le_u16(eocd, 10)?;
        let central_size = u64::from(le_u32(eocd, 12)?);
        let central_offset = u64::from(le_u32(eocd, 16)?);
        let comment_len = usize::from(le_u16(eocd, 20)?);

        if disk != 0 || central_disk != 0 || entries_on_disk != entry_count {
            return Err(Error::new(
                ErrorCode::UnsupportedMultiDisk,
                "multi-disk ZIP packages are not supported",
            ));
        }
        if entry_count == u16::MAX
            || central_size == u64::from(u32::MAX)
            || central_offset == u64::from(u32::MAX)
        {
            return Err(Error::new(
                ErrorCode::UnsupportedZip64,
                "ZIP64 packages are not supported yet",
            ));
        }
        if usize::from(entry_count) > limits.max_entries {
            return Err(limit_error("entry count", u64::from(entry_count)));
        }
        if central_size > limits.max_central_directory_bytes {
            return Err(limit_error("central directory bytes", central_size));
        }
        if comment_len > limits.max_comment_bytes {
            return Err(limit_error("archive comment bytes", comment_len as u64));
        }
        if EOCD_FIXED + comment_len > eocd.len() {
            return Err(Error::new(
                ErrorCode::Truncated,
                "truncated archive comment",
            ));
        }
        let central_end = central_offset
            .checked_add(central_size)
            .ok_or_else(|| Error::new(ErrorCode::InvalidField, "central directory overflow"))?;
        if central_end > eocd_offset {
            return Err(Error::new(
                ErrorCode::OverlappingEntries,
                "central directory overlaps EOCD or entry data",
            ));
        }

        let central = read_vec(&source, central_offset, central_size)?;
        let mut cursor = 0usize;
        let mut entries = Vec::with_capacity(usize::from(entry_count));
        let mut names = HashSet::with_capacity(usize::from(entry_count));
        let mut total_compressed = 0u64;
        let mut total_uncompressed = 0u64;

        for _ in 0..entry_count {
            let header = central.get(cursor..cursor + CENTRAL_FIXED).ok_or_else(|| {
                Error::new(ErrorCode::Truncated, "truncated central-directory header")
            })?;
            if le_u32(header, 0)? != CENTRAL_SIGNATURE {
                return Err(Error::new(
                    ErrorCode::InvalidSignature,
                    format!("invalid central-directory signature at byte {cursor}"),
                ));
            }

            let name_len = usize::from(le_u16(header, 28)?);
            let extra_len = usize::from(le_u16(header, 30)?);
            let entry_comment_len = usize::from(le_u16(header, 32)?);
            let variable_len = name_len
                .checked_add(extra_len)
                .and_then(|value| value.checked_add(entry_comment_len))
                .ok_or_else(|| Error::new(ErrorCode::InvalidField, "entry metadata overflow"))?;
            let record_end = cursor
                .checked_add(CENTRAL_FIXED)
                .and_then(|value| value.checked_add(variable_len))
                .ok_or_else(|| Error::new(ErrorCode::InvalidField, "entry record overflow"))?;
            let record = central.get(cursor..record_end).ok_or_else(|| {
                Error::new(ErrorCode::Truncated, "truncated central-directory entry")
            })?;

            if name_len == 0 || name_len > limits.max_name_bytes {
                return Err(limit_error("entry name bytes", name_len as u64));
            }
            if extra_len > limits.max_extra_bytes {
                return Err(limit_error("entry extra bytes", extra_len as u64));
            }
            if entry_comment_len > limits.max_comment_bytes {
                return Err(limit_error("entry comment bytes", entry_comment_len as u64));
            }

            let name_start = CENTRAL_FIXED;
            let extra_start = name_start + name_len;
            let comment_start = extra_start + extra_len;
            let name = parse_name(&record[name_start..extra_start])?;
            validate_path(&name)?;
            if !names.insert(name.clone()) {
                return Err(Error::new(
                    ErrorCode::DuplicateEntry,
                    format!("duplicate ZIP entry: {name}"),
                ));
            }

            let flags = le_u16(header, 8)?;
            if flags & 1 != 0 {
                return Err(Error::new(
                    ErrorCode::UnsupportedEncryption,
                    format!("encrypted ZIP entry is not supported: {name}"),
                ));
            }
            let compressed_size = u64::from(le_u32(header, 20)?);
            let uncompressed_size = u64::from(le_u32(header, 24)?);
            let local_header_offset = u64::from(le_u32(header, 42)?);
            if compressed_size == u64::from(u32::MAX)
                || uncompressed_size == u64::from(u32::MAX)
                || local_header_offset == u64::from(u32::MAX)
            {
                return Err(Error::new(
                    ErrorCode::UnsupportedZip64,
                    format!("ZIP64 entry is not supported: {name}"),
                ));
            }
            validate_entry_limits(&limits, &name, compressed_size, uncompressed_size)?;
            total_compressed = checked_total(
                "total compressed bytes",
                total_compressed,
                compressed_size,
                limits.max_compressed_bytes,
            )?;
            total_uncompressed = checked_total(
                "total uncompressed bytes",
                total_uncompressed,
                uncompressed_size,
                limits.max_uncompressed_bytes,
            )?;

            let compression = CompressionMethod::from(le_u16(header, 10)?);
            let crc32 = le_u32(header, 16)?;
            let (data_offset, local_record_end, local_extra) = parse_local_header(
                &source,
                central_offset,
                &name,
                flags,
                compression,
                crc32,
                local_header_offset,
                compressed_size,
                uncompressed_size,
                &limits,
            )?;

            entries.push(Entry {
                name,
                compression,
                flags,
                crc32,
                compressed_size,
                uncompressed_size,
                modified_time: le_u16(header, 12)?,
                modified_date: le_u16(header, 14)?,
                version_made_by: le_u16(header, 4)?,
                version_needed: le_u16(header, 6)?,
                internal_attributes: le_u16(header, 36)?,
                external_attributes: le_u32(header, 38)?,
                central_extra: record[extra_start..comment_start].to_vec(),
                local_extra,
                comment: record[comment_start..].to_vec(),
                local_header_offset,
                data_offset,
                local_record_end,
            });
            cursor = record_end;
        }

        if cursor != central.len() {
            return Err(Error::new(
                ErrorCode::InvalidField,
                "central-directory size does not match its entry records",
            ));
        }

        validate_non_overlapping(&entries)?;
        let comment = eocd[EOCD_FIXED..EOCD_FIXED + comment_len].to_vec();
        Ok(Self {
            source,
            entries,
            comment,
            limits,
        })
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn entry(&self, name: &str) -> Option<&Entry> {
        self.entries.iter().find(|entry| entry.name == name)
    }

    pub fn comment(&self) -> &[u8] {
        &self.comment
    }

    pub fn limits(&self) -> &PackageLimits {
        &self.limits
    }

    pub fn source(&self) -> &S {
        &self.source
    }

    pub fn read_compressed(&self, entry: &Entry) -> Result<Vec<u8>> {
        read_vec(&self.source, entry.data_offset, entry.compressed_size)
    }

    pub fn read_entry(&self, entry: &Entry) -> Result<Vec<u8>> {
        let compressed = self.read_compressed(entry)?;
        let bytes = match entry.compression {
            CompressionMethod::Stored => compressed,
            CompressionMethod::Deflate => {
                let capacity = usize::try_from(entry.uncompressed_size).map_err(|_| {
                    Error::new(ErrorCode::LimitExceeded, "entry is too large for this host")
                })?;
                let mut output = Vec::with_capacity(capacity);
                DeflateDecoder::new(compressed.as_slice())
                    .take(entry.uncompressed_size.saturating_add(1))
                    .read_to_end(&mut output)
                    .map_err(|error| {
                        Error::new(
                            ErrorCode::InvalidField,
                            format!("failed to inflate {}: {error}", entry.name),
                        )
                    })?;
                output
            }
            CompressionMethod::Unsupported(code) => {
                return Err(Error::new(
                    ErrorCode::UnsupportedCompression,
                    format!("entry {} uses compression method {code}", entry.name),
                ));
            }
        };

        if bytes.len() as u64 != entry.uncompressed_size {
            return Err(Error::new(
                ErrorCode::SizeMismatch,
                format!("uncompressed size mismatch for {}", entry.name),
            ));
        }
        let actual_crc = crc32fast::hash(&bytes);
        if actual_crc != entry.crc32 {
            return Err(Error::new(
                ErrorCode::ChecksumMismatch,
                format!("CRC-32 mismatch for {}", entry.name),
            ));
        }
        Ok(bytes)
    }
}

#[allow(clippy::too_many_arguments)]
fn parse_local_header<S: ReadAt>(
    source: &S,
    central_offset: u64,
    expected_name: &str,
    expected_flags: u16,
    expected_method: CompressionMethod,
    expected_crc32: u32,
    local_offset: u64,
    compressed_size: u64,
    uncompressed_size: u64,
    limits: &PackageLimits,
) -> Result<(u64, u64, Vec<u8>)> {
    let header = read_vec(source, local_offset, LOCAL_FIXED as u64)?;
    if le_u32(&header, 0)? != LOCAL_SIGNATURE {
        return Err(Error::new(
            ErrorCode::InvalidSignature,
            format!("invalid local header for {expected_name}"),
        ));
    }
    let flags = le_u16(&header, 6)?;
    if flags & 1 != 0 || expected_flags & 1 != 0 {
        return Err(Error::new(
            ErrorCode::UnsupportedEncryption,
            format!("encrypted ZIP entry is not supported: {expected_name}"),
        ));
    }
    if flags != expected_flags || CompressionMethod::from(le_u16(&header, 8)?) != expected_method {
        return Err(Error::new(
            ErrorCode::InvalidField,
            format!("local and central metadata disagree for {expected_name}"),
        ));
    }
    if flags & (1 << 3) == 0
        && (le_u32(&header, 14)? != expected_crc32
            || u64::from(le_u32(&header, 18)?) != compressed_size
            || u64::from(le_u32(&header, 22)?) != uncompressed_size)
    {
        return Err(Error::new(
            ErrorCode::InvalidField,
            format!("local sizes or CRC disagree for {expected_name}"),
        ));
    }
    let name_len = usize::from(le_u16(&header, 26)?);
    let extra_len = usize::from(le_u16(&header, 28)?);
    if name_len == 0 || name_len > limits.max_name_bytes || extra_len > limits.max_extra_bytes {
        return Err(limit_error(
            "local entry metadata bytes",
            (name_len + extra_len) as u64,
        ));
    }
    let variable = read_vec(
        source,
        local_offset + LOCAL_FIXED as u64,
        (name_len + extra_len) as u64,
    )?;
    let local_name = parse_name(&variable[..name_len])?;
    if local_name != expected_name {
        return Err(Error::new(
            ErrorCode::InvalidField,
            format!("local entry name differs from central directory: {expected_name}"),
        ));
    }
    let data_offset = local_offset
        .checked_add(LOCAL_FIXED as u64)
        .and_then(|value| value.checked_add(name_len as u64))
        .and_then(|value| value.checked_add(extra_len as u64))
        .ok_or_else(|| Error::new(ErrorCode::InvalidField, "local data offset overflow"))?;
    let data_end = data_offset
        .checked_add(compressed_size)
        .ok_or_else(|| Error::new(ErrorCode::InvalidField, "entry data range overflow"))?;
    if data_end > central_offset {
        return Err(Error::new(
            ErrorCode::OverlappingEntries,
            format!("entry data overlaps central directory: {expected_name}"),
        ));
    }
    let local_record_end = if flags & (1 << 3) != 0 {
        parse_data_descriptor(
            source,
            central_offset,
            expected_name,
            data_end,
            expected_crc32,
            compressed_size,
            uncompressed_size,
        )?
    } else {
        data_end
    };
    Ok((data_offset, local_record_end, variable[name_len..].to_vec()))
}

#[allow(clippy::too_many_arguments)]
fn parse_data_descriptor<S: ReadAt>(
    source: &S,
    central_offset: u64,
    name: &str,
    data_end: u64,
    expected_crc32: u32,
    expected_compressed: u64,
    expected_uncompressed: u64,
) -> Result<u64> {
    const DESCRIPTOR_SIGNATURE: u32 = 0x0807_4b50;
    let prefix = read_vec(source, data_end, 4)?;
    let has_signature = le_u32(&prefix, 0)? == DESCRIPTOR_SIGNATURE;
    if has_signature
        && descriptor_matches(
            source,
            data_end,
            central_offset,
            16,
            4,
            expected_crc32,
            expected_compressed,
            expected_uncompressed,
        )?
    {
        return Ok(data_end + 16);
    }
    // A signature-less descriptor is ambiguous when its CRC happens to equal the
    // descriptor signature, so always try the 12-byte interpretation as fallback.
    if descriptor_matches(
        source,
        data_end,
        central_offset,
        12,
        0,
        expected_crc32,
        expected_compressed,
        expected_uncompressed,
    )? {
        return Ok(data_end + 12);
    }
    Err(Error::new(
        ErrorCode::InvalidField,
        format!("data descriptor disagrees with the central directory: {name}"),
    ))
}

#[allow(clippy::too_many_arguments)]
fn descriptor_matches<S: ReadAt>(
    source: &S,
    offset: u64,
    central_offset: u64,
    length: u64,
    value_offset: usize,
    expected_crc32: u32,
    expected_compressed: u64,
    expected_uncompressed: u64,
) -> Result<bool> {
    let end = offset
        .checked_add(length)
        .ok_or_else(|| Error::new(ErrorCode::InvalidField, "data descriptor overflow"))?;
    if end > central_offset {
        return Ok(false);
    }
    let descriptor = read_vec(source, offset, length)?;
    Ok(le_u32(&descriptor, value_offset)? == expected_crc32
        && u64::from(le_u32(&descriptor, value_offset + 4)?) == expected_compressed
        && u64::from(le_u32(&descriptor, value_offset + 8)?) == expected_uncompressed)
}

fn validate_non_overlapping(entries: &[Entry]) -> Result<()> {
    let mut ranges = entries
        .iter()
        .map(|entry| {
            (
                entry.local_header_offset,
                entry.local_record_end,
                entry.name.as_str(),
            )
        })
        .collect::<Vec<_>>();
    ranges.sort_unstable_by_key(|range| range.0);
    for pair in ranges.windows(2) {
        if pair[0].1 > pair[1].0 {
            return Err(Error::new(
                ErrorCode::OverlappingEntries,
                format!("ZIP entries overlap: {} and {}", pair[0].2, pair[1].2),
            ));
        }
    }
    Ok(())
}

fn validate_entry_limits(
    limits: &PackageLimits,
    name: &str,
    compressed: u64,
    uncompressed: u64,
) -> Result<()> {
    if uncompressed > limits.max_entry_uncompressed_bytes {
        return Err(limit_error(
            &format!("uncompressed bytes for {name}"),
            uncompressed,
        ));
    }
    if compressed == 0 {
        if uncompressed != 0 {
            return Err(limit_error(
                &format!("compression ratio for {name}"),
                u64::MAX,
            ));
        }
    } else if uncompressed > compressed.saturating_mul(limits.max_compression_ratio) {
        return Err(limit_error(
            &format!("compression ratio for {name}"),
            uncompressed.div_ceil(compressed),
        ));
    }
    Ok(())
}

fn checked_total(label: &str, current: u64, add: u64, maximum: u64) -> Result<u64> {
    let total = current
        .checked_add(add)
        .ok_or_else(|| limit_error(label, u64::MAX))?;
    if total > maximum {
        return Err(limit_error(label, total));
    }
    Ok(total)
}

fn find_eocd(tail: &[u8]) -> Result<usize> {
    if tail.len() < EOCD_FIXED {
        return Err(Error::new(ErrorCode::Truncated, "ZIP is shorter than EOCD"));
    }
    for index in (0..=tail.len() - EOCD_FIXED).rev() {
        if le_u32(tail, index)? != EOCD_SIGNATURE {
            continue;
        }
        let comment_len = usize::from(le_u16(tail, index + 20)?);
        if index + EOCD_FIXED + comment_len == tail.len() {
            return Ok(index);
        }
    }
    Err(Error::new(
        ErrorCode::InvalidSignature,
        "end-of-central-directory record not found",
    ))
}

fn parse_name(bytes: &[u8]) -> Result<String> {
    std::str::from_utf8(bytes)
        .map(str::to_owned)
        .map_err(|_| Error::new(ErrorCode::InvalidPath, "ZIP entry name is not UTF-8"))
}

fn validate_path(name: &str) -> Result<()> {
    if name.starts_with('/')
        || name.starts_with('\\')
        || name.contains('\\')
        || name.contains('\0')
        || name
            .split('/')
            .any(|segment| segment == "." || segment == "..")
        || name.as_bytes().get(1) == Some(&b':')
    {
        return Err(Error::new(
            ErrorCode::InvalidPath,
            format!("unsafe ZIP entry path: {name:?}"),
        ));
    }
    Ok(())
}

fn read_vec<S: ReadAt>(source: &S, offset: u64, length: u64) -> Result<Vec<u8>> {
    let length = usize::try_from(length)
        .map_err(|_| Error::new(ErrorCode::LimitExceeded, "range is too large for this host"))?;
    let mut bytes = vec![0; length];
    source.read_at(offset, &mut bytes)?;
    Ok(bytes)
}

fn le_u16(bytes: &[u8], offset: usize) -> Result<u16> {
    let value = bytes
        .get(offset..offset + 2)
        .ok_or_else(|| Error::new(ErrorCode::Truncated, "truncated 16-bit ZIP field"))?;
    Ok(u16::from_le_bytes([value[0], value[1]]))
}

fn le_u32(bytes: &[u8], offset: usize) -> Result<u32> {
    let value = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| Error::new(ErrorCode::Truncated, "truncated 32-bit ZIP field"))?;
    Ok(u32::from_le_bytes([value[0], value[1], value[2], value[3]]))
}

fn limit_error(label: &str, actual: u64) -> Error {
    Error::new(
        ErrorCode::LimitExceeded,
        format!("package limit exceeded for {label}: {actual}"),
    )
}
