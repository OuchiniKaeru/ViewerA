use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::models::{FileReference, RootEntry, Settings};

const RECENT_FILE_LIMIT: usize = 100;
const FAVORITE_LIMIT: usize = 500;

#[derive(Debug)]
pub struct AppState {
    directory: PathBuf,
    roots: Mutex<Vec<RootEntry>>,
    settings: Mutex<Settings>,
    favorites: Mutex<Vec<FileReference>>,
    recent_files: Mutex<Vec<FileReference>>,
    active_content_search: Mutex<Option<(String, Arc<AtomicBool>)>>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct RootsDocument {
    version: u32,
    roots: Vec<RootEntry>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FileReferencesDocument {
    version: u32,
    items: Vec<FileReference>,
}

impl AppState {
    pub fn load(directory: &Path) -> Result<Self> {
        std::fs::create_dir_all(directory).context("could not initialize application storage")?;
        let roots_document: RootsDocument =
            read_json(&directory.join("roots.json"))?.unwrap_or_default();
        if roots_document.version > 1 {
            bail!("root data version is not supported");
        }
        let settings: Settings = read_json(&directory.join("settings.json"))?.unwrap_or_default();
        validate_settings(&settings)?;
        let favorites = read_file_references(&directory.join("favorites.json"))?;
        let recent_files = read_file_references(&directory.join("recent.json"))?;

        Ok(Self {
            directory: directory.to_path_buf(),
            roots: Mutex::new(roots_document.roots),
            settings: Mutex::new(settings),
            favorites: Mutex::new(favorites),
            recent_files: Mutex::new(recent_files),
            active_content_search: Mutex::new(None),
        })
    }

    pub fn roots(&self) -> Result<Vec<RootEntry>> {
        self.roots
            .lock()
            .map(|roots| roots.clone())
            .map_err(|_| anyhow::anyhow!("root list is unavailable"))
    }

    pub fn root(&self, id: &str) -> Result<RootEntry> {
        self.roots
            .lock()
            .map_err(|_| anyhow::anyhow!("root list is unavailable"))?
            .iter()
            .find(|root| root.id == id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("root is not registered"))
    }

    pub fn add_root(&self, path: &Path) -> Result<RootEntry> {
        let path = path
            .canonicalize()
            .context("selected folder does not exist")?;
        if !path.is_dir() {
            bail!("selected path is not a folder");
        }
        let path_text = path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("folder path is not valid UTF-8"))?
            .to_owned();

        let mut roots = self
            .roots
            .lock()
            .map_err(|_| anyhow::anyhow!("root list is unavailable"))?;
        if let Some(root) = roots.iter().find(|root| root.path == path_text) {
            return Ok(root.clone());
        }
        let label = path
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty())
            .unwrap_or(&path_text)
            .to_owned();
        let root = RootEntry {
            id: uuid::Uuid::new_v4().to_string(),
            label,
            path: path_text,
            added_at: chrono::Utc::now().to_rfc3339(),
        };
        roots.push(root.clone());
        self.persist_roots(&roots)?;
        Ok(root)
    }

    pub fn remove_root(&self, id: &str) -> Result<bool> {
        let mut roots = self
            .roots
            .lock()
            .map_err(|_| anyhow::anyhow!("root list is unavailable"))?;
        let previous_count = roots.len();
        roots.retain(|root| root.id != id);
        if roots.len() == previous_count {
            return Ok(false);
        }
        self.persist_roots(&roots)?;
        Ok(true)
    }

    pub fn settings(&self) -> Result<Settings> {
        self.settings
            .lock()
            .map(|settings| settings.clone())
            .map_err(|_| anyhow::anyhow!("settings are unavailable"))
    }

    pub fn favorites(&self) -> Result<Vec<FileReference>> {
        self.favorites
            .lock()
            .map(|items| items.clone())
            .map_err(|_| anyhow::anyhow!("favorites are unavailable"))
    }

    pub fn recent_files(&self) -> Result<Vec<FileReference>> {
        self.recent_files
            .lock()
            .map(|items| items.clone())
            .map_err(|_| anyhow::anyhow!("recent files are unavailable"))
    }

    pub fn set_favorite(
        &self,
        root_id: &str,
        relative_path: &str,
        is_favorite: bool,
    ) -> Result<()> {
        validate_file_reference(root_id, relative_path)?;
        let mut favorites = self
            .favorites
            .lock()
            .map_err(|_| anyhow::anyhow!("favorites are unavailable"))?;
        let mut updated = favorites.clone();
        updated.retain(|item| item.root_id != root_id || item.relative_path != relative_path);
        if is_favorite {
            if updated.len() >= FAVORITE_LIMIT {
                bail!("favorite limit has been reached");
            }
            updated.insert(
                0,
                FileReference {
                    root_id: root_id.to_owned(),
                    relative_path: relative_path.to_owned(),
                    timestamp: chrono::Utc::now().to_rfc3339(),
                },
            );
        }
        save_file_references(&self.directory.join("favorites.json"), &updated)?;
        *favorites = updated;
        Ok(())
    }

