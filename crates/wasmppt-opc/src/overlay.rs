use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    sync::Arc,
};

use crate::{
    CompressionMethod, Entry, EntryOptions, Error, ErrorCode, MemorySource, PackagePartSource,
    RewriteMode, StreamingZipWriter, WriteStats, ZipArchive,
};

/// Resource bounds for one immutable logical package overlay.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OverlayLimits {
    pub max_materialized_parts: usize,
    pub max_materialized_bytes: usize,
}

impl Default for OverlayLimits {
    fn default() -> Self {
        Self {
            max_materialized_parts: 100_000,
            max_materialized_bytes: 256 * 1024 * 1024,
        }
    }
}

/// One new or rewritten logical package part.
#[derive(Clone, Debug)]
pub struct OverlayPart {
    pub bytes: Arc<[u8]>,
    pub options: EntryOptions,
}

impl OverlayPart {
    #[must_use]
    pub fn deflated(bytes: impl Into<Arc<[u8]>>) -> Self {
        Self {
            bytes: bytes.into(),
            options: EntryOptions::deterministic(CompressionMethod::Deflate),
        }
    }
}

/// Memory accounting for one logical overlay revision.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OverlayStats {
    pub logical_parts: u64,
    pub materialized_parts: u64,
    pub materialized_bytes: u64,
    pub reused_source_bytes: u64,
    pub removed_parts: u64,
}

/// An immutable package revision that retains only changed bytes.
#[derive(Clone, Debug)]
pub struct PackageOverlay {
    archive: ZipArchive<MemorySource>,
    names: Vec<String>,
    name_set: BTreeSet<String>,
    overrides: BTreeMap<String, OverlayPart>,
    removed_parts: u64,
    materialized_bytes: u64,
}

impl PackageOverlay {
    pub fn new(
        source: impl Into<Arc<[u8]>>,
        overrides: BTreeMap<String, OverlayPart>,
        removed: BTreeSet<String>,
        limits: &OverlayLimits,
    ) -> crate::Result<Self> {
        if overrides.len() > limits.max_materialized_parts {
            return Err(limit_error("overlay materialized-part limit exceeded"));
        }
        let materialized_bytes = overrides
            .values()
            .map(|part| part.bytes.len())
            .try_fold(0usize, usize::checked_add)
            .ok_or_else(|| limit_error("overlay materialized-byte count overflowed"))?;
        if materialized_bytes > limits.max_materialized_bytes {
            return Err(limit_error("overlay materialized-byte limit exceeded"));
        }
        for name in overrides.keys().chain(removed.iter()) {
            validate_overlay_name(name)?;
        }

        let archive = ZipArchive::from_bytes(source)?;
        let mut name_set = archive
            .entries()
            .iter()
            .map(|entry| entry.name.clone())
            .collect::<BTreeSet<_>>();
        let removed_parts = removed.iter().filter(|name| name_set.remove(*name)).count() as u64;
        name_set.extend(overrides.keys().cloned());
        let names = name_set.iter().cloned().collect();
        Ok(Self {
            archive,
            names,
            name_set,
            overrides,
            removed_parts,
            materialized_bytes: materialized_bytes as u64,
        })
    }

    #[must_use]
    pub fn stats(&self) -> OverlayStats {
        let reused_source_bytes = self
            .names
            .iter()
            .filter(|name| !self.overrides.contains_key(*name))
            .filter_map(|name| self.archive.entry(name))
            .map(|entry| entry.compressed_size)
            .fold(0u64, u64::saturating_add);
        OverlayStats {
            logical_parts: self.names.len() as u64,
            materialized_parts: self.overrides.len() as u64,
            materialized_bytes: self.materialized_bytes,
            reused_source_bytes,
            removed_parts: self.removed_parts,
        }
    }

    #[must_use]
    pub fn generation_cursor(&self) -> OverlayCursor {
        let entries = self
            .names
            .iter()
            .filter_map(|name| {
                self.overrides.get(name).map_or_else(
                    || self.archive.entry(name).cloned().map(CursorEntry::Raw),
                    |part| {
                        Some(CursorEntry::Materialized {
                            name: name.clone(),
                            part: part.clone(),
                        })
                    },
                )
            })
            .collect();
        OverlayCursor {
            writer: StreamingZipWriter::new(self.archive.source().clone()),
            entries,
            finish_started: false,
            maximum_output_chunk_bytes: 0,
        }
    }

