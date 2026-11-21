use viewera_lib::path_validator::validate_relative_path;

#[test]
fn accepts_a_path_confined_to_the_root() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("reports")).unwrap();
    std::fs::write(root.path().join("reports/summary.txt"), "ok").unwrap();

    let resolved = validate_relative_path(root.path(), "reports/summary.txt").unwrap();

    assert_eq!(resolved, root.path().join("reports/summary.txt"));
}

#[test]
fn rejects_parent_directory_traversal() {
    let root = tempfile::tempdir().unwrap();

    let result = validate_relative_path(root.path(), "../outside.txt");

    assert!(result.is_err());
}

#[test]
fn rejects_absolute_paths() {
    let root = tempfile::tempdir().unwrap();

    let result = validate_relative_path(root.path(), "/etc/passwd");

    assert!(result.is_err());
}

#[cfg(unix)]
#[test]
fn rejects_symbolic_links_even_when_the_target_is_inside_the_root() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("target.txt"), "ok").unwrap();
    std::os::unix::fs::symlink(root.path().join("target.txt"), root.path().join("link.txt"))
        .unwrap();

    let result = validate_relative_path(root.path(), "link.txt");

    assert!(result.is_err());
}
