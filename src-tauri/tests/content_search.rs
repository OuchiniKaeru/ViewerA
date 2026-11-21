use std::sync::atomic::AtomicBool;

use viewera_lib::files::search_file_contents;
use viewera_lib::models::RootEntry;

fn root(path: &std::path::Path) -> RootEntry {
    RootEntry {
        id: "search-test-root".to_owned(),
        label: "Search test".to_owned(),
        path: path.to_string_lossy().into_owned(),
        added_at: String::new(),
    }
}

#[test]
fn searches_supported_text_files_recursively_and_returns_all_matches() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::create_dir(directory.path().join("docs")).unwrap();
    std::fs::write(
        directory.path().join("docs/alpha.txt"),
        "first line\nNeedle appears here\n",
    )
    .unwrap();
    std::fs::write(directory.path().join("notes.md"), "Another NEEDLE match").unwrap();
    std::fs::write(directory.path().join("archive.bin"), b"needle").unwrap();
    std::fs::write(directory.path().join(".hidden.txt"), "needle").unwrap();

    let results = search_file_contents(
        &root(directory.path()),
        "",
        "needle",
        false,
        &AtomicBool::new(false),
    )
    .unwrap();

    assert_eq!(results.total, 2);
    assert_eq!(results.items[0].file.relative_path, "docs/alpha.txt");
    assert!(results.items[0].snippet.contains("Needle"));
    assert_eq!(results.items[1].file.relative_path, "notes.md");
    assert!(!results.truncated);
}

#[test]
fn rejects_empty_queries_and_cancels_long_running_searches() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("notes.txt"), "text").unwrap();

    assert!(
        search_file_contents(
            &root(directory.path()),
            "",
            "  ",
            false,
            &AtomicBool::new(false),
        )
        .is_err()
    );
    assert!(
        search_file_contents(
            &root(directory.path()),
            "",
            "text",
            false,
            &AtomicBool::new(true),
        )
        .is_err()
    );
}
