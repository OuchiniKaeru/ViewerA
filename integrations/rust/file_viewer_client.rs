use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use uuid::Uuid;

const MAX_PAYLOAD_BYTES: usize = 1024 * 1024;
const MAX_MESSAGE_BYTES: usize = 32 * 1024;
const MAX_ARTIFACTS: usize = 20;
const FINAL_RECEIPT_STATUSES: [&str; 3] = ["opened", "rejected", "failed"];

#[derive(Clone, Debug)]
pub struct FileViewerClient {
    executable: PathBuf,
    data_directory: PathBuf,
    receipt_timeout: Duration,
    retention: Duration,
}

#[derive(Debug)]
pub struct HandoffResult {
    pub request_id: String,
    pub payload_path: PathBuf,
    pub receipt_path: PathBuf,
    pub process_id: Option<u32>,
    pub receipt: Option<Value>,
}

impl FileViewerClient {
    pub fn new(executable: impl Into<PathBuf>, data_directory: impl Into<PathBuf>) -> Self {
        Self {
            executable: executable.into(),
            data_directory: data_directory.into(),
            receipt_timeout: Duration::from_secs(10),
            retention: Duration::from_secs(7 * 24 * 60 * 60),
        }
    }

    pub fn from_environment(executable: impl Into<PathBuf>) -> Result<Self> {
        Ok(Self::new(executable, default_data_directory()?))
    }

    pub fn with_receipt_timeout(mut self, timeout: Duration) -> Result<Self> {
        if timeout.is_zero() {
            bail!("receipt timeout must be positive");
        }
        self.receipt_timeout = timeout;
        Ok(self)
    }

    pub fn with_retention(mut self, retention: Duration) -> Self {
        self.retention = retention;
        self
    }

    pub fn open_files(
        &self,
        paths: &[PathBuf],
        source_platform: &str,
        agent_id: &str,
        run_id: Option<&str>,
        message: Option<&str>,
        wait_for_receipt: bool,
    ) -> Result<HandoffResult> {
        let payload = build_payload(paths, source_platform, agent_id, run_id, message)?;
        let request_id = payload["request_id"]
            .as_str()
            .context("generated Agent Return request ID is invalid")?
            .to_owned();
        let requests_directory = self.data_directory.join("agent-inbox/requests");
        let receipts_directory = self.data_directory.join("agent-inbox/receipts");
        ensure_private_directory(&self.data_directory)?;
        ensure_private_directory(&self.data_directory.join("agent-inbox"))?;
        ensure_private_directory(&requests_directory)?;
        ensure_private_directory(&receipts_directory)?;
        self.cleanup_expired(&requests_directory, &receipts_directory)?;

        let payload_path = requests_directory.join(format!("{request_id}.json"));
        let receipt_path = receipts_directory.join(format!("{request_id}.receipt.json"));
        write_payload(&payload_path, &payload)?;

        let mut child = Command::new(&self.executable)
            .arg("agent-return")
            .arg("--payload")
            .arg(&payload_path)
            .arg("--receipt")
            .arg(&receipt_path)
            .arg("--quiet")
            .spawn()
            .with_context(|| {
                format!(
                    "could not start ViewerA at {}",
                    self.executable.display()
                )
            })?;
        let process_id = Some(child.id());
        let receipt = if wait_for_receipt {
            Some(wait_for_receipt_file(
                &mut child,
                &receipt_path,
                self.receipt_timeout,
            )?)
        } else {
            None
        };
        Ok(HandoffResult {
            request_id,
            payload_path,
            receipt_path,
            process_id,
            receipt,
        })
    }

