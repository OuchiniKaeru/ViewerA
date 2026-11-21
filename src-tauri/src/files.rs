use std::cmp::Ordering;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};
use std::time::SystemTime;

use anyhow::{Context, Result, bail};
use calamine::Reader;
use chrono::{DateTime, Utc};

use crate::models::{
    ContentSearchHit, ContentSearchResults, FileEntry, FilePage, PreviewDescriptor, RootEntry,
    TablePage, TextChunk, WorkbookDescriptor, WorksheetCellStyle, WorksheetDescriptor,
    WorksheetRange, ZipEntry,
};
use crate::office::{OfficePackageLimits, validate_package};
use crate::path_validator::validate_relative_path;

const MAX_PAGE_SIZE: usize = 200;
const MAX_CSV_ROWS_PER_PAGE: usize = 200;
const MAX_ZIP_FILE_BYTES: u64 = 100 * 1024 * 1024;
const MAX_ZIP_ENTRIES: usize = 1000;
const MAX_ZIP_UNCOMPRESSED_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_ZIP_ENTRY_NAME_BYTES: usize = 4096;
const MAX_SEARCH_QUERY_BYTES: usize = 256;
const MAX_SEARCH_ENTRIES: usize = 20_000;
const MAX_SEARCH_FILES: usize = 5_000;
const MAX_SEARCH_RESULTS: usize = 500;
const MAX_SEARCH_BYTES: u64 = 100 * 1024 * 1024;
const MAX_SEARCH_FILE_BYTES: u64 = 5 * 1024 * 1024;

pub fn preview_type_for_extension(extension: &str) -> &'static str {
    match extension
        .trim_start_matches('.')
        .to_ascii_lowercase()
        .as_str()
    {
        "pdf" => "pdf",
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" => "image",
        "svg" => "svg",
        "txt" | "log" | "rs" | "py" | "js" | "jsx" | "ts" | "tsx" | "css" | "c" | "cc"
        | "cpp" | "cs" | "h" | "hpp" | "java" | "go" | "kt" | "kts" | "php" | "rb" | "scala"
        | "sh" | "bash" | "zsh" | "sql" | "swift" | "toml" | "xml" | "ini" | "cfg" | "conf"
        | "env" | "properties" | "plist" => "text",
        "html" | "htm" => "html",
        "mhtml" | "mht" => "mhtml",
        "json" | "jsonc" => "json",
        "yaml" | "yml" => "yaml",
        "md" | "markdown" => "markdown",
        "csv" => "csv",
        "xlsx" | "xls" | "xlsb" | "ods" => "spreadsheet",
        "docx" => "docx",
        "pptx" => "pptx",
        "mp3" | "wav" | "ogg" | "oga" | "flac" | "m4a" | "aac" => "audio",
        "mp4" | "m4v" | "webm" | "ogv" | "mov" | "mkv" => "video",
        "zip" => "zip",
        _ => "unsupported",
    }
}

pub fn mime_type_for_extension(extension: Option<&str>, is_dir: bool) -> String {
    if is_dir {
        return "inode/directory".to_owned();
    }

    match extension.unwrap_or_default().to_ascii_lowercase().as_str() {
        "pdf" => "application/pdf",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "svg" => "image/svg+xml",
        "txt" | "log" | "c" | "cc" | "cpp" | "cs" | "h" | "hpp" | "java" | "go" | "kt" | "kts"
        | "php" | "rb" | "rs" | "scala" | "sh" | "bash" | "zsh" | "sql" | "swift" | "toml"
        | "xml" | "ini" | "cfg" | "conf" | "env" | "properties" | "plist" => "text/plain",
        "json" | "jsonc" => "application/json",
        "yaml" | "yml" => "application/yaml",
        "md" | "markdown" => "text/markdown",
        "csv" => "text/csv",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "xls" => "application/vnd.ms-excel",
        "xlsb" => "application/vnd.ms-excel.sheet.binary.macroEnabled.12",
        "ods" => "application/vnd.oasis.opendocument.spreadsheet",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "ogg" | "oga" => "audio/ogg",
        "flac" => "audio/flac",
        "m4a" => "audio/mp4",
        "aac" => "audio/aac",
        "mp4" | "m4v" => "video/mp4",
        "webm" => "video/webm",
        "ogv" => "video/ogg",
        "mov" => "video/quicktime",
        "mkv" => "video/x-matroska",
        "zip" => "application/zip",
        "html" | "htm" => "text/html",
        "mhtml" | "mht" => "multipart/related",
        "css" => "text/css",
        "js" => "text/javascript",
        "py" | "ts" | "tsx" | "jsx" => "text/plain",
        _ => "application/octet-stream",
    }
    .to_owned()
}