    pub fn record_recent(&self, root_id: &str, relative_path: &str) -> Result<()> {
        validate_file_reference(root_id, relative_path)?;
        let mut recent_files = self
            .recent_files
            .lock()
            .map_err(|_| anyhow::anyhow!("recent files are unavailable"))?;
        let mut updated = recent_files.clone();
        updated.retain(|item| item.root_id != root_id || item.relative_path != relative_path);
        updated.insert(
            0,
            FileReference {
                root_id: root_id.to_owned(),
                relative_path: relative_path.to_owned(),
                timestamp: chrono::Utc::now().to_rfc3339(),
            },
        );
        updated.truncate(RECENT_FILE_LIMIT);
        save_file_references(&self.directory.join("recent.json"), &updated)?;
        *recent_files = updated;
        Ok(())
    }

    pub fn begin_content_search(&self, search_id: &str) -> Result<Arc<AtomicBool>> {
        if search_id.is_empty() || search_id.len() > 128 {
            bail!("content search identifier is invalid");
        }
        let mut active = self
            .active_content_search
            .lock()
            .map_err(|_| anyhow::anyhow!("content search state is unavailable"))?;
        if let Some((_, cancellation)) = active.as_ref() {
            cancellation.store(true, Ordering::Relaxed);
        }
        let cancellation = Arc::new(AtomicBool::new(false));
        *active = Some((search_id.to_owned(), cancellation.clone()));
        Ok(cancellation)
    }

    pub fn cancel_content_search(&self, search_id: &str) -> Result<bool> {
        let active = self
            .active_content_search
            .lock()
            .map_err(|_| anyhow::anyhow!("content search state is unavailable"))?;
        if let Some((active_id, cancellation)) = active.as_ref()
            && active_id == search_id
        {
            cancellation.store(true, Ordering::Relaxed);
            return Ok(true);
        }
        Ok(false)
    }

    pub fn finish_content_search(
        &self,
        search_id: &str,
        cancellation: &Arc<AtomicBool>,
    ) -> Result<()> {
        let mut active = self
            .active_content_search
            .lock()
            .map_err(|_| anyhow::anyhow!("content search state is unavailable"))?;
        if active.as_ref().is_some_and(|(active_id, active_token)| {
            active_id == search_id && Arc::ptr_eq(active_token, cancellation)
        }) {
            *active = None;
        }
        Ok(())
    }

    pub fn save_settings(&self, settings: Settings) -> Result<()> {
        validate_settings(&settings)?;
        save_json(&self.directory.join("settings.json"), &settings)?;
        *self
            .settings
            .lock()
            .map_err(|_| anyhow::anyhow!("settings are unavailable"))? = settings;
        Ok(())
    }

    fn persist_roots(&self, roots: &[RootEntry]) -> Result<()> {
        save_json(
            &self.directory.join("roots.json"),
            &RootsDocument {
                version: 1,
                roots: roots.to_vec(),
            },
        )
    }
}

fn validate_file_reference(root_id: &str, relative_path: &str) -> Result<()> {
    if root_id.is_empty()
        || root_id.len() > 128
        || relative_path.is_empty()
        || relative_path.len() > 4096
        || root_id.contains('\0')
        || relative_path.contains('\0')
    {
        bail!("file reference contains invalid values");
    }
    Ok(())
}

fn read_file_references(path: &Path) -> Result<Vec<FileReference>> {
    let document: FileReferencesDocument = read_json(path)?.unwrap_or_default();
    if document.version > 1 {
        bail!("file reference data version is not supported");
    }
    Ok(document.items)
}

fn save_file_references(path: &Path, items: &[FileReference]) -> Result<()> {
    save_json(
        path,
        &FileReferencesDocument {
            version: 1,
            items: items.to_vec(),
        },
    )
}

fn validate_settings(settings: &Settings) -> Result<()> {
    if settings.version != 1
        || !(360..=800).contains(&settings.preview_panel_width)
        || !(160..=400).contains(&settings.sidebar_width)
        || settings.max_text_preview_bytes == 0
        || settings.max_text_preview_bytes > 100 * 1024 * 1024
        || settings.max_preview_cache_bytes > 10 * 1024 * 1024 * 1024
        || !crate::models::COLOR_THEMES.contains(&settings.color_theme.as_str())
    {
        bail!("settings contain unsupported values");
    }
    Ok(())
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<Option<T>> {
    match std::fs::File::open(path) {
        Ok(file) => serde_json::from_reader(file)
            .with_context(|| format!("could not parse {}", path.display()))
            .map(Some),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).with_context(|| format!("could not read {}", path.display())),
    }
}

fn save_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let temporary_path = path.with_extension("tmp");
    let mut temporary = std::fs::File::create(&temporary_path)
        .context("could not create temporary storage file")?;
    serde_json::to_writer_pretty(&mut temporary, value)
        .context("could not serialize application data")?;
    use std::io::Write;
    temporary
        .flush()
        .context("could not flush application data")?;
    temporary
        .sync_all()
        .context("could not synchronize application data")?;

    #[cfg(windows)]
    if path.exists() {
        std::fs::remove_file(path).context("could not replace application data")?;
    }
    std::fs::rename(temporary_path, path).context("could not save application data")
}
