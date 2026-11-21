use viewera_lib::models::Settings;
use viewera_lib::storage::AppState;

#[test]
fn persists_registered_roots_across_restarts() {
    let app_data = tempfile::tempdir().unwrap();
    let selected_root = tempfile::tempdir().unwrap();
    let state = AppState::load(app_data.path()).unwrap();

    let added = state.add_root(selected_root.path()).unwrap();
    let restored = AppState::load(app_data.path()).unwrap();

    assert_eq!(restored.roots().unwrap(), vec![added]);
}

#[test]
fn refuses_to_register_a_file_as_a_root() {
    let app_data = tempfile::tempdir().unwrap();
    let file = app_data.path().join("not-a-folder.txt");
    std::fs::write(&file, "content").unwrap();
    let state = AppState::load(app_data.path()).unwrap();

    assert!(state.add_root(&file).is_err());
}

#[test]
fn persists_settings_and_applies_defaults_when_missing() {
    let app_data = tempfile::tempdir().unwrap();
    let state = AppState::load(app_data.path()).unwrap();
    assert_eq!(state.settings().unwrap(), Settings::default());

    let mut settings = Settings::default();
    settings.preview_panel_width = 640;
    state.save_settings(settings.clone()).unwrap();

    assert_eq!(
        AppState::load(app_data.path()).unwrap().settings().unwrap(),
        settings
    );
}

#[test]
fn persists_the_color_theme_and_defaults_legacy_settings_to_light() {
    let app_data = tempfile::tempdir().unwrap();
    let state = AppState::load(app_data.path()).unwrap();
    assert_eq!(state.settings().unwrap().color_theme, "light");

    let mut settings = Settings::default();
    settings.color_theme = "dark".to_owned();
    state.save_settings(settings).unwrap();
    assert_eq!(
        AppState::load(app_data.path())
            .unwrap()
            .settings()
            .unwrap()
            .color_theme,
        "dark"
    );

    let mut invalid = Settings::default();
    invalid.color_theme = "neon".to_owned();
    assert!(state.save_settings(invalid).is_err());

    for theme in viewera_lib::models::COLOR_THEMES {
        let mut settings = Settings::default();
        settings.color_theme = (*theme).to_owned();
        state.save_settings(settings.clone()).unwrap();
        assert_eq!(
            AppState::load(app_data.path()).unwrap().settings().unwrap(),
            settings
        );
    }

    let mut invalid_width = Settings::default();
    invalid_width.sidebar_width = 80;
    assert!(state.save_settings(invalid_width).is_err());

    // A settings.json written before the theme option existed must still load.
    std::fs::write(
        app_data.path().join("settings.json"),
        r#"{"version":1,"previewPanelWidth":520,"showHiddenFiles":false,"maxTextPreviewBytes":5242880,"maxPreviewCacheBytes":1073741824}"#,
    )
    .unwrap();
    let legacy = AppState::load(app_data.path()).unwrap().settings().unwrap();
    assert_eq!(legacy.color_theme, "light");
    assert_eq!(legacy.sidebar_width, 220);
}

#[test]
fn persists_favorites_and_recent_file_references_without_metadata() {
    let app_data = tempfile::tempdir().unwrap();
    let selected_root = tempfile::tempdir().unwrap();
    std::fs::write(selected_root.path().join("notes.txt"), "file contents").unwrap();
    let state = AppState::load(app_data.path()).unwrap();
    let root = state.add_root(selected_root.path()).unwrap();

    state.set_favorite(&root.id, "notes.txt", true).unwrap();
    state.record_recent(&root.id, "notes.txt").unwrap();

    let restored = AppState::load(app_data.path()).unwrap();
    let favorite = restored.favorites().unwrap();
    let recent = restored.recent_files().unwrap();
    assert_eq!(favorite.len(), 1);
    assert_eq!(favorite[0].root_id, root.id);
    assert_eq!(favorite[0].relative_path, "notes.txt");
    assert!(!favorite[0].timestamp.is_empty());
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0].root_id, root.id);
    assert_eq!(recent[0].relative_path, "notes.txt");
    assert!(!recent[0].timestamp.is_empty());
}

#[test]
fn updates_recent_order_and_removes_unfavorited_references() {
    let app_data = tempfile::tempdir().unwrap();
    let selected_root = tempfile::tempdir().unwrap();
    let state = AppState::load(app_data.path()).unwrap();
    let root = state.add_root(selected_root.path()).unwrap();

    state.set_favorite(&root.id, "a.txt", true).unwrap();
    state.set_favorite(&root.id, "a.txt", false).unwrap();
    state.record_recent(&root.id, "a.txt").unwrap();
    state.record_recent(&root.id, "b.txt").unwrap();
    state.record_recent(&root.id, "a.txt").unwrap();

    assert!(state.favorites().unwrap().is_empty());
    let recent = state.recent_files().unwrap();
    assert_eq!(
        recent
            .iter()
            .map(|item| item.relative_path.as_str())
            .collect::<Vec<_>>(),
        ["a.txt", "b.txt"]
    );
}

#[test]
fn replaces_and_cancels_active_content_searches_without_cancelling_a_newer_search() {
    let app_data = tempfile::tempdir().unwrap();
    let state = AppState::load(app_data.path()).unwrap();

    let first = state.begin_content_search("search-1").unwrap();
    let second = state.begin_content_search("search-2").unwrap();

    assert!(first.load(std::sync::atomic::Ordering::Relaxed));
    assert!(!state.cancel_content_search("search-1").unwrap());
    assert!(!second.load(std::sync::atomic::Ordering::Relaxed));
    assert!(state.cancel_content_search("search-2").unwrap());
    state.finish_content_search("search-2", &second).unwrap();
}
