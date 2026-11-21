"""Small standard-library client for handing local files to ViewerA."""

from __future__ import annotations

import json
import mimetypes
import os
import re
import shutil
import stat
import subprocess
import sys
import time
import uuid
from datetime import datetime, timedelta, timezone
from pathlib import Path
from typing import Any


class FileViewerError(RuntimeError):
    """Base error for ViewerA handoff failures."""


class FileViewerNotInstalled(FileViewerError):
    """The ViewerA executable could not be found or launched."""


class ViewerLaunchError(FileViewerError):
    """The ViewerA command returned an error or its receipt was invalid."""


class ReceiptTimeout(FileViewerError):
    """ViewerA did not finish processing the request before the timeout."""


class AgentReturnRejected(FileViewerError):
    """ViewerA rejected the Agent Return request."""


_REQUEST_NAME = re.compile(
    r"^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}"
    r"(?:\.receipt)?\.json$"
)
_FINAL_RECEIPT_STATUSES = {"opened", "rejected", "failed"}


class FileViewerClient:
    """Write a validated Agent Return payload and invoke the ViewerA CLI."""

    def __init__(
        self,
        executable: str | os.PathLike[str] | None = None,
        *,
        data_dir: str | os.PathLike[str] | None = None,
        receipt_timeout_ms: int = 10_000,
        retention_days: int = 7,
    ) -> None:
        if receipt_timeout_ms <= 0:
            raise ValueError("receipt_timeout_ms must be positive")
        if retention_days < 0:
            raise ValueError("retention_days cannot be negative")
        self.executable = os.fspath(executable) if executable is not None else self._find_executable()
        self.data_dir = Path(data_dir) if data_dir is not None else self._default_data_dir()
        self.requests_dir = self.data_dir / "agent-inbox" / "requests"
        self.receipts_dir = self.data_dir / "agent-inbox" / "receipts"
        self.receipt_timeout_ms = receipt_timeout_ms
        self.retention_days = retention_days

    def open_files(
        self,
        paths: list[str],
        *,
        source_platform: str,
        agent_id: str,
        run_id: str | None = None,
        message: str | None = None,
        wait_for_receipt: bool = False,
    ) -> dict[str, Any]:
        """Hand generated local files to ViewerA and optionally await completion."""
        self._ensure_directories()
        self._cleanup_expired()
        payload = self._build_payload(
            paths,
            source_platform=source_platform,
            agent_id=agent_id,
            run_id=run_id,
            message=message,
        )
        request_id = payload["request_id"]
        payload_path = self.requests_dir / f"{request_id}.json"
        receipt_path = self.receipts_dir / f"{request_id}.receipt.json"
        self._write_payload(payload_path, payload)

        command = [
            self.executable,
            "agent-return",
            "--payload",
            str(payload_path),
            "--receipt",
            str(receipt_path),
            "--quiet",
        ]
        try:
            process = subprocess.Popen(command, shell=False)
        except FileNotFoundError as error:
            raise FileViewerNotInstalled(
                f"Could not launch ViewerA at {self.executable!r}"
            ) from error
        except OSError as error:
            raise ViewerLaunchError(
                f"ViewerA could not be started at {self.executable!r}"
            ) from error

        result: dict[str, Any] = {
            "request_id": request_id,
            "payload_path": str(payload_path),
            "receipt_path": str(receipt_path),
            "process_id": process.pid,
        }
        if wait_for_receipt:
            result["receipt"] = self._wait_for_receipt(process, receipt_path)
        return result

    def _build_payload(
        self,
        paths: list[str],
        *,
        source_platform: str,
        agent_id: str,
        run_id: str | None,
        message: str | None,
    ) -> dict[str, Any]:
        if not source_platform or len(source_platform) > 128:
            raise ValueError("source_platform must contain 1 to 128 characters")
        if not agent_id:
            raise ValueError("agent_id must not be empty")
        if message is not None and len(message.encode("utf-8")) > 32 * 1024:
            raise ValueError("message exceeds 32 KiB")
        if not paths or len(paths) > 20:
            raise ValueError("paths must contain between 1 and 20 files")

        unique_paths = list(dict.fromkeys(paths))
        if len(unique_paths) > 20:
            raise ValueError("paths must contain no more than 20 unique files")
        artifacts = []
        for value in unique_paths:
            path = Path(value).expanduser().resolve(strict=True)
            if not stat.S_ISREG(path.stat().st_mode):
                raise ValueError(f"Artifact is not a regular file: {path}")
            mime_type, _ = mimetypes.guess_type(path.name)
            artifacts.append(
                {
                    "artifact_id": str(uuid.uuid4()),
                    "kind": "file",
                    "path": str(path),
                    "name": path.name,
                    "mime_type": mime_type or "application/octet-stream",
                    "size": path.stat().st_size,
                }
            )

        payload: dict[str, Any] = {
            "schema_version": 1,
            "request_id": str(uuid.uuid4()),
            "source": {
                "platform": source_platform,
                "agent_id": agent_id,
                "run_id": run_id,
            },
            "run": {
                "status": "completed",
                "completed_at": datetime.now(timezone.utc).isoformat(),
            },
            "viewer": {
                "action": "open_artifacts",
                "selection": "first",
                "focus": True,
                "open_preview": True,
            },
            "artifacts": artifacts,
        }
        if message is not None:
            payload["result"] = {"text": message, "content_type": "text"}
        return payload

    def _write_payload(self, path: Path, payload: dict[str, Any]) -> None:
        encoded = json.dumps(payload, ensure_ascii=False, separators=(",", ":")).encode("utf-8")
        if len(encoded) > 1024 * 1024:
            raise ValueError("Agent Return payload exceeds 1 MiB")
        temporary = path.with_name(f".{path.name}.{uuid.uuid4().hex}.tmp")
        descriptor = os.open(temporary, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
        try:
            with os.fdopen(descriptor, "wb") as stream:
                stream.write(encoded)
                stream.flush()
                os.fsync(stream.fileno())
            os.replace(temporary, path)
        except BaseException:
            temporary.unlink(missing_ok=True)
            raise

    def _wait_for_receipt(self, process: subprocess.Popen[bytes], path: Path) -> dict[str, Any]:
        deadline = time.monotonic() + self.receipt_timeout_ms / 1000
        while time.monotonic() < deadline:
            if path.is_symlink():
                raise ViewerLaunchError("ViewerA receipt cannot be a symbolic link")
            if path.exists():
                try:
                    receipt = json.loads(path.read_text(encoding="utf-8"))
                except json.JSONDecodeError:
                    receipt = None
                if isinstance(receipt, dict):
                    status = receipt.get("status")
                    if status in _FINAL_RECEIPT_STATUSES:
                        if status != "opened":
                            raise AgentReturnRejected(
                                f"ViewerA returned {status}: {receipt.get('errors', [])}"
                            )
                        return receipt
            return_code = process.poll()
            if return_code is not None and return_code != 0:
                raise ViewerLaunchError(
                    f"ViewerA exited with status {return_code} before opening the request"
                )
            time.sleep(0.05)
        raise ReceiptTimeout(
            f"ViewerA did not finish the request within {self.receipt_timeout_ms} ms"
        )

    def _ensure_directories(self) -> None:
        inbox = self.data_dir / "agent-inbox"
        for directory in (self.data_dir, inbox, self.requests_dir, self.receipts_dir):
            directory.mkdir(parents=True, mode=0o700, exist_ok=True)
            if directory.is_symlink() or not directory.is_dir():
                raise FileViewerError(f"Handoff directory is not a regular directory: {directory}")
            if os.name == "posix" and directory != self.data_dir:
                directory.chmod(0o700)

    def _cleanup_expired(self) -> None:
        cutoff = datetime.now(timezone.utc) - timedelta(days=self.retention_days)
        for directory in (self.requests_dir, self.receipts_dir):
            for path in directory.iterdir():
                if not _REQUEST_NAME.fullmatch(path.name) or path.is_symlink():
                    continue
                if not path.is_file():
                    continue
                modified_at = datetime.fromtimestamp(path.stat().st_mtime, timezone.utc)
                if modified_at < cutoff:
                    path.unlink()

    @staticmethod
    def _find_executable() -> str:
        executable = shutil.which("viewera")
        if executable is None and os.name == "nt":
            executable = shutil.which("viewera.exe")
        if executable is None:
            raise FileViewerNotInstalled("ViewerA was not found on PATH")
        return executable

    @staticmethod
    def _default_data_dir() -> Path:
        app_directory = "com.fileviewer.desktop"
        if os.name == "nt":
            base = os.environ.get("APPDATA")
            if base:
                return Path(base) / app_directory
        elif sys.platform == "darwin":
            return Path.home() / "Library" / "Application Support" / app_directory
        else:
            base = os.environ.get("XDG_DATA_HOME")
            return (Path(base) if base else Path.home() / ".local" / "share") / app_directory
        return Path.home() / "AppData" / "Roaming" / app_directory