pub fn list_directory(
    root: &RootEntry,
    relative_path: &str,
    page: usize,
    page_size: usize,
    show_hidden: bool,
    query: &str,
    sort_by: &str,
    sort_order: &str,
) -> Result<FilePage> {
    let directory = validate_relative_path(Path::new(&root.path), relative_path)
        .context("directory path is not available within the selected root")?;
    if !directory.is_dir() {
        bail!("the requested path is not a directory");
    }

    let mut entries = Vec::new();
    let query = query.trim().to_lowercase();
    for entry in std::fs::read_dir(&directory).context("could not read the selected directory")? {
        let entry = entry.context("could not read a directory entry")?;
        let file_type = entry
            .file_type()
            .context("could not inspect a directory entry")?;
        if file_type.is_symlink() {
            continue;
        }

        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| anyhow::anyhow!("the directory contains a non-UTF-8 file name"))?;
        if !show_hidden && name.starts_with('.') {
            continue;
        }
        if !query.is_empty() && !name.to_lowercase().contains(&query) {
            continue;
        }
        let metadata = entry
            .metadata()
            .context("could not read directory entry metadata")?;
        let is_dir = file_type.is_dir();
        let extension = if is_dir {
            None
        } else {
            Path::new(&name)
                .extension()
                .and_then(|value| value.to_str())
                .map(str::to_ascii_lowercase)
        };
        let child_path = join_relative_path(relative_path, &name);
        let preview_type = extension
            .as_deref()
            .map(preview_type_for_extension)
            .unwrap_or("unsupported");
        let mime_type = mime_type_for_extension(extension.as_deref(), is_dir);
        let modified_at = metadata.modified().ok().map(format_timestamp);
        entries.push(FileEntry {
            id: format!("{}:{}", root.id, child_path),
            root_id: root.id.clone(),
            relative_path: child_path,
            name,
            extension,
            mime_type,
            size: if is_dir { 0 } else { metadata.len() },
            modified_at,
            is_dir,
            preview_supported: preview_type != "unsupported",
            preview_type: preview_type.to_owned(),
        });
    }

    entries.sort_by(|left, right| compare_entries(left, right, sort_by, sort_order));
    let total = entries.len();
    let page_size = page_size.clamp(1, MAX_PAGE_SIZE);
    let start = page.saturating_mul(page_size).min(total);
    let end = start.saturating_add(page_size).min(total);
    Ok(FilePage {
        items: entries[start..end].to_vec(),
        page,
        page_size,
        total,
    })
}

pub fn file_metadata(root: &RootEntry, relative_path: &str) -> Result<FileEntry> {
    let path = validate_relative_path(Path::new(&root.path), relative_path)
        .context("file path is not available within the selected root")?;
    let metadata = std::fs::metadata(&path).context("could not read file metadata")?;
    let is_dir = metadata.is_dir();
    if !is_dir && !metadata.is_file() {
        bail!("the requested path is not a regular file or directory");
    }

    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| anyhow::anyhow!("file name is not valid UTF-8"))?
        .to_owned();
    let extension = if is_dir {
        None
    } else {
        path.extension()
            .and_then(|value| value.to_str())
            .map(str::to_ascii_lowercase)
    };
    let preview_type = extension
        .as_deref()
        .map(preview_type_for_extension)
        .unwrap_or("unsupported");
    Ok(FileEntry {
        id: format!("{}:{relative_path}", root.id),
        root_id: root.id.clone(),
        relative_path: relative_path.to_owned(),
        name,
        extension: extension.clone(),
        mime_type: mime_type_for_extension(extension.as_deref(), is_dir),
        size: if is_dir { 0 } else { metadata.len() },
        modified_at: metadata.modified().ok().map(format_timestamp),
        is_dir,
        preview_supported: preview_type != "unsupported",
        preview_type: preview_type.to_owned(),
    })
}

