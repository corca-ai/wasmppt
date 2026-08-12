/// Resource limits applied before allocating or inflating untrusted package content.
#[derive(Clone, Debug)]
pub struct PackageLimits {
    pub max_entries: usize,
    pub max_central_directory_bytes: u64,
    pub max_compressed_bytes: u64,
    pub max_uncompressed_bytes: u64,
    pub max_entry_uncompressed_bytes: u64,
    pub max_compression_ratio: u64,
    pub max_name_bytes: usize,
    pub max_extra_bytes: usize,
    pub max_comment_bytes: usize,
}

impl Default for PackageLimits {
    fn default() -> Self {
        Self {
            max_entries: 10_000,
            max_central_directory_bytes: 32 * 1024 * 1024,
            max_compressed_bytes: 512 * 1024 * 1024,
            max_uncompressed_bytes: 2 * 1024 * 1024 * 1024,
            max_entry_uncompressed_bytes: 256 * 1024 * 1024,
            max_compression_ratio: 1_000,
            max_name_bytes: 4 * 1024,
            max_extra_bytes: 64 * 1024,
            max_comment_bytes: 64 * 1024,
        }
    }
}
