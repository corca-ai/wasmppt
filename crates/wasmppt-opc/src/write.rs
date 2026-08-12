use std::{collections::HashSet, io::Write};

use flate2::{Compression, write::DeflateEncoder};

use crate::{
    CompressionMethod, Entry, Error, ErrorCode, OutputSink, ReadAt, Result, VecSink, ZipArchive,
};

const LOCAL_SIGNATURE: u32 = 0x0403_4b50;
const CENTRAL_SIGNATURE: u32 = 0x0201_4b50;
const EOCD_SIGNATURE: u32 = 0x0605_4b50;
const COPY_BUFFER_BYTES: usize = 64 * 1024;
const UTF8_FLAG: u16 = 1 << 11;
const DATA_DESCRIPTOR_FLAG: u16 = 1 << 3;
const ENCRYPTED_FLAG: u16 = 1;
const DETERMINISTIC_DOS_DATE: u16 = 0x0021; // 1980-01-01

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum RewriteMode {
    #[default]
    Preserve,
    Deterministic,
}

#[derive(Clone, Debug)]
pub struct EntryOptions {
    pub compression: CompressionMethod,
    pub modified_time: u16,
    pub modified_date: u16,
    pub local_extra: Vec<u8>,
    pub central_extra: Vec<u8>,
    pub comment: Vec<u8>,
    pub internal_attributes: u16,
    pub external_attributes: u32,
}

impl Default for EntryOptions {
    fn default() -> Self {
        Self::deterministic(CompressionMethod::Deflate)
    }
}

