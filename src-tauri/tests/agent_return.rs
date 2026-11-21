use viewera_lib::agent_return::AgentReturnState;
use viewera_lib::agent_return::validate_payload;
use serde_json::json;

fn payload() -> serde_json::Value {
    json!({
        "schema_version": 1,
        "request_id": "8a72d1d6-1e1e-4e7a-b22c-19fc8e3f86d0",
        "source": {
            "platform": "test-agent",
            "agent_id": "report_agent",
            "run_id": "run-1"
        },
        "run": {
            "status": "completed",
            "completed_at": "2026-11-15T10:30:00+09:00"
        },
        "viewer": {
            "action": "open_artifacts",
            "selection": "first",
            "focus": true,
            "open_preview": true
        },
        "artifacts": [{
            "artifact_id": "artifact-1",
            "kind": "file",
            "path": "/tmp/report.pdf",
            "name": "report.pdf",
            "mime_type": "application/pdf",
            "size": 20
        }],
        "result": {
            "text": "Report created",
            "content_type": "text"
        }
    })
}

#[test]
fn accepts_a_valid_completed_agent_return() {
    let bytes = serde_json::to_vec(&payload()).unwrap();

    let result = validate_payload(&bytes).unwrap();

    assert_eq!(result.request_id, "8a72d1d6-1e1e-4e7a-b22c-19fc8e3f86d0");
    assert_eq!(result.artifacts.len(), 1);
}

#[test]
fn rejects_unknown_schema_versions_and_invalid_request_ids() {
    let mut invalid_version = payload();
    invalid_version["schema_version"] = json!(2);
    assert!(validate_payload(&serde_json::to_vec(&invalid_version).unwrap()).is_err());

    let mut invalid_id = payload();
    invalid_id["request_id"] = json!("not-a-uuid");
    assert!(validate_payload(&serde_json::to_vec(&invalid_id).unwrap()).is_err());
}

#[test]
fn rejects_incomplete_runs_unknown_actions_and_excess_artifacts() {
    let mut incomplete = payload();
    incomplete["run"]["status"] = json!("failed");
    assert!(validate_payload(&serde_json::to_vec(&incomplete).unwrap()).is_err());

    let mut unknown_action = payload();
    unknown_action["viewer"]["action"] = json!("execute_command");
    assert!(validate_payload(&serde_json::to_vec(&unknown_action).unwrap()).is_err());

    let mut too_many = payload();
    too_many["artifacts"] = json!(
        (0..21)
            .map(|index| {
                json!({
                    "artifact_id": format!("artifact-{index}"),
                    "kind": "file",
                    "path": "/tmp/output.txt",
                    "name": "output.txt"
                })
            })
            .collect::<Vec<_>>()
    );
    assert!(validate_payload(&serde_json::to_vec(&too_many).unwrap()).is_err());
}