    fn cleanup_expired(&self, requests_directory: &Path, receipts_directory: &Path) -> Result<()> {
        for directory in [requests_directory, receipts_directory] {
            for entry in fs::read_dir(directory)
                .with_context(|| format!("could not inspect {}", directory.display()))?
            {
                let entry = entry.context("could not inspect handoff file")?;
                let file_name = entry.file_name();
                let Some(file_name) = file_name.to_str() else {
                    continue;
                };
                let id = file_name
                    .strip_suffix(".receipt.json")
                    .or_else(|| file_name.strip_suffix(".json"));
                if id.is_none_or(|value| Uuid::parse_str(value).is_err()) {
                    continue;
                }
                let metadata = fs::symlink_metadata(entry.path())
                    .context("could not inspect handoff file metadata")?;
                if metadata.file_type().is_symlink() || !metadata.is_file() {
                    continue;
                }
                let age = SystemTime::now()
                    .duration_since(metadata.modified().unwrap_or(UNIX_EPOCH))
                    .unwrap_or_default();
                if age > self.retention {
                    fs::remove_file(entry.path())
                        .context("could not remove expired handoff file")?;
                }
            }
        }
        Ok(())
    }
}

fn build_payload(
    paths: &[PathBuf],
    source_platform: &str,
    agent_id: &str,
    run_id: Option<&str>,
    message: Option<&str>,
) -> Result<Value> {
    if source_platform.is_empty() || source_platform.len() > 128 {
        bail!("source_platform must contain 1 to 128 bytes");
    }
    if agent_id.is_empty() || agent_id.len() > 128 {
        bail!("agent_id must contain 1 to 128 bytes");
    }
    if message.is_some_and(|text| text.len() > MAX_MESSAGE_BYTES) {
        bail!("message exceeds 32 KiB");
    }
    if paths.is_empty() || paths.len() > MAX_ARTIFACTS {
        bail!("paths must contain between 1 and 20 files");
    }

    let mut artifacts = Vec::new();
    let mut seen = HashSet::new();
    for candidate in paths {
        let file_path = fs::canonicalize(candidate)
            .with_context(|| format!("artifact is unavailable: {}", candidate.display()))?;
        if !seen.insert(file_path.clone()) {
            continue;
        }
        let metadata = fs::metadata(&file_path)
            .with_context(|| format!("could not inspect {}", file_path.display()))?;
        if !metadata.is_file() {
            bail!("artifact is not a regular file: {}", file_path.display());
        }
        let name = file_path
            .file_name()
            .and_then(|value| value.to_str())
            .context("artifact name is not valid UTF-8")?;
        artifacts.push(json!({
            "artifact_id": Uuid::new_v4().to_string(),
            "kind": "file",
            "path": file_path.to_string_lossy(),
            "name": name,
            "mime_type": mime_type(name),
            "size": metadata.len(),
        }));
    }
    if artifacts.is_empty() {
        bail!("paths must contain at least one unique file");
    }

    let mut payload = json!({
        "schema_version": 1,
        "request_id": Uuid::new_v4().to_string(),
        "source": {
            "platform": source_platform,
            "agent_id": agent_id,
            "run_id": run_id,
        },
        "run": { "status": "completed" },
        "viewer": {
            "action": "open_artifacts",
            "selection": "first",
            "focus": true,
            "open_preview": true,
        },
        "artifacts": artifacts,
    });
    if let Some(message) = message {
        payload["result"] = json!({ "text": message, "content_type": "text" });
    }
    if serde_json::to_vec(&payload)?.len() > MAX_PAYLOAD_BYTES {
        bail!("Agent Return payload exceeds 1 MiB");
    }
    Ok(payload)
}

fn mime_type(name: &str) -> &'static str {
    match Path::new(name)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "csv" => "text/csv",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "json" => "application/json",
        "md" => "text/markdown",
        "pdf" => "application/pdf",
        "png" => "image/png",
        "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        "txt" => "text/plain",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        _ => "application/octet-stream",
    }
}

