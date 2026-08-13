use std::{env, fs};

use wasmppt_opc::{RewriteMode, ZipArchive, rewrite_archive_to_vec};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let input = env::args_os()
        .nth(1)
        .ok_or("usage: open_rewrite INPUT OUTPUT")?;
    let output = env::args_os()
        .nth(2)
        .ok_or("usage: open_rewrite INPUT OUTPUT")?;
    let archive = ZipArchive::from_bytes(fs::read(input)?)?;
    let (bytes, stats) = rewrite_archive_to_vec(&archive, RewriteMode::Preserve)?;
    fs::write(output, bytes)?;
    println!(
        "rewrote {} entries ({} raw-copied bytes)",
        stats.entries, stats.raw_copied_bytes
    );
    Ok(())
}