    #[must_use]
    pub fn changed_parts_since(&self, previous: &Self) -> Vec<String> {
        let source_changed =
            self.archive.source().as_bytes() != previous.archive.source().as_bytes();
        let mut candidates = self
            .overrides
            .keys()
            .chain(previous.overrides.keys())
            .chain(self.name_set.symmetric_difference(&previous.name_set))
            .cloned()
            .collect::<BTreeSet<_>>();
        if source_changed {
            candidates.extend(self.name_set.union(&previous.name_set).cloned());
        }
        candidates
            .into_iter()
            .filter(
                |name| match (self.logical_bytes(name), previous.logical_bytes(name)) {
                    (Some(current), Some(old)) => current != old,
                    (None, None) => false,
                    _ => true,
                },
            )
            .collect()
    }

    fn logical_bytes(&self, name: &str) -> Option<Vec<u8>> {
        if !self.name_set.contains(name) {
            return None;
        }
        self.overrides
            .get(name)
            .map(|part| part.bytes.to_vec())
            .or_else(|| {
                self.archive
                    .entry(name)
                    .and_then(|entry| self.archive.read_entry(entry).ok())
            })
    }
}

impl PackagePartSource for PackageOverlay {
    fn part_names(&self) -> Vec<String> {
        self.names.clone()
    }

    fn is_modified(&self, name: &str) -> bool {
        self.overrides.contains_key(name)
    }

    fn contains_part(&self, name: &str) -> bool {
        self.name_set.contains(name)
    }

    fn read_part(&self, name: &str) -> crate::Result<Vec<u8>> {
        if !self.name_set.contains(name) {
            return Err(Error::new(
                ErrorCode::InvalidField,
                format!("logical package part not found: {name}"),
            ));
        }
        if let Some(part) = self.overrides.get(name) {
            return Ok(part.bytes.to_vec());
        }
        let entry = self.archive.entry(name).ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidField,
                format!("source package part not found: {name}"),
            )
        })?;
        self.archive.read_entry(entry)
    }
}

#[derive(Debug)]
enum CursorEntry {
    Raw(Entry),
    Materialized { name: String, part: OverlayPart },
}

/// Pull-based exact export of one [`PackageOverlay`] revision.
#[derive(Debug)]
pub struct OverlayCursor {
    writer: StreamingZipWriter<MemorySource>,
    entries: VecDeque<CursorEntry>,
    finish_started: bool,
    maximum_output_chunk_bytes: u64,
}

impl OverlayCursor {
    pub fn pull(&mut self, maximum_bytes: usize) -> crate::Result<Vec<u8>> {
        if maximum_bytes == 0 {
            return Err(Error::new(
                ErrorCode::InvalidField,
                "overlay output chunk size must be positive",
            ));
        }
        let mut output = Vec::with_capacity(maximum_bytes);
        while output.len() < maximum_bytes && !self.writer.is_done() {
            if !self.writer.entry_active() && !self.finish_started {
                match self.entries.pop_front() {
                    Some(CursorEntry::Raw(entry)) => {
                        self.writer.start_raw_copy(&entry, RewriteMode::Preserve)?;
                    }
                    Some(CursorEntry::Materialized { name, part }) => {
                        self.writer
                            .start_shared_entry(name, part.bytes, part.options)?;
                    }
                    None => {
                        self.writer.start_finish()?;
                        self.finish_started = true;
                    }
                }
            }
            let chunk = self.writer.pull(maximum_bytes - output.len())?;
            if chunk.is_empty() && self.writer.is_done() {
                break;
            }
            output.extend(chunk);
        }
        self.maximum_output_chunk_bytes = self.maximum_output_chunk_bytes.max(output.len() as u64);
        Ok(output)
    }

    #[must_use]
    pub fn is_done(&self) -> bool {
        self.writer.is_done()
    }

    #[must_use]
    pub fn stats(&self) -> Option<(WriteStats, u64)> {
        self.is_done()
            .then(|| (self.writer.stats(), self.maximum_output_chunk_bytes))
    }
}

fn validate_overlay_name(name: &str) -> crate::Result<()> {
    if name.is_empty()
        || name.starts_with('/')
        || name.ends_with('/')
        || name.contains('\\')
        || name
            .split('/')
            .any(|segment| segment.is_empty() || segment == "." || segment == "..")
    {
        return Err(Error::new(
            ErrorCode::InvalidPath,
            format!("unsafe overlay part name: {name}"),
        ));
    }
    Ok(())
}

fn limit_error(message: &str) -> Error {
    Error::new(ErrorCode::LimitExceeded, message)
}
