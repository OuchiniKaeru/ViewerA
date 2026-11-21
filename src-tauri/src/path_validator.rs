use std::path::{Component, Path, PathBuf};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum PathValidationError {
    #[error("the requested path must be relative to a registered root")]
    InvalidRelativePath,
    #[error("the registered root is unavailable")]
    RootUnavailable,
    #[error("symbolic links are not allowed")]
    SymbolicLinkNotAllowed,
    #[error("the requested path resolves outside the registered root")]
    OutsideRoot,
    #[error("file system access failed: {0}")]
    Io(#[from] std::io::Error),
}

pub fn validate_relative_path(
    root: &Path,
    relative_path: &str,
) -> Result<PathBuf, PathValidationError> {
    if relative_path.contains('\0')
        || relative_path.starts_with("\\\\")
        || has_windows_drive_prefix(relative_path)
    {
        return Err(PathValidationError::InvalidRelativePath);
    }

    let root = root
        .canonicalize()
        .map_err(|_| PathValidationError::RootUnavailable)?;
    if !root.is_dir() {
        return Err(PathValidationError::RootUnavailable);
    }

    let requested = Path::new(relative_path);
    if requested.is_absolute() {
        return Err(PathValidationError::InvalidRelativePath);
    }

    let mut resolved = root.clone();
    for component in requested.components() {
        match component {
            Component::Normal(name) => {
                resolved.push(name);
                let metadata = std::fs::symlink_metadata(&resolved)?;
                if metadata.file_type().is_symlink() {
                    return Err(PathValidationError::SymbolicLinkNotAllowed);
                }
            }
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(PathValidationError::InvalidRelativePath);
            }
        }
    }

    let canonical = resolved.canonicalize()?;
    if !canonical.starts_with(&root) {
        return Err(PathValidationError::OutsideRoot);
    }
    Ok(canonical)
}

fn has_windows_drive_prefix(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}