fn ensure_private_directory(directory: &Path) -> Result<()> {
    fs::create_dir_all(directory)
        .with_context(|| format!("could not create {}", directory.display()))?;
    let metadata = fs::symlink_metadata(directory)
        .with_context(|| format!("could not inspect {}", directory.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        bail!(
            "handoff path is not a regular directory: {}",
            directory.display()
        );
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(directory, fs::Permissions::from_mode(0o700))
            .with_context(|| format!("could not protect {}", directory.display()))?;
    }
    Ok(())
}

fn write_payload(path: &Path, payload: &Value) -> Result<()> {
    let temporary_path = path.with_extension(format!("{}.tmp", Uuid::new_v4()));
    let bytes = serde_json::to_vec(payload)?;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(&temporary_path)
        .context("could not create temporary Agent Return payload")?;
    let write_result = file.write_all(&bytes).and_then(|()| file.sync_all());
    drop(file);
    if let Err(error) = write_result {
        let _ = fs::remove_file(&temporary_path);
        return Err(error).context("could not write Agent Return payload");
    }
    if let Err(error) = fs::rename(&temporary_path, path) {
        let _ = fs::remove_file(&temporary_path);
        return Err(error).context("could not publish Agent Return payload");
    }
    Ok(())
}

fn wait_for_receipt_file(child: &mut Child, path: &Path, timeout: Duration) -> Result<Value> {
    let deadline = std::time::Instant::now() + timeout;
    while std::time::Instant::now() < deadline {
        match fs::symlink_metadata(path) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() || !metadata.is_file() {
                    bail!("ViewerA receipt must be a regular file");
                }
                if metadata.len() > MAX_PAYLOAD_BYTES as u64 {
                    bail!("ViewerA receipt exceeds the size limit");
                }
                if let Ok(bytes) = fs::read(path) {
                    if let Ok(receipt) = serde_json::from_slice::<Value>(&bytes) {
                        if let Some(status) = receipt["status"].as_str()
                            && FINAL_RECEIPT_STATUSES.contains(&status)
                        {
                            if status != "opened" {
                                bail!("ViewerA returned {status}: {}", receipt["errors"]);
                            }
                            return Ok(receipt);
                        }
                    }
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error).context("could not read ViewerA receipt"),
        }
        if let Some(status) = child
            .try_wait()
            .context("could not inspect ViewerA process")?
            && !status.success()
        {
            bail!(
                "ViewerA exited with status {} before opening the request",
                status
            );
        }
        thread::sleep(Duration::from_millis(50));
    }
    bail!("ViewerA did not finish the request before the receipt timeout")
}

fn default_data_directory() -> Result<PathBuf> {
    let application_directory = "com.fileviewer.desktop";
    if cfg!(windows) {
        return std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("USERPROFILE")
                    .map(|home| PathBuf::from(home).join("AppData/Roaming"))
            })
            .map(|base| base.join(application_directory))
            .context("Windows application data directory is unavailable");
    }
    if cfg!(target_os = "macos") {
        return std::env::var_os("HOME")
            .map(|home| PathBuf::from(home).join("Library/Application Support"))
            .map(|base| base.join(application_directory))
            .context("macOS application data directory is unavailable");
    }
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
        .map(|base| base.join(application_directory))
        .context("Linux application data directory is unavailable")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_canonical_payload_and_deduplicates_files() {
        let directory = std::env::temp_dir().join(format!("file-viewer-rust-{}", Uuid::new_v4()));
        fs::create_dir_all(&directory).unwrap();
        let artifact = directory.join("report.txt");
        fs::write(&artifact, "report").unwrap();

        let payload = build_payload(
            &[artifact.clone(), artifact.clone()],
            "rust-test",
            "test-agent",
            Some("run-1"),
            Some("Completed"),
        )
        .unwrap();

        assert_eq!(payload["artifacts"].as_array().unwrap().len(), 1);
        assert_eq!(payload["run"]["status"], "completed");
        assert_eq!(payload["result"]["text"], "Completed");
        assert!(Uuid::parse_str(payload["request_id"].as_str().unwrap()).is_ok());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn rejects_directories_and_empty_artifact_lists() {
        let directory = std::env::temp_dir().join(format!("file-viewer-rust-{}", Uuid::new_v4()));
        fs::create_dir_all(&directory).unwrap();
        assert!(build_payload(&[], "rust-test", "agent", None, None).is_err());
        assert!(build_payload(&[directory.clone()], "rust-test", "agent", None, None).is_err());
        fs::remove_dir_all(directory).unwrap();
    }
}