pub fn list_zip_entries(root: &RootEntry, relative_path: &str) -> Result<Vec<ZipEntry>> {
    let path = validate_relative_path(Path::new(&root.path), relative_path)
        .context("ZIP path is not available within the selected root")?;
    let metadata = std::fs::metadata(&path).context("could not read ZIP metadata")?;
    if !metadata.is_file() {
        bail!("the requested ZIP path is not a regular file");
    }
    if path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_none_or(|extension| !extension.eq_ignore_ascii_case("zip"))
    {
        bail!("the requested file is not a ZIP archive");
    }
    if metadata.len() > MAX_ZIP_FILE_BYTES {
        bail!("ZIP archive exceeds the compressed file size limit");
    }

    let file = std::fs::File::open(&path).context("could not open ZIP archive")?;
    let mut archive = zip::ZipArchive::new(file).context("file is not a valid ZIP archive")?;
    if archive.len() > MAX_ZIP_ENTRIES {
        bail!("ZIP archive exceeds the entry count limit");
    }

    let mut total_uncompressed = 0_u64;
    let mut entries = Vec::with_capacity(archive.len());
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .context("could not inspect ZIP archive entry")?;
        let name = entry.name();
        if name.len() > MAX_ZIP_ENTRY_NAME_BYTES
            || name.contains('\\')
            || entry.enclosed_name().is_none()
        {
            bail!("ZIP archive contains an unsafe entry path");
        }
        total_uncompressed = total_uncompressed
            .checked_add(entry.size())
            .context("ZIP archive size overflow")?;
        if total_uncompressed > MAX_ZIP_UNCOMPRESSED_BYTES {
            bail!("ZIP archive exceeds the uncompressed size limit");
        }
        entries.push(ZipEntry {
            name: name.to_owned(),
            compressed_size: entry.compressed_size(),
            uncompressed_size: entry.size(),
            is_dir: entry.is_dir(),
        });
    }
    Ok(entries)
}

