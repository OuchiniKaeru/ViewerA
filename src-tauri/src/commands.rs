use std::path::Path;

use serde::Serialize;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_dialog::{DialogExt, FilePath};

use crate::agent_return::{AgentReceipt, AgentResultView, AgentReturnState};
use crate::files;
use crate::models::{
    ContentSearchResults, FileEntry, FilePage, FileReference, PreviewDescriptor, RootEntry,
    Settings, TablePage, TextChunk, WorkbookDescriptor, WorksheetRange, ZipEntry,
};
use crate::storage::AppState;

#[tauri::command]
pub fn list_roots(state: State<'_, AppState>) -> Result<Vec<RootEntry>, String> {
    state.roots().map_err(display_error)
}

#[tauri::command]
pub async fn select_root_folder(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Option<RootEntry>, String> {
    let selected = app
        .dialog()
        .file()
        .set_title("Select a folder to browse")
        .blocking_pick_folder();
    let Some(selected) = selected else {
        return Ok(None);
    };
    let path = match selected {
        FilePath::Path(path) => path,
        FilePath::Url(_) => return Err("Only local folders are supported.".to_owned()),
    };
    let root = state.add_root(&path).map_err(display_error)?;
    app.asset_protocol_scope()
        .allow_directory(Path::new(&root.path), true)
        .map_err(display_error)?;
    Ok(Some(root))
}

#[tauri::command]
pub fn remove_root(
    app: AppHandle,
    state: State<'_, AppState>,
    root_id: String,
) -> Result<bool, String> {
    let root = state.root(&root_id).map_err(display_error)?;
    app.asset_protocol_scope()
        .forbid_directory(Path::new(&root.path), true)
        .map_err(display_error)?;
    state.remove_root(&root_id).map_err(display_error)
}

#[tauri::command]
pub fn list_directory(
    state: State<'_, AppState>,
    root_id: String,
    relative_path: String,
    page: usize,
    page_size: usize,
    query: String,
    sort_by: String,
    sort_order: String,
) -> Result<FilePage, String> {
    let root = state.root(&root_id).map_err(display_error)?;
    let settings = state.settings().map_err(display_error)?;
    files::list_directory(
        &root,
        &relative_path,
        page,
        page_size,
        settings.show_hidden_files,
        &query,
        &sort_by,
        &sort_order,
    )
    .map_err(display_error)
}

#[tauri::command]
pub fn get_file_metadata(
    state: State<'_, AppState>,
    root_id: String,
    relative_path: String,
) -> Result<FileEntry, String> {
    let root = state.root(&root_id).map_err(display_error)?;
    files::file_metadata(&root, &relative_path).map_err(display_error)
}

#[tauri::command]
pub fn list_zip_entries(
    state: State<'_, AppState>,
    root_id: String,
    relative_path: String,
) -> Result<Vec<ZipEntry>, String> {
    let root = state.root(&root_id).map_err(display_error)?;
    files::list_zip_entries(&root, &relative_path).map_err(display_error)
}

#[tauri::command]
pub async fn search_file_contents(
    state: State<'_, AppState>,
    root_id: String,
    relative_path: String,
    query: String,
    search_id: String,
) -> Result<ContentSearchResults, String> {
    let app_state = state.inner();
    let root = app_state.root(&root_id).map_err(display_error)?;
    let settings = app_state.settings().map_err(display_error)?;
    let cancellation = app_state
        .begin_content_search(&search_id)
        .map_err(display_error)?;
    let search_cancellation = cancellation.clone();
    let result = tokio::task::spawn_blocking(move || {
        files::search_file_contents(
            &root,
            &relative_path,
            &query,
            settings.show_hidden_files,
            &search_cancellation,
        )
    })
    .await
    .map_err(|error| format!("content search task failed: {error}"))
    .and_then(|result| result.map_err(display_error));
    app_state
        .finish_content_search(&search_id, &cancellation)
        .map_err(display_error)?;
    result
}

#[tauri::command]
pub fn cancel_content_search(
    state: State<'_, AppState>,
    search_id: String,
) -> Result<bool, String> {
    state
        .cancel_content_search(&search_id)
        .map_err(display_error)
}

#[tauri::command]
pub fn get_favorites(state: State<'_, AppState>) -> Result<Vec<FileReference>, String> {
    state.favorites().map_err(display_error)
}

#[tauri::command]
pub fn get_recent_files(state: State<'_, AppState>) -> Result<Vec<FileReference>, String> {
    state.recent_files().map_err(display_error)
}

#[tauri::command]
pub fn set_file_favorite(
    state: State<'_, AppState>,
    root_id: String,
    relative_path: String,
    is_favorite: bool,
) -> Result<(), String> {
    let root = state.root(&root_id).map_err(display_error)?;
    files::file_metadata(&root, &relative_path).map_err(display_error)?;
    state
        .set_favorite(&root_id, &relative_path, is_favorite)
        .map_err(display_error)
}

#[tauri::command]
pub fn record_recent_file(
    state: State<'_, AppState>,
    root_id: String,
    relative_path: String,
) -> Result<(), String> {
    let root = state.root(&root_id).map_err(display_error)?;
    let file = files::file_metadata(&root, &relative_path).map_err(display_error)?;
    if file.is_dir {
        return Err("recent entries must be files".to_owned());
    }
    state
        .record_recent(&root_id, &relative_path)
        .map_err(display_error)
}

#[tauri::command]
pub fn get_preview_descriptor(
    state: State<'_, AppState>,
    root_id: String,
    relative_path: String,
) -> Result<PreviewDescriptor, String> {
    let root = state.root(&root_id).map_err(display_error)?;
    files::preview_descriptor(&root, &relative_path).map_err(display_error)
}

#[tauri::command]
pub fn read_text_preview(
    state: State<'_, AppState>,
    root_id: String,
    relative_path: String,
    offset: u64,
    limit: usize,
) -> Result<TextChunk, String> {
    let root = state.root(&root_id).map_err(display_error)?;
    let settings = state.settings().map_err(display_error)?;
    files::read_text_preview(
        &root,
        &relative_path,
        offset,
        limit,
        settings.max_text_preview_bytes,
    )
    .map_err(display_error)
}

#[tauri::command]
pub fn read_csv_preview(
    state: State<'_, AppState>,
    root_id: String,
    relative_path: String,
    page: usize,
    page_size: usize,
    has_header: bool,
) -> Result<TablePage, String> {
    let root = state.root(&root_id).map_err(display_error)?;
    files::read_csv_preview(&root, &relative_path, page, page_size, has_header, 5000)
        .map_err(display_error)
}

#[tauri::command]
pub fn read_workbook(
    state: State<'_, AppState>,
    root_id: String,
    relative_path: String,
) -> Result<WorkbookDescriptor, String> {
    let root = state.root(&root_id).map_err(display_error)?;
    files::read_workbook(&root, &relative_path).map_err(display_error)
}

#[tauri::command]
pub fn read_worksheet_range(
    state: State<'_, AppState>,
    root_id: String,
    relative_path: String,
    sheet_id: String,
    start_row: u32,
    start_column: u32,
    row_count: usize,
    column_count: usize,
) -> Result<WorksheetRange, String> {
    let root = state.root(&root_id).map_err(display_error)?;
    files::read_worksheet_range(
        &root,
        &relative_path,
        &sheet_id,
        start_row,
        start_column,
        row_count,
        column_count,
    )
    .map_err(display_error)
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Result<Settings, String> {
    state.settings().map_err(display_error)
}

#[tauri::command]
pub fn save_settings(state: State<'_, AppState>, settings: Settings) -> Result<(), String> {
    state.save_settings(settings).map_err(display_error)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentArtifactPreview {
    pub id: String,
    pub root_id: String,
    pub relative_path: String,
    pub name: String,
    pub extension: Option<String>,
    pub mime_type: String,
    pub size: u64,
    pub modified_at: Option<String>,
    pub is_dir: bool,
    pub preview_type: String,
    pub preview_supported: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentFolderLocation {
    pub root: RootEntry,
    pub relative_path: String,
}

#[tauri::command]
pub fn get_agent_results(
    state: State<'_, AgentReturnState>,
    storage: State<'_, AppState>,
) -> Result<Vec<AgentResultView>, String> {
    let roots = storage.roots().map_err(display_error)?;
    state.results(&roots).map_err(display_error)
}

#[tauri::command]
pub fn dismiss_agent_result(
    state: State<'_, AgentReturnState>,
    request_id: String,
) -> Result<bool, String> {
    state.dismiss(&request_id).map_err(display_error)
}

#[tauri::command]
pub fn confirm_agent_result_root(
    state: State<'_, AgentReturnState>,
    storage: State<'_, AppState>,
    request_id: String,
    root_id: String,
) -> Result<RootEntry, String> {
    let root = storage.root(&root_id).map_err(display_error)?;
    let roots = storage.roots().map_err(display_error)?;
    state
        .allow_root(&request_id, Path::new(&root.path), &roots)
        .map_err(display_error)
}

#[tauri::command]
pub fn open_agent_artifact(
    app: AppHandle,
    state: State<'_, AgentReturnState>,
    storage: State<'_, AppState>,
    request_id: String,
    artifact_id: String,
    root_id: String,
) -> Result<AgentArtifactPreview, String> {
    let root = storage.root(&root_id).map_err(display_error)?;
    let relative_path = state
        .artifact_relative_path(&request_id, &artifact_id, &root)
        .map_err(display_error)?;
    let descriptor = files::preview_descriptor(&root, &relative_path).map_err(display_error)?;
    app.asset_protocol_scope()
        .allow_directory(Path::new(&root.path), true)
        .map_err(display_error)?;
    let name = Path::new(&relative_path)
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "Agent Return artifact name is invalid.".to_owned())?
        .to_owned();
    let extension = Path::new(&name)
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase);
    Ok(AgentArtifactPreview {
        id: format!("{}:{relative_path}", root.id),
        root_id: root.id,
        relative_path,
        name,
        extension,
        mime_type: descriptor.mime_type,
        size: descriptor.size,
        modified_at: None,
        is_dir: false,
        preview_supported: descriptor.preview_type != "unsupported",
        preview_type: descriptor.preview_type,
    })
}

#[tauri::command]
pub fn open_agent_artifact_folder(
    app: AppHandle,
    state: State<'_, AgentReturnState>,
    storage: State<'_, AppState>,
    request_id: String,
    artifact_id: String,
    root_id: String,
) -> Result<AgentFolderLocation, String> {
    let root = storage.root(&root_id).map_err(display_error)?;
    let relative_path = state
        .artifact_parent_relative_path(&request_id, &artifact_id, &root)
        .map_err(display_error)?;
    app.asset_protocol_scope()
        .allow_directory(Path::new(&root.path), true)
        .map_err(display_error)?;
    Ok(AgentFolderLocation {
        root,
        relative_path,
    })
}

#[tauri::command]
pub fn mark_agent_artifact_opened(
    state: State<'_, AgentReturnState>,
    request_id: String,
    artifact_id: String,
) -> Result<AgentReceipt, String> {
    state
        .mark_opened(&request_id, &artifact_id)
        .map_err(display_error)
}

#[tauri::command]
pub fn mark_agent_artifact_failed(
    state: State<'_, AgentReturnState>,
    request_id: String,
    artifact_id: String,
    error: String,
) -> Result<AgentReceipt, String> {
    state
        .mark_failed(&request_id, &artifact_id, &error)
        .map_err(display_error)
}

fn display_error(error: impl std::fmt::Display) -> String {
    error.to_string()
}
