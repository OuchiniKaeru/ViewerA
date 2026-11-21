---
name: viewera-skill
description: Use when handing agent artifacts to ViewerA via Agent Return.
version: 1.0.0
author: OuchiniKaeru
license: MIT
metadata:
  hermes:
    tags: [viewera, agent-return, cli, integration]
    related_skills: [viewera-cli, tauri-desktop-debugging]
---

# ViewerA Agent Host Skill

## When to Use

- An agent host needs to show generated local files (PDF, images, documents,
  HTML…) in the ViewerA desktop app.
- Implementing or debugging the Agent Return handoff (payload, receipt,
  errors) in Python, Node.js, Rust, or PowerShell.

ViewerA accepts a versioned Agent Return JSON payload through its CLI. The
canonical schema is [`schema/agent-return-v1.schema.json`](./schema/agent-return-v1.schema.json).
The CLI stores requests in the current user's application-data directory and
uses the Tauri single-instance plugin to start the application or forward the
request to an existing instance.

## CLI

```text
viewera --help
viewera agent-return --payload PATH [--receipt PATH] [--quiet]
```

Invoke the executable with an argument array and `shell: false`; never build a
shell command from artifact paths. `--receipt` is optional, but integrations
should provide a file inside the current user's
`com.fileviewer.desktop/agent-inbox/receipts` directory. Payloads belong in the
matching `agent-inbox/requests` directory. The receiver rejects paths outside
those directories, symbolic links, unknown JSON fields, unsupported schema
versions, non-completed runs, and files outside the registered Root model.

Receipt statuses include `accepted`, `opened`, `rejected`, and `failed`.
`accepted` means the request was queued; wait for `opened`, `rejected`, or
`failed` when the host needs the final result. An unregistered Root requires
user confirmation. Do not retry an accepted request with a new request ID;
reusing its original ID is deduplicated.

Only explicitly enumerated local artifact metadata is supported. Do not parse
paths from natural-language agent output, trigger from intermediate streaming
events, or send failed/cancelled runs. The viewer is read-only and never runs
commands contained in a result.

## Python and Agno

The standard-library Python client and tests are in
[`python/file_viewer_client.py`](./python/file_viewer_client.py).

```python
from file_viewer_client import FileViewerClient

client = FileViewerClient(executable="/path/to/viewera")
client.open_files(
    ["/path/to/generated-report.pdf"],
    source_platform="my-agent",
    agent_id="report-agent",
    run_id="run-123",
    wait_for_receipt=False,
)
```

[`python/examples/agno_agent_adapter.py`](./python/examples/agno_agent_adapter.py)
shows a completion callback. The host must provide explicit artifact paths from
its final completed-run event. It intentionally does not import Agno classes or
assume a version-specific event API; connect it to the completion hook exposed
by the installed Agno version. It ignores failed/cancelled runs and does not
extract paths from prose.

## Node.js

The dependency-free Node client and example are in
[`node/file_viewer_client.mjs`](./node/file_viewer_client.mjs) and
[`examples/node-agent-return.mjs`](./examples/node-agent-return.mjs).
Node.js is needed only by the Agent Host running this adapter, not by the
ViewerA desktop application.

```sh
node skills/viewera-skill/examples/node-agent-return.mjs /path/to/generated-report.pdf
```

The client uses Node's `spawn` argument array with `shell: false`. Pass the
installed executable explicitly when it is not available on `PATH`.

## Rust

The small Rust client crate is in [`rust/`](./rust/). Add it as a local path
dependency, or copy the module and its dependencies into the Agent Host:

```toml
[dependencies]
file-viewer-client = { path = "skills/viewera-skill/rust" }
```

```rust
use file_viewer_client::FileViewerClient;
use std::path::PathBuf;

let client = FileViewerClient::new(
    "/path/to/viewera",
    "/path/to/user-data/com.fileviewer.desktop",
);
let handoff = client.open_files(
    &[PathBuf::from("/path/to/generated-report.pdf")],
    "my-rust-agent",
    "report-agent",
    Some("run-123"),
    None,
    false,
)?;
println!("Sent Agent Return {}", handoff.request_id);
```

## PowerShell

[`examples/powershell-agent-return.ps1`](./examples/powershell-agent-return.ps1)
uses .NET `ProcessStartInfo.ArgumentList`, not a shell command string. It
requires PowerShell 7 or later.

```powershell
./skills/viewera-skill/examples/powershell-agent-return.ps1 `
  -Executable "C:\Program Files\ViewerA\viewera.exe" `
  -Paths "C:\agent output\report.pdf" `
  -AgentId "report-agent"
```

## Data handling and failures

Adapters write payloads and receipts under the current user's app-data
directory, use restrictive POSIX permissions where available, and remove old
UUID-named handoff files according to their retention setting. Applications
may retain their own audit logs; do not put secrets or full agent transcripts
in the optional `result.text` field.

The Python and Node clients expose distinct errors for missing executables,
launch failures, rejected Agent Returns, and receipt timeouts. The Rust client
returns contextual `anyhow` errors. The PowerShell example throws on invalid
input, launch failures, rejected receipts, and timeouts.