impl EntryOptions {
    pub fn deterministic(compression: CompressionMethod) -> Self {
        Self {
            compression,
            modified_time: 0,
            modified_date: DETERMINISTIC_DOS_DATE,
            local_extra: Vec::new(),
            central_extra: Vec::new(),
            comment: Vec::new(),
            internal_attributes: 0,
            external_attributes: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WriteStats {
    pub entries: u64,
    pub raw_copied_entries: u64,
    pub raw_copied_bytes: u64,
    pub inflated_entries: u64,
    pub recompressed_entries: u64,
}

#[derive(Clone, Debug)]
struct WrittenEntry {
    name: String,
    flags: u16,
    compression: CompressionMethod,
    crc32: u32,
    compressed_size: u32,
    uncompressed_size: u32,
    modified_time: u16,
    modified_date: u16,
    version_made_by: u16,
    version_needed: u16,
    internal_attributes: u16,
    external_attributes: u32,
    central_extra: Vec<u8>,
    comment: Vec<u8>,
    local_header_offset: u32,
}

/// A forward-only ZIP writer. It never seeks and keeps only central metadata.
#[derive(Debug)]
pub struct ZipWriter<S> {
    sink: S,
    entries: Vec<WrittenEntry>,
    names: HashSet<String>,
    archive_comment: Vec<u8>,
    stats: WriteStats,
}

impl<S: OutputSink> ZipWriter<S> {
    pub fn new(sink: S) -> Self {
        Self {
            sink,
            entries: Vec::new(),
            names: HashSet::new(),
            archive_comment: Vec::new(),
            stats: WriteStats::default(),
        }
    }

    pub fn set_comment(&mut self, comment: &[u8]) -> Result<()> {
        ensure_u16("archive comment", comment.len())?;
        self.archive_comment.clear();
        self.archive_comment.extend_from_slice(comment);
        Ok(())
    }

    /// Copy an entry's compressed payload without inflating or recompressing it.
    pub fn raw_copy<R: ReadAt>(
        &mut self,
        archive: &ZipArchive<R>,
        entry: &Entry,
        mode: RewriteMode,
    ) -> Result<()> {
        validate_name(&entry.name)?;
        let compressed_size = ensure_u32("compressed entry size", entry.compressed_size)?;
        let uncompressed_size = ensure_u32("uncompressed entry size", entry.uncompressed_size)?;
        let local_header_offset = ensure_u32("local header offset", self.sink.position())?;
        let metadata = RawMetadata::from_entry(entry, mode);
        ensure_metadata_lengths(
            entry.name.len(),
            metadata.local_extra.len(),
            metadata.central_extra.len(),
            metadata.comment.len(),
        )?;
        self.reserve_name(&entry.name)?;
        let flags = normalized_flags(entry.flags);
        self.write_local_header(
            &entry.name,
            flags,
            entry.compression,
            entry.crc32,
            compressed_size,
            uncompressed_size,
            metadata.modified_time,
            metadata.modified_date,
            metadata.local_extra,
            metadata.version_needed,
        )?;

        let mut offset = entry.data_offset;
        let mut remaining = entry.compressed_size;
        let mut buffer = vec![0u8; COPY_BUFFER_BYTES];
        while remaining != 0 {
            let amount = usize::try_from(remaining.min(buffer.len() as u64))
                .expect("copy chunk always fits usize");
            archive.source().read_at(offset, &mut buffer[..amount])?;
            self.sink.write_all(&buffer[..amount])?;
            offset += amount as u64;
            remaining -= amount as u64;
        }

        self.entries.push(WrittenEntry {
            name: entry.name.clone(),
            flags,
            compression: entry.compression,
            crc32: entry.crc32,
            compressed_size,
            uncompressed_size,
            modified_time: metadata.modified_time,
            modified_date: metadata.modified_date,
            version_made_by: metadata.version_made_by,
            version_needed: metadata.version_needed,
            internal_attributes: metadata.internal_attributes,
            external_attributes: metadata.external_attributes,
            central_extra: metadata.central_extra.to_vec(),
            comment: metadata.comment.to_vec(),
            local_header_offset,
        });
        self.stats.entries += 1;
        self.stats.raw_copied_entries += 1;
        self.stats.raw_copied_bytes += entry.compressed_size;
        Ok(())
    }

    /// Write a changed entry. Only the changed bytes are compressed in memory.
    pub fn write_entry(&mut self, name: &str, bytes: &[u8], options: &EntryOptions) -> Result<()> {
        validate_name(name)?;
        ensure_metadata_lengths(
            name.len(),
            options.local_extra.len(),
            options.central_extra.len(),
            options.comment.len(),
        )?;
        let uncompressed_size = ensure_u32("uncompressed entry size", bytes.len() as u64)?;
        let crc32 = crc32fast::hash(bytes);
        let compressed = match options.compression {
            CompressionMethod::Stored => bytes.to_vec(),
            CompressionMethod::Deflate => {
                let mut encoder = DeflateEncoder::new(Vec::new(), Compression::new(6));
                encoder.write_all(bytes).map_err(|error| {
                    Error::new(ErrorCode::Io, format!("failed to deflate {name}: {error}"))
                })?;
                encoder.finish().map_err(|error| {
                    Error::new(ErrorCode::Io, format!("failed to finish {name}: {error}"))
                })?
            }
            CompressionMethod::Unsupported(code) => {
                return Err(Error::new(
                    ErrorCode::UnsupportedCompression,
                    format!("cannot encode compression method {code} for {name}"),
                ));
            }
        };
        let compressed_size = ensure_u32("compressed entry size", compressed.len() as u64)?;
        let local_header_offset = ensure_u32("local header offset", self.sink.position())?;
        self.reserve_name(name)?;
        let flags = UTF8_FLAG;
        self.write_local_header(
            name,
            flags,
            options.compression,
            crc32,
            compressed_size,
            uncompressed_size,
            options.modified_time,
            options.modified_date,
            &options.local_extra,
            version_needed(options.compression),
        )?;
        self.sink.write_all(&compressed)?;
        self.entries.push(WrittenEntry {
            name: name.to_owned(),
            flags,
            compression: options.compression,
            crc32,
            compressed_size,
            uncompressed_size,
            modified_time: options.modified_time,
            modified_date: options.modified_date,
            version_made_by: 20,
            version_needed: version_needed(options.compression),
            internal_attributes: options.internal_attributes,
            external_attributes: options.external_attributes,
            central_extra: options.central_extra.clone(),
            comment: options.comment.clone(),
            local_header_offset,
        });
        self.stats.entries += 1;
        self.stats.recompressed_entries +=
            u64::from(options.compression == CompressionMethod::Deflate);
        Ok(())
    }

    pub fn finish(mut self) -> Result<(S, WriteStats)> {
        let central_offset = ensure_u32("central-directory offset", self.sink.position())?;
        for index in 0..self.entries.len() {
            let entry = self.entries[index].clone();
            self.write_central_header(&entry)?;
        }
        let central_size = self
            .sink
            .position()
            .checked_sub(u64::from(central_offset))
            .ok_or_else(|| Error::new(ErrorCode::InvalidField, "central offset underflow"))?;
        let central_size = ensure_u32("central-directory size", central_size)?;
        let entry_count = ensure_u16("entry count", self.entries.len())?;
        let comment_len = ensure_u16("archive comment", self.archive_comment.len())?;

        let mut eocd = Vec::with_capacity(22 + self.archive_comment.len());
        push_u32(&mut eocd, EOCD_SIGNATURE);
        push_u16(&mut eocd, 0);
        push_u16(&mut eocd, 0);
        push_u16(&mut eocd, entry_count);
        push_u16(&mut eocd, entry_count);
        push_u32(&mut eocd, central_size);
        push_u32(&mut eocd, central_offset);
        push_u16(&mut eocd, comment_len);
        eocd.extend_from_slice(&self.archive_comment);
        self.sink.write_all(&eocd)?;
        Ok((self.sink, self.stats))
    }

    #[allow(clippy::too_many_arguments)]
    fn write_local_header(
        &mut self,
        name: &str,
        flags: u16,
        compression: CompressionMethod,
        crc32: u32,
        compressed_size: u32,
        uncompressed_size: u32,
        modified_time: u16,
        modified_date: u16,
        extra: &[u8],
        version_needed: u16,
    ) -> Result<()> {
        let mut header = Vec::with_capacity(30 + name.len() + extra.len());
        push_u32(&mut header, LOCAL_SIGNATURE);
        push_u16(&mut header, version_needed);
        push_u16(&mut header, flags);
        push_u16(&mut header, compression.code());
        push_u16(&mut header, modified_time);
        push_u16(&mut header, modified_date);
        push_u32(&mut header, crc32);
        push_u32(&mut header, compressed_size);
        push_u32(&mut header, uncompressed_size);
        push_u16(&mut header, ensure_u16("entry name", name.len())?);
        push_u16(&mut header, ensure_u16("local extra", extra.len())?);
        header.extend_from_slice(name.as_bytes());
        header.extend_from_slice(extra);
        self.sink.write_all(&header)
    }

    fn write_central_header(&mut self, entry: &WrittenEntry) -> Result<()> {
        let mut header = Vec::with_capacity(
            46 + entry.name.len() + entry.central_extra.len() + entry.comment.len(),
        );
        push_u32(&mut header, CENTRAL_SIGNATURE);
        push_u16(&mut header, entry.version_made_by);
        push_u16(&mut header, entry.version_needed);
        push_u16(&mut header, entry.flags);
        push_u16(&mut header, entry.compression.code());
        push_u16(&mut header, entry.modified_time);
        push_u16(&mut header, entry.modified_date);
        push_u32(&mut header, entry.crc32);
        push_u32(&mut header, entry.compressed_size);
        push_u32(&mut header, entry.uncompressed_size);
        push_u16(&mut header, ensure_u16("entry name", entry.name.len())?);
        push_u16(
            &mut header,
            ensure_u16("central extra", entry.central_extra.len())?,
        );
        push_u16(
            &mut header,
            ensure_u16("entry comment", entry.comment.len())?,
        );
        push_u16(&mut header, 0);
        push_u16(&mut header, entry.internal_attributes);
        push_u32(&mut header, entry.external_attributes);
        push_u32(&mut header, entry.local_header_offset);
        header.extend_from_slice(entry.name.as_bytes());
        header.extend_from_slice(&entry.central_extra);
        header.extend_from_slice(&entry.comment);
        self.sink.write_all(&header)
    }

    fn reserve_name(&mut self, name: &str) -> Result<()> {
        if self.entries.len() >= usize::from(u16::MAX) {
            return Err(Error::new(
                ErrorCode::UnsupportedZip64,
                "entry count requires ZIP64",
            ));
        }
        if !self.names.insert(name.to_owned()) {
            return Err(Error::new(
                ErrorCode::DuplicateEntry,
                format!("duplicate ZIP entry: {name}"),
            ));
        }
        Ok(())
    }
}

struct RawMetadata<'a> {
    modified_time: u16,
    modified_date: u16,
    version_made_by: u16,
    version_needed: u16,
    internal_attributes: u16,
    external_attributes: u32,
    local_extra: &'a [u8],
    central_extra: &'a [u8],
    comment: &'a [u8],
}

impl<'a> RawMetadata<'a> {
    fn from_entry(entry: &'a Entry, mode: RewriteMode) -> Self {
        match mode {
            RewriteMode::Preserve => Self {
                modified_time: entry.modified_time,
                modified_date: entry.modified_date,
                version_made_by: entry.version_made_by,
                version_needed: entry.version_needed,
                internal_attributes: entry.internal_attributes,
                external_attributes: entry.external_attributes,
                local_extra: &entry.local_extra,
                central_extra: &entry.central_extra,
                comment: &entry.comment,
            },
            RewriteMode::Deterministic => Self {
                modified_time: 0,
                modified_date: DETERMINISTIC_DOS_DATE,
                version_made_by: 20,
                version_needed: entry.version_needed.max(version_needed(entry.compression)),
                internal_attributes: 0,
                external_attributes: 0,
                local_extra: &[],
                central_extra: &[],
                comment: &[],
            },
        }
    }
}

pub fn rewrite_archive<R: ReadAt, S: OutputSink>(
    archive: &ZipArchive<R>,
    sink: S,
    mode: RewriteMode,
) -> Result<(S, WriteStats)> {
    let mut writer = ZipWriter::new(sink);
    if mode == RewriteMode::Preserve {
        writer.set_comment(archive.comment())?;
    }
    let mut entries = archive.entries().iter().collect::<Vec<_>>();
    if mode == RewriteMode::Deterministic {
        entries.sort_unstable_by(|left, right| left.name.as_bytes().cmp(right.name.as_bytes()));
    }
    for entry in entries {
        writer.raw_copy(archive, entry, mode)?;
    }
    writer.finish()
}

pub fn rewrite_archive_to_vec<R: ReadAt>(
    archive: &ZipArchive<R>,
    mode: RewriteMode,
) -> Result<(Vec<u8>, WriteStats)> {
    let (sink, stats) = rewrite_archive(archive, VecSink::new(), mode)?;
    Ok((sink.into_inner(), stats))
}

fn normalized_flags(flags: u16) -> u16 {
    (flags & !(DATA_DESCRIPTOR_FLAG | ENCRYPTED_FLAG)) | UTF8_FLAG
}

const fn version_needed(compression: CompressionMethod) -> u16 {
    match compression {
        CompressionMethod::Stored => 10,
        CompressionMethod::Deflate | CompressionMethod::Unsupported(_) => 20,
    }
}

fn validate_name(name: &str) -> Result<()> {
    if name.is_empty()
        || name.starts_with('/')
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

fn ensure_metadata_lengths(
    name: usize,
    local: usize,
    central: usize,
    comment: usize,
) -> Result<()> {
    ensure_u16("entry name", name)?;
    ensure_u16("local extra", local)?;
    ensure_u16("central extra", central)?;
    ensure_u16("entry comment", comment)?;
    Ok(())
}

fn ensure_u16(label: &str, value: usize) -> Result<u16> {
    u16::try_from(value).map_err(|_| {
        Error::new(
            ErrorCode::LimitExceeded,
            format!("{label} exceeds classic ZIP limit: {value}"),
        )
    })
}

fn ensure_u32(label: &str, value: u64) -> Result<u32> {
    u32::try_from(value).map_err(|_| {
        Error::new(
            ErrorCode::UnsupportedZip64,
            format!("{label} requires ZIP64: {value}"),
        )
    })
}

fn push_u16(bytes: &mut Vec<u8>, value: u16) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn push_u32(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend_from_slice(&value.to_le_bytes());
}
