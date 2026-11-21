use std::io::{Cursor, Write};

use viewera_lib::files::list_zip_entries;
use viewera_lib::models::RootEntry;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

fn root(path: &std::path::Path) -> RootEntry {
    RootEntry {
        id: "zip-test-root".to_owned(),
        label: "ZIP test".to_owned(),
        path: path.to_string_lossy().into_owned(),
        added_at: String::new(),
    }
}

fn write_archive(path: &std::path::Path, entries: &[(&str, &[u8])]) {
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
    std::fs::write(path, archive.finish().unwrap().into_inner()).unwrap();
}

#[test]
fn lists_zip_entry_metadata_without_extracting_entries() {
    let directory = tempfile::tempdir().unwrap();
    write_archive(
        &directory.path().join("archive.zip"),
        &[
            ("docs/readme.txt", b"read only"),
            ("images/photo.png", b"image"),
        ],
    );

    let entries = list_zip_entries(&root(directory.path()), "archive.zip").unwrap();

    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].name, "docs/readme.txt");
    assert_eq!(entries[0].uncompressed_size, 9);
    assert_eq!(entries[1].name, "images/photo.png");
    assert!(!directory.path().join("docs").exists());
}

#[test]
fn rejects_unsafe_zip_entry_paths_and_archive_paths_outside_the_root() {
    let directory = tempfile::tempdir().unwrap();
    write_archive(
        &directory.path().join("unsafe.zip"),
        &[("../outside.txt", b"escape")],
    );

    assert!(list_zip_entries(&root(directory.path()), "unsafe.zip").is_err());
    assert!(list_zip_entries(&root(directory.path()), "../outside.zip").is_err());
}
