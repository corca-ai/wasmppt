//! Bounded Open Packaging Conventions and ZIP primitives.
//!
//! This crate is host-agnostic. Runtime I/O belongs behind capability traits and
//! must not introduce browser, JavaScript, or Cloudflare dependencies here.

/// The package version of this crate.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
