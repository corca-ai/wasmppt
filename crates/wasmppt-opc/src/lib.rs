//! Bounded Open Packaging Conventions and ZIP primitives.
//!
//! The ZIP reader indexes the central directory without inflating entry payloads.
//! The streaming writer can copy unchanged compressed payloads verbatim, which is
//! the critical path for repeated PowerPoint template generation.
//!
//! This crate is host-agnostic. Runtime I/O belongs behind capability traits and
//! must not introduce browser, JavaScript, or Cloudflare dependencies here.

mod error;
mod graph;
mod io;
mod limits;
mod read;
mod write;

pub use error::{Error, ErrorCode, Result};
pub use graph::{
    Conformance, Diagnostic, DiagnosticCode, GraphError, PackageGraph, Part, PartId, Relationship,
    RelationshipTarget, TraversalLimit,
};
pub use io::{MemorySource, OutputSink, ReadAt, VecSink, WriteSink};
pub use limits::PackageLimits;
pub use read::{CompressionMethod, Entry, ZipArchive};
pub use write::{
    EntryOptions, RewriteMode, WriteStats, ZipWriter, rewrite_archive, rewrite_archive_to_vec,
};

/// The package version of this crate.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
