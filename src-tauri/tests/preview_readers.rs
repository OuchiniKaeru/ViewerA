use viewera_lib::files::{read_csv_preview, read_text_preview};
use viewera_lib::models::RootEntry;

fn root(path: &std::path::Path) -> RootEntry {
    RootEntry {
        id: "reader-test".to_owned(),
        label: "Test".to_owned(),
        path: path.to_string_lossy().into_owned(),
        added_at: String::new(),
    }
}

#[test]
fn limits_text_preview_reads_and_reports_truncation() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("notes.txt"), "abcdefgh").unwrap();

    let preview = read_text_preview(&root(directory.path()), "notes.txt", 0, 4, 8).unwrap();

    assert_eq!(preview.text, "abcd");
    assert_eq!(preview.total_bytes, 8);
    assert!(preview.truncated);
}

#[test]
fn reads_csv_pages_without_interpreting_cells_as_markup() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(
        directory.path().join("data.csv"),
        "name,value\n<svg onload=alert(1)>,1\nsecond,2\n",
    )
    .unwrap();

    let page = read_csv_preview(&root(directory.path()), "data.csv", 0, 1, true, 5000).unwrap();

    assert_eq!(page.headers, ["name", "value"]);
    assert_eq!(page.rows.len(), 1);
    assert_eq!(page.rows[0][0], "<svg onload=alert(1)>");
    assert!(page.has_more);
}
