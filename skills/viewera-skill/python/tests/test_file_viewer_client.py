import sys
import tempfile
import unittest
import uuid
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from file_viewer_client import FileViewerClient


class FileViewerClientTests(unittest.TestCase):
    def test_builds_payload_from_verified_local_files(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            artifact = root / "report.txt"
            artifact.write_text("report", encoding="utf-8")
            client = FileViewerClient(executable="file-viewer", data_dir=root / "data")

            payload = client._build_payload(
                [str(artifact)],
                source_platform="test",
                agent_id="report-agent",
                run_id="run-1",
                message="Done",
            )

        self.assertEqual(payload["schema_version"], 1)
        self.assertEqual(payload["run"]["status"], "completed")
        self.assertEqual(payload["viewer"]["action"], "open_artifacts")
        self.assertEqual(payload["artifacts"][0]["size"], 6)
        self.assertEqual(payload["artifacts"][0]["path"], str(artifact.resolve()))
        self.assertEqual(payload["result"]["text"], "Done")
        self.assertEqual(str(uuid.UUID(payload["request_id"])), payload["request_id"])

    def test_rejects_missing_files_and_more_than_twenty_artifacts(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            client = FileViewerClient(executable="file-viewer", data_dir=root / "data")
            arguments = {
                "source_platform": "test",
                "agent_id": "agent",
                "run_id": None,
                "message": None,
            }
            with self.assertRaises(FileNotFoundError):
                client._build_payload([str(root / "missing")], **arguments)
            with self.assertRaises(ValueError):
                client._build_payload([str(root)] * 21, **arguments)

    def test_writes_payload_with_private_permissions(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            client = FileViewerClient(executable="file-viewer", data_dir=root / "data")
            client._ensure_directories()
            payload_path = client.requests_dir / "request.json"

            client._write_payload(payload_path, {"schema_version": 1})

            self.assertEqual(payload_path.read_text(encoding="utf-8"), '{"schema_version":1}')
            if hasattr(payload_path.stat(), "st_mode"):
                self.assertEqual(payload_path.stat().st_mode & 0o777, 0o600)

    def test_launches_cli_with_argument_array_and_shell_disabled(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            artifact = root / "report.txt"
            artifact.write_text("report", encoding="utf-8")
            client = FileViewerClient(executable="file-viewer", data_dir=root / "data")
            with patch("file_viewer_client.subprocess.Popen") as popen:
                popen.return_value.pid = 123

                result = client.open_files(
                    [str(artifact)],
                    source_platform="test",
                    agent_id="agent",
                )

        command = popen.call_args.args[0]
        self.assertEqual(command[1], "agent-return")
        self.assertIn("--payload", command)
        self.assertIn("--receipt", command)
        self.assertIs(popen.call_args.kwargs["shell"], False)
        self.assertEqual(result["process_id"], 123)


if __name__ == "__main__":
    unittest.main()