pub fn search_file_contents(
    root: &RootEntry,
    relative_path: &str,
    query: &str,
    show_hidden: bool,
    cancelled: &AtomicBool,
) -> Result<ContentSearchResults> {
    let query = query.trim();
    if query.is_empty() || query.len() > MAX_SEARCH_QUERY_BYTES {
        bail!("content search query must contain 1 to 256 bytes");
    }
    let search_root = validate_relative_path(Path::new(&root.path), relative_path)
        .context("search path is not available within the selected root")?;
    if !search_root.is_dir() {
        bail!("content search must start in a directory");
    }

    let query = query.to_lowercase();
    let mut directories = vec![relative_path.trim_matches('/').to_owned()];
    let mut hits = Vec::new();
    let mut scanned_files = 0;
    let mut visited_entries = 0;
    let mut scanned_bytes = 0_u64;
    let mut truncated = false;

    'search: while let Some(directory_path) = directories.pop() {
        if cancelled.load(AtomicOrdering::Relaxed) {
            bail!("content search was cancelled");
        }
        let directory = validate_relative_path(Path::new(&root.path), &directory_path)
            .context("search directory is not available within the selected root")?;
        for entry in std::fs::read_dir(directory).context("could not read a search directory")? {
            if cancelled.load(AtomicOrdering::Relaxed) {
                bail!("content search was cancelled");
            }
            visited_entries += 1;
            if visited_entries > MAX_SEARCH_ENTRIES {
                truncated = true;
                break 'search;
            }
            let entry = entry.context("could not read a search directory entry")?;
            let file_type = entry
                .file_type()
                .context("could not inspect a search directory entry")?;
            if file_type.is_symlink() {
                continue;
            }
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| anyhow::anyhow!("a search result has a non-UTF-8 file name"))?;
            if !show_hidden && name.starts_with('.') {
                continue;
            }
            let child_path = join_relative_path(&directory_path, &name);
            if file_type.is_dir() {
                directories.push(child_path);
                continue;
            }
            if !file_type.is_file() {
                continue;
            }

            let extension = Path::new(&name)
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or_default();
            if !matches!(
                preview_type_for_extension(extension),
                "text" | "json" | "yaml" | "markdown" | "csv" | "html" | "mhtml"
            ) {
                continue;
            }
            scanned_files += 1;
            if scanned_files > MAX_SEARCH_FILES {
                truncated = true;
                break 'search;
            }

            let metadata = entry
                .metadata()
                .context("could not inspect a searchable file")?;
            if metadata.len() == 0 {
                continue;
            }
            let remaining_bytes = MAX_SEARCH_BYTES.saturating_sub(scanned_bytes);
            if remaining_bytes == 0 {
                truncated = true;
                break 'search;
            }
            let bytes_to_read = metadata
                .len()
                .min(MAX_SEARCH_FILE_BYTES)
                .min(remaining_bytes);
            let chunk = read_text_preview(
                root,
                &child_path,
                0,
                bytes_to_read as usize,
                bytes_to_read as usize,
            )?;
            scanned_bytes = scanned_bytes.saturating_add(bytes_to_read);
            truncated |= chunk.truncated;
            if let Some(line) = chunk
                .text
                .lines()
                .find(|line| line.to_lowercase().contains(&query))
            {
                hits.push(ContentSearchHit {
                    file: file_metadata(root, &child_path)?,
                    snippet: line.chars().take(240).collect(),
                });
                if hits.len() >= MAX_SEARCH_RESULTS {
                    truncated = true;
                    break 'search;
                }
            }
        }
    }

    hits.sort_by(|left, right| {
        natord::compare_ignore_case(&left.file.relative_path, &right.file.relative_path)
    });
    let total = hits.len();
    Ok(ContentSearchResults {
        items: hits,
        total,
        scanned_files,
        truncated,
    })
}

pub fn preview_descriptor(root: &RootEntry, relative_path: &str) -> Result<PreviewDescriptor> {
    let path = validate_relative_path(Path::new(&root.path), relative_path)
        .context("file path is not available within the selected root")?;
    let metadata = std::fs::metadata(&path).context("could not read file metadata")?;
    if !metadata.is_file() {
        bail!("the requested path is not a file");
    }
    let extension = path.extension().and_then(|value| value.to_str());
    if matches!(
        extension.unwrap_or_default().to_ascii_lowercase().as_str(),
        "docx" | "pptx" | "xlsx" | "ods"
    ) {
        validate_package(&path, &OfficePackageLimits::default())
            .context("Office package failed safety checks")?;
    }
    let preview_type = preview_type_for_extension(extension.unwrap_or_default());
    Ok(PreviewDescriptor {
        root_id: root.id.clone(),
        relative_path: relative_path.to_owned(),
        file_path: path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("the file path is not valid UTF-8"))?
            .to_owned(),
        preview_type: preview_type.to_owned(),
        mime_type: mime_type_for_extension(extension, false),
        size: metadata.len(),
        supports_range: matches!(preview_type, "pdf" | "image" | "svg" | "html" | "mhtml"),
        warning_codes: Vec::new(),
    })
}

