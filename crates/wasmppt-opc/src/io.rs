use std::{io::Write, sync::Arc};

use crate::{Error, ErrorCode, Result};

/// Random-access byte source used by the package indexer and raw-copy writer.
pub trait ReadAt {
    /// Total source length.
    fn len(&self) -> u64;

    /// Whether this source has no bytes.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Fill `buffer` with bytes starting at `offset`.
    fn read_at(&self, offset: u64, buffer: &mut [u8]) -> Result<()>;
}

/// Immutable, cheaply cloned in-memory source used by browser and native adapters.
#[derive(Clone, Debug)]
pub struct MemorySource {
    bytes: Arc<[u8]>,
}

impl MemorySource {
    pub fn new(bytes: impl Into<Arc<[u8]>>) -> Self {
        Self {
            bytes: bytes.into(),
        }
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }
}

impl ReadAt for MemorySource {
    fn len(&self) -> u64 {
        self.bytes.len() as u64
    }

    fn read_at(&self, offset: u64, buffer: &mut [u8]) -> Result<()> {
        let start = usize::try_from(offset)
            .map_err(|_| Error::new(ErrorCode::Truncated, "read offset does not fit in memory"))?;
        let end = start.checked_add(buffer.len()).ok_or_else(|| {
            Error::new(ErrorCode::Truncated, "read range overflows address space")
        })?;
        let source = self.bytes.get(start..end).ok_or_else(|| {
            Error::new(
                ErrorCode::Truncated,
                format!("read range {start}..{end} exceeds source length"),
            )
        })?;
        buffer.copy_from_slice(source);
        Ok(())
    }
}

/// Forward-only output capability. Implementations track their own byte position.
pub trait OutputSink {
    fn position(&self) -> u64;
    fn write_all(&mut self, bytes: &[u8]) -> Result<()>;
}

/// In-memory output sink for browser buffers, tests, and small native results.
#[derive(Debug, Default)]
pub struct VecSink {
    bytes: Vec<u8>,
}

impl VecSink {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn into_inner(self) -> Vec<u8> {
        self.bytes
    }
}

impl OutputSink for VecSink {
    fn position(&self) -> u64 {
        self.bytes.len() as u64
    }

    fn write_all(&mut self, bytes: &[u8]) -> Result<()> {
        self.bytes.extend_from_slice(bytes);
        Ok(())
    }
}

/// Adapter from any `std::io::Write` to a forward-only package sink.
#[derive(Debug)]
pub struct WriteSink<W> {
    inner: W,
    position: u64,
}

impl<W> WriteSink<W> {
    pub fn new(inner: W) -> Self {
        Self { inner, position: 0 }
    }

    pub fn into_inner(self) -> W {
        self.inner
    }
}

impl<W: Write> OutputSink for WriteSink<W> {
    fn position(&self) -> u64 {
        self.position
    }

    fn write_all(&mut self, bytes: &[u8]) -> Result<()> {
        self.inner.write_all(bytes).map_err(|error| {
            Error::new(
                ErrorCode::Io,
                format!("failed to write ZIP output: {error}"),
            )
        })?;
        self.position = self
            .position
            .checked_add(bytes.len() as u64)
            .ok_or_else(|| Error::new(ErrorCode::LimitExceeded, "output position overflow"))?;
        Ok(())
    }
}
