use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RootEntry {
    pub id: String,
    pub label: String,
    pub path: String,
    pub added_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileReference {
    pub root_id: String,
    pub relative_path: String,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FilePage {
    pub items: Vec<FileEntry>,
    pub page: usize,
    pub page_size: usize,
    pub total: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ZipEntry {
    pub name: String,
    pub compressed_size: u64,
    pub uncompressed_size: u64,
    pub is_dir: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentSearchHit {
    pub file: FileEntry,
    pub snippet: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentSearchResults {
    pub items: Vec<ContentSearchHit>,
    pub total: usize,
    pub scanned_files: usize,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewDescriptor {
    pub root_id: String,
    pub relative_path: String,
    pub file_path: String,
    pub preview_type: String,
    pub mime_type: String,
    pub size: u64,
    pub supports_range: bool,
    pub warning_codes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextChunk {
    pub text: String,
    pub offset: u64,
    pub total_bytes: u64,
    pub truncated: bool,
    pub encoding: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TablePage {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub page: usize,
    pub page_size: usize,
    pub has_more: bool,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkbookDescriptor {
    pub sheets: Vec<WorksheetDescriptor>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorksheetDescriptor {
    pub id: String,
    pub name: String,
    pub start_row: u32,
    pub start_column: u32,
    pub row_count: u32,
    pub column_count: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorksheetRange {
    pub sheet_id: String,
    pub start_row: u32,
    pub start_column: u32,
    pub rows: Vec<Vec<String>>,
    pub styles: Vec<WorksheetCellStyle>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorksheetCellStyle {
    pub row: u32,
    pub column: u32,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikethrough: bool,
    pub font_color: Option<String>,
    pub fill_color: Option<String>,
    pub horizontal_alignment: Option<String>,
    pub vertical_alignment: Option<String>,
    pub wrap_text: bool,
    pub border_top: Option<String>,
    pub border_right: Option<String>,
    pub border_bottom: Option<String>,
    pub border_left: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub version: u32,
    pub preview_panel_width: u32,
    #[serde(default = "default_sidebar_width")]
    pub sidebar_width: u32,
    pub show_hidden_files: bool,
    pub max_text_preview_bytes: usize,
    pub max_preview_cache_bytes: u64,
    #[serde(default = "default_color_theme")]
    pub color_theme: String,
}

fn default_color_theme() -> String {
    "light".to_owned()
}

fn default_sidebar_width() -> u32 {
    220
}

pub const COLOR_THEMES: &[&str] = &[
    "light",
    "dark",
    "lite-pink",
    "light-orange",
    "light-green",
    "light-purple",
    "dark-green",
    "dark-pink",
    "dark-orange",
    "dark-yellow",
];

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: 1,
            preview_panel_width: 520,
            sidebar_width: default_sidebar_width(),
            show_hidden_files: false,
            max_text_preview_bytes: 5 * 1024 * 1024,
            max_preview_cache_bytes: 1024 * 1024 * 1024,
            color_theme: default_color_theme(),
        }
    }
}
