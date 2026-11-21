use viewera_lib::files::{list_directory, preview_descriptor};
use viewera_lib::models::RootEntry;

fn root(path: &std::path::Path) -> RootEntry {
    RootEntry {
        id: "root-test".to_owned(),
        label: "Test".to_owned(),
        path: path.to_string_lossy().into_owned(),
        added_at: String::new(),
    }
}

#[test]
fn lists_directories_before_files_and_uses_natural_name_order() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::create_dir(directory.path().join("folder")).unwrap();
    std::fs::write(directory.path().join("file10.txt"), "10").unwrap();
    std::fs::write(directory.path().join("file2.txt"), "2").unwrap();
    std::fs::write(directory.path().join(".hidden.txt"), "hidden").unwrap();

    let first_page =
        list_directory(&root(directory.path()), "", 0, 2, false, "", "name", "asc").unwrap();
    let second_page =
        list_directory(&root(directory.path()), "", 1, 2, false, "", "name", "asc").unwrap();

    assert_eq!(
        first_page
            .items
            .iter()
            .map(|entry| entry.name.as_str())
            .collect::<Vec<_>>(),
        ["folder", "file2.txt"]
    );
    assert_eq!(first_page.total, 3);
    assert_eq!(second_page.items[0].name, "file10.txt");
}

#[test]
fn preview_descriptor_resolves_only_a_file_within_its_root() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("report.PDF"), "pdf").unwrap();

    let descriptor = preview_descriptor(&root(directory.path()), "report.PDF").unwrap();

    assert_eq!(descriptor.preview_type, "pdf");
    assert_eq!(descriptor.size, 3);
    assert!(descriptor.supports_range);
}

#[test]
fn filters_the_full_current_directory_before_paging() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("alpha.txt"), "a").unwrap();
    std::fs::write(directory.path().join("report.txt"), "r").unwrap();

    let page = list_directory(
        &root(directory.path()),
        "",
        0,
        1,
        false,
        "report",
        "name",
        "asc",
    )
    .unwrap();

    assert_eq!(page.total, 1);
    assert_eq!(page.items[0].name, "report.txt");
}

#[test]
fn sorts_files_by_size_and_keeps_directories_first() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::create_dir(directory.path().join("folder")).unwrap();
    std::fs::write(directory.path().join("large.txt"), "a much larger file").unwrap();
    std::fs::write(directory.path().join("small.txt"), "a").unwrap();

    let ascending =
        list_directory(&root(directory.path()), "", 0, 10, false, "", "size", "asc").unwrap();
    let descending = list_directory(
        &root(directory.path()),
        "",
        0,
        10,
        false,
        "",
        "size",
        "desc",
    )
    .unwrap();

    assert_eq!(
        ascending
            .items
            .iter()
            .map(|entry| entry.name.as_str())
            .collect::<Vec<_>>(),
        ["folder", "small.txt", "large.txt"]
    );
    assert_eq!(
        descending
            .items
            .iter()
            .map(|entry| entry.name.as_str())
            .collect::<Vec<_>>(),
        ["folder", "large.txt", "small.txt"]
    );
}

#[test]
fn resolves_file_metadata_only_inside_the_registered_root() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("notes.txt"), "hello").unwrap();

    let entry =
        viewera_lib::files::file_metadata(&root(directory.path()), "notes.txt").unwrap();
    assert_eq!(entry.name, "notes.txt");
    assert_eq!(entry.size, 5);
    assert!(!entry.is_dir);
    assert!(
        viewera_lib::files::file_metadata(&root(directory.path()), "../outside.txt").is_err()
    );
}
