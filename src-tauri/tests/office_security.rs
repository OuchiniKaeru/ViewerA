use std::io::{Cursor, Write};

use viewera_lib::office::{OfficePackageLimits, validate_package};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

fn write_archive(entries: &[(&str, &[u8])]) -> tempfile::NamedTempFile {
    let file = tempfile::NamedTempFile::new().unwrap();
    let mut archive = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, contents) in entries {
        archive
            .start_file(
                *name,
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
            )
            .unwrap();
        archive.write_all(contents).unwrap();
    }
    let bytes = archive.finish().unwrap().into_inner();
    std::fs::write(file.path(), bytes).unwrap();
    file
}

#[test]
fn accepts_a_small_well_formed_office_archive() {
    let archive = write_archive(&[("word/document.xml", b"<document/>")]);

    validate_package(archive.path(), &OfficePackageLimits::default()).unwrap();
}

#[test]
fn rejects_archives_that_exceed_the_uncompressed_size_limit() {
    let archive = write_archive(&[("word/document.xml", b"<document/>")]);
    let limits = OfficePackageLimits {
        max_uncompressed_bytes: 8,
        ..OfficePackageLimits::default()
    };

    assert!(validate_package(archive.path(), &limits).is_err());
}

#[test]
fn rejects_archive_entries_that_escape_the_package_directory() {
    let archive = write_archive(&[("../outside.xml", b"<data/>")]);

    assert!(validate_package(archive.path(), &OfficePackageLimits::default()).is_err());
}
