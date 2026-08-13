#![warn(missing_docs)]

//! Native filesystem capabilities for the host-neutral package core.

use std::{
    fs::File,
    io::{BufWriter, Read, Seek, SeekFrom, Write},
    path::Path,
    sync::Mutex,
};

use wasmppt_opc::{Error, ErrorCode, OutputSink, ReadAt, Result, WriteSink};

/// A bounded random-access source backed by an open native file.
#[derive(Debug)]
pub struct FileSource {
    file: Mutex<File>,
    length: u64,
}

impl FileSource {
    /// Open `path` once and retain its length for bounded random-access reads.
    pub fn open(path: impl AsRef<Path>) -> std::io::Result<Self> {
        let file = File::open(path)?;
        let length = file.metadata()?.len();
        Ok(Self {
            file: Mutex::new(file),
            length,
        })
    }
}

impl ReadAt for FileSource {
    fn len(&self) -> u64 {
        self.length
    }

    fn read_at(&self, offset: u64, buffer: &mut [u8]) -> Result<()> {
        let end = offset
            .checked_add(buffer.len() as u64)
            .ok_or_else(|| Error::new(ErrorCode::Truncated, "native file read range overflows"))?;
        if end > self.length {
            return Err(Error::new(
                ErrorCode::Truncated,
                format!(
                    "native file read range {offset}..{end} exceeds {}",
                    self.length
                ),
            ));
        }
        let mut file = self
            .file
            .lock()
            .map_err(|_| Error::new(ErrorCode::Io, "native file source lock was poisoned"))?;
        file.seek(SeekFrom::Start(offset))
            .and_then(|_| file.read_exact(buffer))
            .map_err(|error| {
                Error::new(
                    ErrorCode::Io,
                    format!("failed to read native file: {error}"),
                )
            })
    }
}

/// A forward-only, buffered native file sink.
#[derive(Debug)]
pub struct FileSink {
    sink: WriteSink<BufWriter<File>>,
}

impl FileSink {
    /// Create or truncate `path` and buffer forward-only package writes.
    pub fn create(path: impl AsRef<Path>) -> std::io::Result<Self> {
        Ok(Self {
            sink: WriteSink::new(BufWriter::new(File::create(path)?)),
        })
    }

    /// Flush all buffered bytes and return the final output length.
    pub fn finish(self) -> std::io::Result<u64> {
        let length = self.sink.position();
        let mut writer = self.sink.into_inner();
        writer.flush()?;
        Ok(length)
    }
}

impl OutputSink for FileSink {
    fn position(&self) -> u64 {
        self.sink.position()
    }

    fn write_all(&mut self, bytes: &[u8]) -> Result<()> {
        self.sink.write_all(bytes)
    }
}

/// Native adapter package version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
