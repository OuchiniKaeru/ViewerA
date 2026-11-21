use std::path::Path;

use anyhow::{Context, Result, bail};
use zip::ZipArchive;

#[derive(Debug, Clone, Copy)]
pub struct OfficePackageLimits {
    pub max_compressed_bytes: u64,
    pub max_entries: usize,
    pub max_uncompressed_bytes: u64,
    pub max_xml_part_bytes: u64,
    pub max_total_xml_bytes: u64,
}

impl Default for OfficePackageLimits {
    fn default() -> Self {
        Self {
            max_compressed_bytes: 100 * 1024 * 1024,
            max_entries: 5000,
            max_uncompressed_bytes: 512 * 1024 * 1024,
            max_xml_part_bytes: 32 * 1024 * 1024,
            max_total_xml_bytes: 256 * 1024 * 1024,
        }
    }
}

pub fn validate_package(path: &Path, limits: &OfficePackageLimits) -> Result<()> {
    let file_size = std::fs::metadata(path)
        .context("could not inspect Office package metadata")?
        .len();
    if file_size > limits.max_compressed_bytes {
        bail!("Office package exceeds the compressed file size limit");
    }

    let file = std::fs::File::open(path).context("could not open Office package")?;
    let mut archive =
        ZipArchive::new(file).context("Office document is not a valid ZIP package")?;
    if archive.len() > limits.max_entries {
        bail!("Office package exceeds the entry count limit");
    }

    let mut total_uncompressed = 0_u64;
    let mut total_xml = 0_u64;
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .context("could not inspect an Office package entry")?;
        let name = entry.name();
        if name.contains('\\') || entry.enclosed_name().is_none() {
            bail!("Office package contains an unsafe entry path");
        }
        let size = entry.size();
        total_uncompressed = total_uncompressed
            .checked_add(size)
            .context("Office package size overflow")?;
        if total_uncompressed > limits.max_uncompressed_bytes {
            bail!("Office package exceeds the uncompressed size limit");
        }
        if name.ends_with(".xml") || name.ends_with(".rels") {
            if size > limits.max_xml_part_bytes {
                bail!("Office package contains an oversized XML part");
            }
            total_xml = total_xml
                .checked_add(size)
                .context("Office XML size overflow")?;
            if total_xml > limits.max_total_xml_bytes {
                bail!("Office package exceeds the total XML size limit");
            }
        }
    }
    Ok(())
}
