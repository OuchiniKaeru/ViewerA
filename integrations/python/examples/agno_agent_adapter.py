"""Example callback for handing an Agno run's explicit local artifacts to ViewerA."""

from __future__ import annotations

import asyncio
from pathlib import Path
import sys
from typing import Iterable

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from file_viewer_client import FileViewerClient


async def handle_completed_run(
    run_output: object,
    artifact_paths: Iterable[str | Path],
    viewer_client: FileViewerClient,
) -> dict | None:
    """Call from the host's final-run callback, passing artifact paths explicitly."""
    status = getattr(run_output, "status", None)
    if getattr(status, "value", status) != "completed":
        return None

    paths = [str(Path(path).expanduser().resolve(strict=True)) for path in artifact_paths]
    if not paths:
        return None

    agent_id = getattr(run_output, "agent_id", None) or "agno-agent"
    run_id = getattr(run_output, "run_id", None)
    return await asyncio.to_thread(
        viewer_client.open_files,
        paths,
        source_platform="agno",
        agent_id=str(agent_id),
        run_id=str(run_id) if run_id is not None else None,
        message="エージェントがファイルを生成しました。",
        wait_for_receipt=False,
    )