#[test]
fn queues_agent_results_once_and_keeps_unregistered_files_unopened() {
    let app_data = tempfile::tempdir().unwrap();
    let artifact_folder = tempfile::tempdir().unwrap();
    let artifact_path = artifact_folder.path().join("report.pdf");
    std::fs::write(&artifact_path, "pdf").unwrap();
    let mut request = payload();
    request["artifacts"][0]["path"] = json!(artifact_path);
    request["artifacts"][0]["size"] = json!(3);
    let requests = app_data.path().join("agent-inbox/requests");
    std::fs::create_dir_all(&requests).unwrap();
    let payload_path = requests.join("request.json");
    std::fs::write(&payload_path, serde_json::to_vec(&request).unwrap()).unwrap();

    let state = AgentReturnState::load(app_data.path()).unwrap();
    let first = state.receive_payload(&payload_path, None).unwrap();
    let duplicate = state.receive_payload(&payload_path, None).unwrap();
    let results = state.results(&[]).unwrap();

    assert_eq!(first.status, "accepted");
    assert_eq!(duplicate.status, "accepted");
    assert_eq!(results.len(), 1);
    assert!(results[0].artifacts[0].needs_root);
    assert!(
        state
            .dismiss("8a72d1d6-1e1e-4e7a-b22c-19fc8e3f86d0")
            .unwrap()
    );
    let receipt: serde_json::Value = serde_json::from_slice(
        &std::fs::read(
            app_data
                .path()
                .join("agent-inbox/receipts/8a72d1d6-1e1e-4e7a-b22c-19fc8e3f86d0.receipt.json"),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(receipt["status"], "rejected");
}

#[test]
fn persists_requests_receipts_and_dismissals() {
    let app_data = tempfile::tempdir().unwrap();
    let artifact_folder = tempfile::tempdir().unwrap();
    let artifact_path = artifact_folder.path().join("report.pdf");
    std::fs::write(&artifact_path, "pdf").unwrap();
    let mut request = payload();
    request["artifacts"][0]["path"] = json!(artifact_path);
    request["artifacts"][0]["size"] = json!(3);
    let requests = app_data.path().join("agent-inbox/requests");
    std::fs::create_dir_all(&requests).unwrap();
    let payload_path = requests.join("request.json");
    std::fs::write(&payload_path, serde_json::to_vec(&request).unwrap()).unwrap();

    let state = AgentReturnState::load(app_data.path()).unwrap();
    state.receive_payload(&payload_path, None).unwrap();
    let failed = state
        .mark_failed(
            "8a72d1d6-1e1e-4e7a-b22c-19fc8e3f86d0",
            "artifact-1",
            "renderer failed",
        )
        .unwrap();
    assert_eq!(failed.status, "failed");
    let receipt = state
        .mark_opened("8a72d1d6-1e1e-4e7a-b22c-19fc8e3f86d0", "artifact-1")
        .unwrap();
    assert_eq!(receipt.status, "opened");
    assert_eq!(receipt.opened_artifacts, vec!["artifact-1"]);
    std::fs::remove_file(&artifact_path).unwrap();
    let duplicate = state.receive_payload(&payload_path, None).unwrap();
    assert_eq!(duplicate.status, "opened");
    assert_eq!(duplicate.opened_artifacts, vec!["artifact-1"]);
    drop(state);

    let state = AgentReturnState::load(app_data.path()).unwrap();
    assert_eq!(state.results(&[]).unwrap().len(), 1);
    assert!(
        state
            .dismiss("8a72d1d6-1e1e-4e7a-b22c-19fc8e3f86d0")
            .unwrap()
    );
    drop(state);
    let restored = AgentReturnState::load(app_data.path()).unwrap();
    assert_eq!(
        restored
            .receive_payload(&payload_path, None)
            .unwrap()
            .status,
        "opened"
    );
    assert!(restored.results(&[]).unwrap().is_empty());
}

#[test]
fn explicitly_registered_roots_resolve_only_the_matching_artifact() {
    let app_data = tempfile::tempdir().unwrap();
    let root_folder = tempfile::tempdir().unwrap();
    let artifact_path = root_folder.path().join("report.pdf");
    std::fs::write(&artifact_path, "pdf").unwrap();
    let mut request = payload();
    request["artifacts"][0]["path"] = json!(artifact_path);
    request["artifacts"][0]["size"] = json!(3);
    let requests = app_data.path().join("agent-inbox/requests");
    std::fs::create_dir_all(&requests).unwrap();
    let payload_path = requests.join("request.json");
    std::fs::write(&payload_path, serde_json::to_vec(&request).unwrap()).unwrap();

    let state = AgentReturnState::load(app_data.path()).unwrap();
    state.receive_payload(&payload_path, None).unwrap();
    let storage =
        viewera_lib::storage::AppState::load(&app_data.path().join("settings")).unwrap();
    let root = storage.add_root(root_folder.path()).unwrap();
    let roots = storage.roots().unwrap();
    let allowed = state
        .allow_root(
            "8a72d1d6-1e1e-4e7a-b22c-19fc8e3f86d0",
            root_folder.path(),
            &roots,
        )
        .unwrap();

    assert_eq!(allowed.id, root.id);
    assert_eq!(
        state
            .artifact_relative_path("8a72d1d6-1e1e-4e7a-b22c-19fc8e3f86d0", "artifact-1", &root,)
            .unwrap(),
        "report.pdf"
    );
}

#[test]
fn refuses_payloads_outside_the_private_request_directory() {
    let app_data = tempfile::tempdir().unwrap();
    let state = AgentReturnState::load(app_data.path()).unwrap();
    let outside = app_data.path().join("untrusted.json");
    std::fs::write(&outside, serde_json::to_vec(&payload()).unwrap()).unwrap();

    assert!(state.receive_payload(&outside, None).is_err());
}

#[cfg(unix)]
#[test]
fn refuses_symbolic_link_artifacts() {
    let app_data = tempfile::tempdir().unwrap();
    let artifact_folder = tempfile::tempdir().unwrap();
    let real_artifact = artifact_folder.path().join("report.pdf");
    let linked_artifact = artifact_folder.path().join("linked-report.pdf");
    std::fs::write(&real_artifact, "pdf").unwrap();
    std::os::unix::fs::symlink(&real_artifact, &linked_artifact).unwrap();
    let mut request = payload();
    request["artifacts"][0]["path"] = json!(linked_artifact);
    request["artifacts"][0]["size"] = json!(3);
    let requests = app_data.path().join("agent-inbox/requests");
    std::fs::create_dir_all(&requests).unwrap();
    let payload_path = requests.join("request.json");
    std::fs::write(&payload_path, serde_json::to_vec(&request).unwrap()).unwrap();
    let state = AgentReturnState::load(app_data.path()).unwrap();

    assert!(state.receive_payload(&payload_path, None).is_err());
}

#[test]
fn refuses_receipts_outside_the_private_receipt_directory() {
    let app_data = tempfile::tempdir().unwrap();
    let requests = app_data.path().join("agent-inbox/requests");
    std::fs::create_dir_all(&requests).unwrap();
    let payload_path = requests.join("request.json");
    let mut request = payload();
    request["viewer"]["action"] = json!("show_message");
    request["artifacts"] = json!([]);
    std::fs::write(&payload_path, serde_json::to_vec(&request).unwrap()).unwrap();
    let outside_receipt = app_data.path().join("receipt.json");
    let state = AgentReturnState::load(app_data.path()).unwrap();

    assert!(
        state
            .receive_payload(&payload_path, Some(&outside_receipt))
            .is_err()
    );
    assert!(!outside_receipt.exists());
}

#[test]
fn writes_a_rejected_receipt_for_identifiable_invalid_requests() {
    let app_data = tempfile::tempdir().unwrap();
    let requests = app_data.path().join("agent-inbox/requests");
    let receipts = app_data.path().join("agent-inbox/receipts");
    std::fs::create_dir_all(&requests).unwrap();
    let payload_path = requests.join("request.json");
    let mut request = payload();
    request["run"]["status"] = json!("failed");
    request["viewer"]["action"] = json!("show_message");
    request["artifacts"] = json!([]);
    std::fs::write(&payload_path, serde_json::to_vec(&request).unwrap()).unwrap();
    let state = AgentReturnState::load(app_data.path()).unwrap();

    let receive_error = state.receive_payload(&payload_path, None).unwrap_err();
    let receipt: serde_json::Value = serde_json::from_slice(
        &std::fs::read(receipts.join("8a72d1d6-1e1e-4e7a-b22c-19fc8e3f86d0.receipt.json"))
            .unwrap_or_else(|error| panic!("{receive_error:#}; receipt unavailable: {error}")),
    )
    .unwrap();
    assert_eq!(receipt["status"], "rejected");
    assert!(!receipt["errors"][0].as_str().unwrap().is_empty());
}

#[cfg(unix)]
#[test]
fn protects_agent_return_storage_with_user_only_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let app_data = tempfile::tempdir().unwrap();
    let requests = app_data.path().join("agent-inbox/requests");
    std::fs::create_dir_all(&requests).unwrap();
    let payload_path = requests.join("request.json");
    let mut request = payload();
    request["viewer"]["action"] = json!("show_message");
    request["artifacts"] = json!([]);
    std::fs::write(&payload_path, serde_json::to_vec(&request).unwrap()).unwrap();

    let state = AgentReturnState::load(app_data.path()).unwrap();
    state.receive_payload(&payload_path, None).unwrap();
    let inbox = app_data.path().join("agent-inbox");
    let receipt = inbox
        .join("receipts")
        .join("8a72d1d6-1e1e-4e7a-b22c-19fc8e3f86d0.receipt.json");
    assert_eq!(
        std::fs::metadata(inbox).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(
        std::fs::metadata(receipt).unwrap().permissions().mode() & 0o777,
        0o600
    );
}
