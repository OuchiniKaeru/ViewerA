use std::collections::HashMap;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

pub const MAX_PAYLOAD_BYTES: usize = 1024 * 1024;
pub const MAX_ARTIFACTS: usize = 20;
const MAX_RESULT_TEXT_BYTES: usize = 32 * 1024;
const MAX_SEEN_REQUESTS: usize = 100_000;
const MAX_AGENT_STATE_BYTES: usize = 64 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentReturn {
    pub schema_version: u32,
    pub request_id: String,
    pub source: AgentSource,
    pub run: AgentRun,
    pub viewer: AgentViewer,
    #[serde(default)]
    pub artifacts: Vec<AgentArtifact>,
    #[serde(default)]
    pub result: Option<AgentText>,
    #[serde(default)]
    pub auth: Option<AgentAuth>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentSource {
    pub platform: String,
    #[serde(default)]
    pub agent_id: Option<String>,
    #[serde(default)]
    pub agent_name: Option<String>,
    #[serde(default)]
    pub run_id: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentRun {
    pub status: String,
    #[serde(default)]
    pub completed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentViewer {
    pub action: ViewerAction,
    #[serde(default)]
    pub selection: Option<ViewerSelection>,
    #[serde(default)]
    pub focus: bool,
    #[serde(default)]
    pub open_preview: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewerAction {
    None,
    OpenArtifacts,
    OpenFirst,
    OpenFolder,
    ShowMessage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewerSelection {
    First,
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentArtifact {
    pub artifact_id: String,
    pub kind: String,
    pub path: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub mime_type: Option<String>,
    #[serde(default)]
    pub size: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentText {
    pub text: String,
    #[serde(default = "default_content_type")]
    pub content_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentAuth {
    pub scheme: String,
    pub key_id: String,
    pub issued_at: String,
    pub nonce: String,
    pub signature: String,
}

pub fn validate_payload(bytes: &[u8]) -> Result<AgentReturn> {
    if bytes.is_empty() || bytes.len() > MAX_PAYLOAD_BYTES {
        bail!("Agent Return payload must be between 1 byte and 1 MiB");
    }
    let payload: AgentReturn =
        serde_json::from_slice(bytes).context("Agent Return payload is not valid JSON")?;
    if payload.schema_version != 1 {
        bail!("Agent Return schema version is not supported");
    }
    let request_id = uuid::Uuid::parse_str(&payload.request_id)
        .context("Agent Return request_id must be a UUID")?;
    if request_id.to_string() != payload.request_id.to_ascii_lowercase() {
        bail!("Agent Return request_id must use canonical UUID notation");
    }
    if payload.run.status != "completed" {
        bail!("only completed Agent Runs can be opened");
    }
    if payload.source.platform.is_empty() || payload.source.platform.len() > 128 {
        bail!("Agent Return source platform is invalid");
    }
    if payload.artifacts.len() > MAX_ARTIFACTS {
        bail!("Agent Return exceeds the artifact count limit");
    }
    let mut ids = std::collections::HashSet::new();
    for artifact in &payload.artifacts {
        if artifact.artifact_id.is_empty()
            || artifact.artifact_id.len() > 128
            || !ids.insert(&artifact.artifact_id)
            || artifact.kind != "file"
            || artifact.path.is_empty()
            || artifact.path.len() > 32 * 1024
            || artifact.path.contains('\0')
        {
            bail!("Agent Return contains invalid artifact metadata");
        }
    }
    if matches!(
        payload.viewer.action,
        ViewerAction::OpenArtifacts | ViewerAction::OpenFirst | ViewerAction::OpenFolder
    ) && payload.artifacts.is_empty()
    {
        bail!("Agent Return action requires at least one artifact");
    }
    if payload.viewer.action == ViewerAction::ShowMessage
        && payload
            .result
            .as_ref()
            .is_none_or(|result| result.text.is_empty())
    {
        bail!("show_message requires result.text");
    }
    if let Some(result) = &payload.result {
        if result.text.len() > MAX_RESULT_TEXT_BYTES
            || !matches!(result.content_type.as_str(), "text" | "markdown")
        {
            bail!("Agent Return result text is invalid");
        }
    }
    Ok(payload)
}

#[derive(Debug)]
pub struct AgentReturnState {
    requests_directory: PathBuf,
    receipts_directory: PathBuf,
    pending_path: PathBuf,
    seen_path: PathBuf,
    pending: Mutex<Vec<PendingRequest>>,
    seen: Mutex<HashMap<String, SeenRequest>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PendingRequest {
    payload: AgentReturn,
    received_at: String,
    receipt_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SeenRequest {
    receipt_path: PathBuf,
    received_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentResultView {
    pub request_id: String,
    pub platform: String,
    pub agent_name: Option<String>,
    pub run_id: Option<String>,
    pub completed_at: Option<String>,
    pub action: ViewerAction,
    pub message: Option<String>,
    pub received_at: String,
    pub artifacts: Vec<AgentArtifactView>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentArtifactView {
    pub artifact_id: String,
    pub name: String,
    pub path: String,
    pub root_id: Option<String>,
    pub relative_path: Option<String>,
    pub root_path: Option<String>,
    pub needs_root: bool,
    pub exists: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentReceipt {
    pub schema_version: u32,
    pub request_id: String,
    pub status: String,
    pub received_at: String,
    pub completed_at: Option<String>,
    pub opened_artifacts: Vec<String>,
    pub errors: Vec<String>,
}

impl AgentReturnState {
    pub fn load(data_directory: &Path) -> Result<Self> {
        let inbox = data_directory.join("agent-inbox");
        let requests_directory = inbox.join("requests");
        let receipts_directory = inbox.join("receipts");
        ensure_agent_directory(&inbox)?;
        ensure_agent_directory(&requests_directory)?;
        ensure_agent_directory(&receipts_directory)?;
        for directory in [&inbox, &requests_directory, &receipts_directory] {
            if std::fs::symlink_metadata(directory)?
                .file_type()
                .is_symlink()
            {
                bail!("Agent Return storage directories cannot be symbolic links");
            }
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let private_permissions = std::fs::Permissions::from_mode(0o700);
            for directory in [&inbox, &requests_directory, &receipts_directory] {
                std::fs::set_permissions(directory, private_permissions.clone())
                    .context("could not secure Agent Return storage permissions")?;
            }
        }
        let pending_path = inbox.join("pending.json");
        let seen_path = inbox.join("seen.json");
        if let Ok(metadata) = std::fs::symlink_metadata(&pending_path)
            && metadata.file_type().is_symlink()
        {
            bail!("Agent Return queue cannot be a symbolic link");
        }
        if let Ok(metadata) = std::fs::symlink_metadata(&seen_path)
            && metadata.file_type().is_symlink()
        {
            bail!("Agent Return deduplication index cannot be a symbolic link");
        }
        let pending = match std::fs::read(&pending_path) {
            Ok(bytes) => {
                if bytes.len() > MAX_AGENT_STATE_BYTES {
                    bail!("saved Agent Return queue exceeds the size limit");
                }
                let saved: Vec<PendingRequest> = serde_json::from_slice(&bytes)
                    .context("saved Agent Return queue is invalid")?;
                if saved.len() > 50 {
                    bail!("saved Agent Return queue exceeds the request limit");
                }
                for request in &saved {
                    validate_payload(&serde_json::to_vec(&request.payload)?)
                        .context("saved Agent Return request failed validation")?;
                }
                saved
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(error) => return Err(error).context("could not read Agent Return queue"),
        };
        let (mut seen, seen_file_exists) = match std::fs::read(&seen_path) {
            Ok(bytes) => {
                if bytes.len() > MAX_AGENT_STATE_BYTES {
                    bail!("Agent Return deduplication index exceeds the size limit");
                }
                let saved: HashMap<String, SeenRequest> = serde_json::from_slice(&bytes)
                    .context("saved Agent Return deduplication index is invalid")?;
                if saved.len() > MAX_SEEN_REQUESTS {
                    bail!("Agent Return deduplication index exceeds the request limit");
                }
                (saved, true)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (HashMap::new(), false),
            Err(error) => {
                return Err(error).context("could not read Agent Return deduplication index");
            }
        };
        for (request_id, entry) in &seen {
            let id = uuid::Uuid::parse_str(request_id)
                .context("saved Agent Return deduplication ID is invalid")?;
            if id.to_string() != request_id.to_ascii_lowercase() {
                bail!("saved Agent Return deduplication ID is not canonical");
            }
            let receipt_path = Self::canonical_receipt_path(
                &receipts_directory,
                Some(&entry.receipt_path),
                request_id,
            )?;
            if receipt_path != entry.receipt_path {
                bail!("saved Agent Return receipt path is not canonical");
            }
        }
        let previous_seen_count = seen.len();
        for request in &pending {
            let receipt_path = Self::canonical_receipt_path(
                &receipts_directory,
                Some(&request.receipt_path),
                &request.payload.request_id,
            )?;
            if receipt_path != request.receipt_path {
                bail!("saved Agent Return receipt path is not canonical");
            }
            match seen.get(&request.payload.request_id) {
                Some(existing) if existing.receipt_path != receipt_path => {
                    bail!("saved Agent Return queue and deduplication index disagree");
                }
                Some(_) => {}
                None => {
                    seen.insert(
                        request.payload.request_id.clone(),
                        SeenRequest {
                            receipt_path,
                            received_at: request.received_at.clone(),
                        },
                    );
                }
            }
        }
        if seen.len() > MAX_SEEN_REQUESTS {
            bail!("Agent Return deduplication index exceeds the request limit");
        }
        let state = Self {
            requests_directory,
            receipts_directory,
            pending_path,
            seen_path,
            pending: Mutex::new(pending),
            seen: Mutex::new(seen),
        };
        let current_seen_count = state
            .seen
            .lock()
            .map_err(|_| anyhow::anyhow!("Agent Return deduplication index is unavailable"))?
            .len();
        if !seen_file_exists || current_seen_count != previous_seen_count {
            let seen = state
                .seen
                .lock()
                .map_err(|_| anyhow::anyhow!("Agent Return deduplication index is unavailable"))?;
            persist_seen(&state.seen_path, &seen)?;
        }
        Ok(state)
    }

    pub fn receive_payload(
        &self,
        payload_path: &Path,
        receipt_path: Option<&Path>,
    ) -> Result<AgentReceipt> {
        match self.receive_payload_inner(payload_path, receipt_path) {
            Ok(receipt) => Ok(receipt),
            Err(error) => {
                if let Err(receipt_error) =
                    self.record_rejected_receipt(payload_path, receipt_path, &error)
                {
                    return Err(anyhow::anyhow!(
                        "{error}; could not write rejected Agent Return receipt: {receipt_error}"
                    ));
                }
                Err(error)
            }
        }
    }

    fn receive_payload_inner(
        &self,
        payload_path: &Path,
        receipt_path: Option<&Path>,
    ) -> Result<AgentReceipt> {
        let payload_path = self.validate_request_path(payload_path)?;
        let metadata = std::fs::symlink_metadata(&payload_path)
            .context("Agent Return payload is unavailable")?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            bail!("Agent Return payload must be a regular file");
        }
        let file =
            std::fs::File::open(&payload_path).context("could not open Agent Return payload")?;
        let mut bytes = Vec::with_capacity(metadata.len().min(MAX_PAYLOAD_BYTES as u64) as usize);
        file.take((MAX_PAYLOAD_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .context("could not read Agent Return payload")?;
        let mut payload = validate_payload(&bytes)?;
        let request_id = payload.request_id.clone();
        let should_queue = payload.viewer.action != ViewerAction::None;
        let received_at = chrono::Utc::now().to_rfc3339();
        let receipt_path = self.validate_receipt_path(receipt_path, &request_id)?;
        if let Some(receipt) = self.existing_receipt(&request_id, &receipt_path)? {
            return Ok(receipt);
        }
        for artifact in &mut payload.artifacts {
            canonicalize_artifact(artifact)?;
        }

        let mut pending = self
            .pending
            .lock()
            .map_err(|_| anyhow::anyhow!("Agent Return queue is unavailable"))?;
        let mut seen = self
            .seen
            .lock()
            .map_err(|_| anyhow::anyhow!("Agent Return deduplication index is unavailable"))?;
        if let Some(existing) = seen.get(&request_id) {
            let existing_receipt_path =
                self.validate_receipt_path(Some(&existing.receipt_path), &request_id)?;
            let existing_received_at = existing.received_at.clone();
            let receipt = match std::fs::read(&existing_receipt_path) {
                Ok(bytes) => serde_json::from_slice(&bytes)
                    .context("saved Agent Return receipt is invalid")?,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    let receipt = AgentReceipt::accepted(request_id, existing_received_at);
                    write_json_atomically(&existing_receipt_path, &receipt)?;
                    receipt
                }
                Err(error) => return Err(error).context("could not read Agent Return receipt"),
            };
            if receipt_path != existing_receipt_path {
                write_json_atomically(&receipt_path, &receipt)?;
            }
            return Ok(receipt);
        }

        if seen.len() >= MAX_SEEN_REQUESTS {
            bail!("Agent Return deduplication index is full");
        }
        if should_queue {
            if pending.len() >= 50 {
                bail!("Agent Return queue is full");
            }
            pending.push(PendingRequest {
                payload,
                received_at: received_at.clone(),
                receipt_path: receipt_path.clone(),
            });
            if let Err(error) = persist_pending(&self.pending_path, &pending) {
                pending.pop();
                return Err(error);
            }
        }

        seen.insert(
            request_id.clone(),
            SeenRequest {
                receipt_path: receipt_path.clone(),
                received_at: received_at.clone(),
            },
        );
        if let Err(error) = persist_seen(&self.seen_path, &seen) {
            seen.remove(&request_id);
            if should_queue {
                pending.pop();
                if let Err(rollback_error) = persist_pending(&self.pending_path, &pending) {
                    return Err(anyhow::anyhow!(
                        "{error}; could not roll back Agent Return queue: {rollback_error}"
                    ));
                }
            }
            return Err(error);
        }
        let receipt = AgentReceipt::accepted(request_id, received_at);
        write_json_atomically(&receipt_path, &receipt)?;
        Ok(receipt)
    }

    fn existing_receipt(
        &self,
        request_id: &str,
        requested_receipt_path: &Path,
    ) -> Result<Option<AgentReceipt>> {
        let pending = self
            .pending
            .lock()
            .map_err(|_| anyhow::anyhow!("Agent Return queue is unavailable"))?;
        let seen = self
            .seen
            .lock()
            .map_err(|_| anyhow::anyhow!("Agent Return deduplication index is unavailable"))?;
        let Some(existing) = seen.get(request_id) else {
            return Ok(None);
        };
        let receipt_path = self.validate_receipt_path(Some(&existing.receipt_path), request_id)?;
        let existing_received_at = existing.received_at.clone();
        let receipt = match std::fs::read(&receipt_path) {
            Ok(bytes) => {
                serde_json::from_slice(&bytes).context("saved Agent Return receipt is invalid")?
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let receipt = AgentReceipt::accepted(request_id.to_owned(), existing_received_at);
                write_json_atomically(&receipt_path, &receipt)?;
                receipt
            }
            Err(error) => return Err(error).context("could not read Agent Return receipt"),
        };
        if requested_receipt_path != receipt_path {
            write_json_atomically(requested_receipt_path, &receipt)?;
        }
        drop(seen);
        drop(pending);
        Ok(Some(receipt))
    }

    fn record_rejected_receipt(
        &self,
        payload_path: &Path,
        requested_receipt_path: Option<&Path>,
        error: &anyhow::Error,
    ) -> Result<()> {
        let Ok(payload_path) = self.validate_request_path(payload_path) else {
            return Ok(());
        };
        let Ok(metadata) = std::fs::symlink_metadata(&payload_path) else {
            return Ok(());
        };
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Ok(());
        }
        let file = std::fs::File::open(&payload_path)?;
        let mut bytes = Vec::new();
        file.take((MAX_PAYLOAD_BYTES + 1) as u64)
            .read_to_end(&mut bytes)?;
        let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
            return Ok(());
        };
        let Some(request_id) = value.get("request_id").and_then(serde_json::Value::as_str) else {
            return Ok(());
        };
        let Ok(uuid) = uuid::Uuid::parse_str(request_id) else {
            return Ok(());
        };
        if uuid.to_string() != request_id.to_ascii_lowercase() {
            return Ok(());
        }
        let pending = self
            .pending
            .lock()
            .map_err(|_| anyhow::anyhow!("Agent Return queue is unavailable"))?;
        if pending
            .iter()
            .any(|request| request.payload.request_id == request_id)
        {
            return Ok(());
        }
        drop(pending);
        let seen = self
            .seen
            .lock()
            .map_err(|_| anyhow::anyhow!("Agent Return deduplication index is unavailable"))?;
        if seen.contains_key(request_id) {
            return Ok(());
        }
        drop(seen);
        let Ok(receipt_path) = self.validate_receipt_path(requested_receipt_path, request_id)
        else {
            return Ok(());
        };
        let receipt = AgentReceipt::rejected(request_id.to_owned(), error.to_string());
        write_json_atomically(&receipt_path, &receipt)
    }

    pub fn results(&self, roots: &[crate::models::RootEntry]) -> Result<Vec<AgentResultView>> {
        let pending = self
            .pending
            .lock()
            .map_err(|_| anyhow::anyhow!("Agent Return queue is unavailable"))?;
        pending
            .iter()
            .map(|request| {
                let artifacts = request
                    .payload
                    .artifacts
                    .iter()
                    .map(|artifact| artifact_view(artifact, roots))
                    .collect::<Result<Vec<_>>>()?;
                Ok(AgentResultView {
                    request_id: request.payload.request_id.clone(),
                    platform: request.payload.source.platform.clone(),
                    agent_name: request.payload.source.agent_name.clone(),
                    run_id: request.payload.source.run_id.clone(),
                    completed_at: request.payload.run.completed_at.clone(),
                    action: request.payload.viewer.action,
                    message: request
                        .payload
                        .result
                        .as_ref()
                        .map(|result| result.text.clone()),
                    received_at: request.received_at.clone(),
                    artifacts,
                })
            })
            .collect()
    }

    pub fn dismiss(&self, request_id: &str) -> Result<bool> {
        let mut pending = self
            .pending
            .lock()
            .map_err(|_| anyhow::anyhow!("Agent Return queue is unavailable"))?;
        let Some(index) = pending
            .iter()
            .position(|request| request.payload.request_id == request_id)
        else {
            return Ok(false);
        };
        let request = &pending[index];
        let receipt_path = self.validate_receipt_path(Some(&request.receipt_path), request_id)?;
        let mut receipt: AgentReceipt = serde_json::from_slice(
            &std::fs::read(&receipt_path).context("Agent Return receipt is unavailable")?,
        )
        .context("Agent Return receipt is invalid")?;
        if receipt.opened_artifacts.is_empty() {
            receipt.status = "rejected".to_owned();
            receipt.errors.push("Dismissed by the user.".to_owned());
        }
        receipt.completed_at = Some(chrono::Utc::now().to_rfc3339());
        write_json_atomically(&receipt_path, &receipt)?;
        let removed = pending.remove(index);
        if let Err(error) = persist_pending(&self.pending_path, &pending) {
            pending.insert(index, removed);
            return Err(error);
        }
        Ok(true)
    }

    pub fn mark_opened(&self, request_id: &str, artifact_id: &str) -> Result<AgentReceipt> {
        let pending = self
            .pending
            .lock()
            .map_err(|_| anyhow::anyhow!("Agent Return queue is unavailable"))?;
        let request = pending
            .iter()
            .find(|request| request.payload.request_id == request_id)
            .ok_or_else(|| anyhow::anyhow!("Agent Return request is no longer available"))?;
        if !request
            .payload
            .artifacts
            .iter()
            .any(|artifact| artifact.artifact_id == artifact_id)
        {
            bail!("artifact does not belong to this Agent Return request");
        }
        let receipt_path = self.validate_receipt_path(Some(&request.receipt_path), request_id)?;
        let mut receipt: AgentReceipt = serde_json::from_slice(
            &std::fs::read(&receipt_path).context("Agent Return receipt is unavailable")?,
        )
        .context("Agent Return receipt is invalid")?;
        if !receipt.opened_artifacts.iter().any(|id| id == artifact_id) {
            receipt.opened_artifacts.push(artifact_id.to_owned());
        }
        receipt.status = "opened".to_owned();
        receipt.completed_at = Some(chrono::Utc::now().to_rfc3339());
        write_json_atomically(&receipt_path, &receipt)?;
        Ok(receipt)
    }

    pub fn mark_failed(
        &self,
        request_id: &str,
        artifact_id: &str,
        error: &str,
    ) -> Result<AgentReceipt> {
        let pending = self
            .pending
            .lock()
            .map_err(|_| anyhow::anyhow!("Agent Return queue is unavailable"))?;
        let request = pending
            .iter()
            .find(|request| request.payload.request_id == request_id)
            .ok_or_else(|| anyhow::anyhow!("Agent Return request is no longer available"))?;
        if !request
            .payload
            .artifacts
            .iter()
            .any(|artifact| artifact.artifact_id == artifact_id)
        {
            bail!("artifact does not belong to this Agent Return request");
        }
        let receipt_path = self.validate_receipt_path(Some(&request.receipt_path), request_id)?;
        let mut receipt: AgentReceipt = serde_json::from_slice(
            &std::fs::read(&receipt_path).context("Agent Return receipt is unavailable")?,
        )
        .context("Agent Return receipt is invalid")?;
        if receipt.opened_artifacts.is_empty() {
            receipt.status = "failed".to_owned();
        }
        receipt.completed_at = Some(chrono::Utc::now().to_rfc3339());
        let error = error.chars().take(1024).collect::<String>();
        if receipt.errors.len() < 20 && !receipt.errors.contains(&error) {
            receipt.errors.push(error);
        }
        write_json_atomically(&receipt_path, &receipt)?;
        Ok(receipt)
    }

    pub fn allow_root(
        &self,
        request_id: &str,
        selected_path: &Path,
        roots: &[crate::models::RootEntry],
    ) -> Result<crate::models::RootEntry> {
        let selected = selected_path
            .canonicalize()
            .context("selected folder does not exist")?;
        if !selected.is_dir() {
            bail!("selected path is not a folder");
        }
        let pending = self
            .pending
            .lock()
            .map_err(|_| anyhow::anyhow!("Agent Return queue is unavailable"))?;
        let request = pending
            .iter()
            .find(|request| request.payload.request_id == request_id)
            .ok_or_else(|| anyhow::anyhow!("Agent Return request is no longer available"))?;
        let has_matching_artifact = request.payload.artifacts.iter().any(|artifact| {
            Path::new(&artifact.path).starts_with(&selected)
                && validate_absolute_path(Path::new(&artifact.path)).is_ok()
        });
        if !has_matching_artifact {
            bail!("selected folder does not contain a requested artifact");
        }
        roots
            .iter()
            .find(|root| Path::new(&root.path) == selected)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("selected folder must be registered as a root first"))
    }

    pub fn artifact_relative_path(
        &self,
        request_id: &str,
        artifact_id: &str,
        root: &crate::models::RootEntry,
    ) -> Result<String> {
        let pending = self
            .pending
            .lock()
            .map_err(|_| anyhow::anyhow!("Agent Return queue is unavailable"))?;
        let request = pending
            .iter()
            .find(|request| request.payload.request_id == request_id)
            .ok_or_else(|| anyhow::anyhow!("Agent Return request is no longer available"))?;
        let artifact = request
            .payload
            .artifacts
            .iter()
            .find(|artifact| artifact.artifact_id == artifact_id)
            .ok_or_else(|| {
                anyhow::anyhow!("artifact does not belong to this Agent Return request")
            })?;
        let artifact_path = Path::new(&artifact.path);
        validate_absolute_path(artifact_path)?;
        let canonical_artifact = artifact_path
            .canonicalize()
            .context("Agent Return artifact is unavailable")?;
        let canonical_root = Path::new(&root.path)
            .canonicalize()
            .context("registered folder is unavailable")?;
        let relative = canonical_artifact
            .strip_prefix(&canonical_root)
            .context("artifact is outside the registered folder")?;
        let relative_path = relative
            .components()
            .map(|component| component.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        let resolved =
            crate::path_validator::validate_relative_path(&canonical_root, &relative_path)
                .context("Agent Return artifact is not a safe file")?;
        if resolved != canonical_artifact || !resolved.is_file() {
            bail!("Agent Return artifact is not a safe file");
        }
        Ok(relative_path)
    }

    pub fn artifact_parent_relative_path(
        &self,
        request_id: &str,
        artifact_id: &str,
        root: &crate::models::RootEntry,
    ) -> Result<String> {
        let relative_path = self.artifact_relative_path(request_id, artifact_id, root)?;
        Ok(Path::new(&relative_path)
            .parent()
            .map(|parent| {
                parent
                    .components()
                    .map(|component| component.as_os_str().to_string_lossy())
                    .collect::<Vec<_>>()
                    .join("/")
            })
            .unwrap_or_default())
    }

    fn validate_request_path(&self, path: &Path) -> Result<PathBuf> {
        verify_agent_directory(&self.requests_directory)?;
        if let Some(inbox) = self.requests_directory.parent() {
            verify_agent_directory(inbox)?;
        }
        let request_directory = self
            .requests_directory
            .canonicalize()
            .context("Agent Return request directory is unavailable")?;
        let parent = path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("Agent Return payload path is invalid"))?
            .canonicalize()
            .context("Agent Return payload directory is unavailable")?;
        if parent != request_directory {
            bail!("Agent Return payload must be inside the request directory");
        }
        Ok(parent.join(
            path.file_name()
                .ok_or_else(|| anyhow::anyhow!("Agent Return payload path is invalid"))?,
        ))
    }

    fn validate_receipt_path(&self, path: Option<&Path>, request_id: &str) -> Result<PathBuf> {
        Self::canonical_receipt_path(&self.receipts_directory, path, request_id)
    }

    fn canonical_receipt_path(
        receipts_directory: &Path,
        path: Option<&Path>,
        request_id: &str,
    ) -> Result<PathBuf> {
        verify_agent_directory(receipts_directory)?;
        if let Some(inbox) = receipts_directory.parent() {
            verify_agent_directory(inbox)?;
        }
        let receipt_directory = receipts_directory
            .canonicalize()
            .context("Agent Return receipt directory is unavailable")?;
        let path = path
            .map(Path::to_path_buf)
            .unwrap_or_else(|| receipt_directory.join(format!("{request_id}.receipt.json")));
        let parent = path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("Agent Return receipt path is invalid"))?
            .canonicalize()
            .context("Agent Return receipt directory is unavailable")?;
        if parent != receipt_directory {
            bail!("Agent Return receipt must be inside the receipt directory");
        }
        if let Ok(metadata) = std::fs::symlink_metadata(&path)
            && metadata.file_type().is_symlink()
        {
            bail!("Agent Return receipt path cannot be a symbolic link");
        }
        Ok(parent.join(
            path.file_name()
                .ok_or_else(|| anyhow::anyhow!("Agent Return receipt path is invalid"))?,
        ))
    }
}

impl AgentReceipt {
    fn accepted(request_id: String, received_at: String) -> Self {
        Self {
            schema_version: 1,
            request_id,
            status: "accepted".to_owned(),
            received_at,
            completed_at: None,
            opened_artifacts: Vec::new(),
            errors: Vec::new(),
        }
    }

    fn rejected(request_id: String, error: String) -> Self {
        let received_at = chrono::Utc::now().to_rfc3339();
        Self {
            schema_version: 1,
            request_id,
            status: "rejected".to_owned(),
            received_at,
            completed_at: Some(chrono::Utc::now().to_rfc3339()),
            opened_artifacts: Vec::new(),
            errors: vec![error.chars().take(1024).collect()],
        }
    }
}

fn canonicalize_artifact(artifact: &mut AgentArtifact) -> Result<()> {
    let path = Path::new(&artifact.path);
    if !path.is_absolute() {
        bail!("Agent Return artifact path must be absolute");
    }
    validate_absolute_path(path)?;
    let canonical = path
        .canonicalize()
        .context("Agent Return artifact does not exist")?;
    let metadata = std::fs::metadata(&canonical).context("could not inspect Agent artifact")?;
    if !metadata.is_file() {
        bail!("Agent Return artifact is not a file");
    }
    artifact.path = canonical
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("Agent Return artifact path is not valid UTF-8"))?
        .to_owned();
    if artifact.name.is_none() {
        artifact.name = canonical
            .file_name()
            .and_then(|name| name.to_str())
            .map(str::to_owned);
    }
    if artifact.size.is_some_and(|size| size != metadata.len()) {
        bail!("Agent Return artifact size does not match the file");
    }
    artifact.size = Some(metadata.len());
    Ok(())
}

fn validate_absolute_path(path: &Path) -> Result<()> {
    if !path.is_absolute() {
        bail!("file path must be absolute");
    }
    let mut current = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => current.push(prefix.as_os_str()),
            Component::RootDir => current.push(component.as_os_str()),
            Component::CurDir => {}
            Component::Normal(name) => {
                current.push(name);
                let metadata = std::fs::symlink_metadata(&current)?;
                if metadata.file_type().is_symlink() {
                    bail!("symbolic links are not allowed in Agent artifact paths");
                }
            }
            Component::ParentDir => bail!("parent-directory segments are not allowed"),
        }
    }
    Ok(())
}

fn artifact_view(
    artifact: &AgentArtifact,
    roots: &[crate::models::RootEntry],
) -> Result<AgentArtifactView> {
    let path = Path::new(&artifact.path);
    let exists = path.is_file();
    for root in roots {
        let root_path = Path::new(&root.path);
        if let Ok(relative) = path.strip_prefix(root_path) {
            let relative_path = relative
                .components()
                .map(|component| component.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/");
            if crate::path_validator::validate_relative_path(root_path, &relative_path).is_ok() {
                return Ok(AgentArtifactView {
                    artifact_id: artifact.artifact_id.clone(),
                    name: artifact.name.clone().unwrap_or_else(|| {
                        path.file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .into_owned()
                    }),
                    path: artifact.path.clone(),
                    root_id: Some(root.id.clone()),
                    relative_path: Some(relative_path),
                    root_path: None,
                    needs_root: false,
                    exists,
                });
            }
        }
    }
    Ok(AgentArtifactView {
        artifact_id: artifact.artifact_id.clone(),
        name: artifact.name.clone().unwrap_or_else(|| {
            path.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        }),
        path: artifact.path.clone(),
        root_id: None,
        relative_path: None,
        root_path: path
            .parent()
            .map(|parent| parent.to_string_lossy().into_owned()),
        needs_root: true,
        exists,
    })
}

fn persist_pending(path: &Path, requests: &[PendingRequest]) -> Result<()> {
    write_json_atomically(path, requests)
}

fn persist_seen(path: &Path, seen: &HashMap<String, SeenRequest>) -> Result<()> {
    write_json_atomically(path, seen)
}

fn ensure_agent_directory(path: &Path) -> Result<()> {
    if let Ok(metadata) = std::fs::symlink_metadata(path) {
        if metadata.file_type().is_symlink() {
            bail!("Agent Return storage directories cannot be symbolic links");
        }
        if !metadata.is_dir() {
            bail!("Agent Return storage path is not a directory");
        }
    } else {
        std::fs::create_dir_all(path).with_context(|| {
            format!(
                "could not initialize Agent Return directory {}",
                path.display()
            )
        })?;
    }
    verify_agent_directory(path)
}

fn verify_agent_directory(path: &Path) -> Result<()> {
    let metadata = std::fs::symlink_metadata(path)
        .with_context(|| format!("Agent Return directory is unavailable: {}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        bail!("Agent Return storage directories must be real directories");
    }
    Ok(())
}

fn write_json_atomically<T: Serialize + ?Sized>(path: &Path, value: &T) -> Result<()> {
    let bytes = serde_json::to_vec(value).context("could not serialize Agent Return data")?;
    if bytes.len() > MAX_AGENT_STATE_BYTES {
        bail!("Agent Return storage exceeds the size limit");
    }
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Agent Return storage path is invalid"))?;
    verify_agent_directory(parent)?;
    if let Some(grandparent) = parent.parent() {
        verify_agent_directory(grandparent)?;
    }
    let file_name = path
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("storage path is invalid"))?
        .to_string_lossy();
    let temporary_path = path.with_file_name(format!("{file_name}.{}.tmp", uuid::Uuid::new_v4()));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(&temporary_path)
        .context("could not create temporary Agent Return storage")?;
    use std::io::Write;
    let write_result = (|| {
        file.write_all(&bytes)
            .context("could not write Agent Return data")?;
        file.flush().context("could not flush Agent Return data")?;
        file.sync_all()
            .context("could not synchronize Agent Return data")
    })();
    if let Err(error) = write_result {
        drop(file);
        let _ = std::fs::remove_file(&temporary_path);
        return Err(error);
    }
    #[cfg(windows)]
    if path.exists() {
        std::fs::remove_file(path).context("could not replace Agent Return data")?;
    }
    std::fs::rename(temporary_path, path).context("could not save Agent Return data")
}

fn default_content_type() -> String {
    "text".to_owned()
}