pub fn read_text_preview(
    root: &RootEntry,
    relative_path: &str,
    offset: u64,
    requested_limit: usize,
    max_bytes: usize,
) -> Result<TextChunk> {
    let path = validate_relative_path(Path::new(&root.path), relative_path)
        .context("file path is not available within the selected root")?;
    let mut file = std::fs::File::open(&path).context("could not open the text file")?;
    let total_bytes = file.metadata()?.len();
    if offset > total_bytes {
        bail!("text preview offset exceeds the file size");
    }

    let byte_limit = requested_limit.min(max_bytes).max(1);
    file.seek(SeekFrom::Start(offset))?;
    let mut bytes = vec![0; byte_limit];
    let count = file.read(&mut bytes)?;
    bytes.truncate(count);
    let (text, encoding) = match std::str::from_utf8(&bytes) {
        Ok(text) => (text.to_owned(), "utf-8"),
        Err(_) => {
            let (decoded, _, _) = encoding_rs::SHIFT_JIS.decode(&bytes);
            (decoded.into_owned(), "shift_jis")
        }
    };

    Ok(TextChunk {
        text,
        offset,
        total_bytes,
        truncated: offset.saturating_add(count as u64) < total_bytes,
        encoding: encoding.to_owned(),
    })
}

pub fn read_csv_preview(
    root: &RootEntry,
    relative_path: &str,
    page: usize,
    page_size: usize,
    has_header: bool,
    max_rows: usize,
) -> Result<TablePage> {
    let path = validate_relative_path(Path::new(&root.path), relative_path)
        .context("file path is not available within the selected root")?;
    let page_size = page_size.clamp(1, MAX_CSV_ROWS_PER_PAGE);
    let start = page.saturating_mul(page_size);
    let max_rows = max_rows.clamp(1, 100_000);
    if start >= max_rows {
        bail!("CSV row limit has been reached");
    }

    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .from_path(path)
        .context("could not parse the CSV file")?;
    let mut records = reader.records();
    let headers = if has_header {
        records
            .next()
            .transpose()
            .context("could not read CSV header")?
            .map(|record| record.iter().map(str::to_owned).collect())
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    for _ in 0..start {
        if records
            .next()
            .transpose()
            .context("could not read CSV row")?
            .is_none()
        {
            return Ok(TablePage {
                headers,
                rows: Vec::new(),
                page,
                page_size,
                has_more: false,
                truncated: false,
            });
        }
    }

    let mut rows = Vec::new();
    for _ in 0..page_size.min(max_rows - start) {
        let Some(record) = records
            .next()
            .transpose()
            .context("could not read CSV row")?
        else {
            break;
        };
        rows.push(record.iter().map(str::to_owned).collect::<Vec<_>>());
    }
    let has_extra = records
        .next()
        .transpose()
        .context("could not read CSV row")?
        .is_some();
    let reached_limit = start + rows.len() >= max_rows;
    let headers = if has_header {
        headers
    } else {
        let column_count = rows.first().map_or(0, Vec::len);
        (1..=column_count)
            .map(|index| format!("Column {index}"))
            .collect()
    };

    Ok(TablePage {
        headers,
        rows,
        page,
        page_size,
        has_more: has_extra && !reached_limit,
        truncated: has_extra && reached_limit,
    })
}

pub fn read_workbook(root: &RootEntry, relative_path: &str) -> Result<WorkbookDescriptor> {
    let path = validated_office_path(root, relative_path)?;
    let mut workbook =
        calamine::open_workbook_auto(&path).context("could not read the spreadsheet")?;
    let mut sheets = Vec::new();
    for name in workbook.sheet_names() {
        let range = workbook
            .worksheet_range(&name)
            .with_context(|| format!("could not read worksheet {name}"))?;
        let (start_row, start_column, row_count, column_count) = match (range.start(), range.end())
        {
            (Some((start_row, start_column)), Some((end_row, end_column))) => (
                start_row,
                start_column,
                end_row.saturating_add(1),
                end_column.saturating_add(1),
            ),
            _ => (0, 0, 0, 0),
        };
        sheets.push(WorksheetDescriptor {
            id: name.clone(),
            name,
            start_row,
            start_column,
            row_count,
            column_count,
        });
    }
    Ok(WorkbookDescriptor { sheets })
}

pub fn read_worksheet_range(
    root: &RootEntry,
    relative_path: &str,
    sheet_id: &str,
    start_row: u32,
    start_column: u32,
    row_count: usize,
    column_count: usize,
) -> Result<WorksheetRange> {
    const MAX_RANGE_ROWS: usize = 200;
    const MAX_RANGE_COLUMNS: usize = 100;
    if row_count == 0
        || column_count == 0
        || row_count > MAX_RANGE_ROWS
        || column_count > MAX_RANGE_COLUMNS
    {
        bail!("worksheet range exceeds the per-request cell limit");
    }

    let path = validated_office_path(root, relative_path)?;
    let mut workbook =
        calamine::open_workbook_auto(&path).context("could not read the spreadsheet")?;
    let range = workbook
        .worksheet_range(sheet_id)
        .with_context(|| format!("could not read worksheet {sheet_id}"))?;
    let mut rows = Vec::with_capacity(row_count);
    for row in start_row..start_row.saturating_add(row_count as u32) {
        let mut values = Vec::with_capacity(column_count);
        for column in start_column..start_column.saturating_add(column_count as u32) {
            values.push(
                range
                    .get_value((row, column))
                    .map(ToString::to_string)
                    .unwrap_or_default(),
            );
        }
        rows.push(values);
    }
    let style_range = workbook
        .worksheet_style(sheet_id)
        .with_context(|| format!("could not read worksheet styles for {sheet_id}"))?;
    let mut styles = Vec::new();
    for row in start_row..start_row.saturating_add(row_count as u32) {
        for column in start_column..start_column.saturating_add(column_count as u32) {
            if let Some(style) = style_range.get((row, column))
                && let Some(cell_style) = worksheet_cell_style(row, column, style)
            {
                styles.push(cell_style);
            }
        }
    }
    Ok(WorksheetRange {
        sheet_id: sheet_id.to_owned(),
        start_row,
        start_column,
        rows,
        styles,
    })
}

fn worksheet_cell_style(
    row: u32,
    column: u32,
    style: &calamine::Style,
) -> Option<WorksheetCellStyle> {
    let font = style.font.as_ref();
    let fill = style.fill.as_ref();
    let alignment = style.alignment.as_ref();
    let borders = style.borders.as_ref();
    let bold = font.is_some_and(calamine::Font::is_bold);
    let italic = font.is_some_and(calamine::Font::is_italic);
    let underline = font.is_some_and(calamine::Font::has_underline);
    let strikethrough = font.is_some_and(calamine::Font::has_strikethrough);
    let font_color = font.and_then(|font| font.color.map(|color| color.to_string()));
    let fill_color = fill
        .filter(|fill| fill.is_visible())
        .and_then(calamine::Fill::get_color)
        .map(|color| color.to_string());
    let horizontal_alignment = alignment.and_then(|alignment| match alignment.horizontal {
        calamine::HorizontalAlignment::Left => Some("left"),
        calamine::HorizontalAlignment::Center | calamine::HorizontalAlignment::CenterContinuous => {
            Some("center")
        }
        calamine::HorizontalAlignment::Right => Some("right"),
        calamine::HorizontalAlignment::Justify | calamine::HorizontalAlignment::Distributed => {
            Some("justify")
        }
        _ => None,
    });
    let vertical_alignment = alignment.and_then(|alignment| match alignment.vertical {
        calamine::VerticalAlignment::Top => Some("top"),
        calamine::VerticalAlignment::Center => Some("middle"),
        calamine::VerticalAlignment::Bottom => Some("bottom"),
        calamine::VerticalAlignment::Justify | calamine::VerticalAlignment::Distributed => {
            Some("middle")
        }
        _ => None,
    });
    let wrap_text = alignment.is_some_and(|alignment| alignment.wrap_text);
    let border_top = borders.and_then(|borders| css_border(&borders.top));
    let border_right = borders.and_then(|borders| css_border(&borders.right));
    let border_bottom = borders.and_then(|borders| css_border(&borders.bottom));
    let border_left = borders.and_then(|borders| css_border(&borders.left));
    if !bold
        && !italic
        && !underline
        && !strikethrough
        && font_color.is_none()
        && fill_color.is_none()
        && horizontal_alignment.is_none()
        && vertical_alignment.is_none()
        && !wrap_text
        && border_top.is_none()
        && border_right.is_none()
        && border_bottom.is_none()
        && border_left.is_none()
    {
        return None;
    }
    Some(WorksheetCellStyle {
        row,
        column,
        bold,
        italic,
        underline,
        strikethrough,
        font_color,
        fill_color,
        horizontal_alignment: horizontal_alignment.map(str::to_owned),
        vertical_alignment: vertical_alignment.map(str::to_owned),
        wrap_text,
        border_top,
        border_right,
        border_bottom,
        border_left,
    })
}

fn css_border(border: &calamine::Border) -> Option<String> {
    if !border.is_visible() {
        return None;
    }
    let (width, style) = match border.style {
        calamine::BorderStyle::Thin | calamine::BorderStyle::Hair => ("1px", "solid"),
        calamine::BorderStyle::Medium => ("2px", "solid"),
        calamine::BorderStyle::Thick => ("3px", "solid"),
        calamine::BorderStyle::Double => ("3px", "double"),
        calamine::BorderStyle::Dashed
        | calamine::BorderStyle::MediumDashed
        | calamine::BorderStyle::DashDot
        | calamine::BorderStyle::MediumDashDot
        | calamine::BorderStyle::DashDotDot
        | calamine::BorderStyle::MediumDashDotDot
        | calamine::BorderStyle::SlantDashDot => ("1px", "dashed"),
        calamine::BorderStyle::Dotted => ("1px", "dotted"),
        _ => return None,
    };
    let color = border
        .color
        .map(|color| color.to_string())
        .unwrap_or_else(|| "#A0A8B5".to_owned());
    Some(format!("{width} {style} {color}"))
}

fn validated_office_path(root: &RootEntry, relative_path: &str) -> Result<std::path::PathBuf> {
    let path = validate_relative_path(Path::new(&root.path), relative_path)
        .context("file path is not available within the selected root")?;
    let extension = path.extension().and_then(|value| value.to_str());
    if matches!(
        extension.unwrap_or_default().to_ascii_lowercase().as_str(),
        "xlsx" | "ods"
    ) {
        validate_package(&path, &OfficePackageLimits::default())
            .context("spreadsheet package failed safety checks")?;
    }
    let metadata = std::fs::metadata(&path).context("could not inspect spreadsheet metadata")?;
    if metadata.len() > OfficePackageLimits::default().max_compressed_bytes {
        bail!("spreadsheet exceeds the file size limit");
    }
    Ok(path)
}

fn join_relative_path(parent: &str, name: &str) -> String {
    if parent.is_empty() {
        name.to_owned()
    } else {
        format!("{}/{}", parent.trim_end_matches('/'), name)
    }
}

fn compare_entries(
    left: &FileEntry,
    right: &FileEntry,
    sort_by: &str,
    sort_order: &str,
) -> Ordering {
    let mut order = match sort_by {
        "size" => left.size.cmp(&right.size),
        "modifiedAt" | "modified_at" | "modified" => left
            .modified_at
            .as_deref()
            .unwrap_or_default()
            .cmp(&right.modified_at.as_deref().unwrap_or_default()),
        "type" => left
            .preview_type
            .cmp(&right.preview_type)
            .then_with(|| natord::compare_ignore_case(&left.name, &right.name)),
        _ => natord::compare_ignore_case(&left.name, &right.name),
    };
    if sort_order.eq_ignore_ascii_case("desc") {
        order = order.reverse();
    }
    right.is_dir.cmp(&left.is_dir).then_with(|| order)
}

fn format_timestamp(timestamp: SystemTime) -> String {
    DateTime::<Utc>::from(timestamp).to_rfc3339()
}
