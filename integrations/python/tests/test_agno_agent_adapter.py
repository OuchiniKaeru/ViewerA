import asyncio
import sys
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import Mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from examples.agno_agent_adapter import handle_completed_run


class AgnoAdapterTests(unittest.TestCase):
    def test_only_hands_off_explicit_files_after_completed_runs(self):
        with tempfile.TemporaryDirectory() as temporary:
            artifact = Path(temporary) / "report.pdf"
            artifact.write_bytes(b"pdf")
            client = Mock()
            client.open_files.return_value = {"request_id": "request-1"}

            result = asyncio.run(
                handle_completed_run(
                    SimpleNamespace(status="completed", agent_id="agent-1", run_id="run-1"),
                    [artifact],
                    client,
                )
            )

        self.assertEqual(result, {"request_id": "request-1"})
        client.open_files.assert_called_once()
        self.assertEqual(client.open_files.call_args.args[0], [str(artifact.resolve())])
        self.assertEqual(client.open_files.call_args.kwargs["source_platform"], "agno")

    def test_ignores_non_completed_runs(self):
        client = Mock()

        result = asyncio.run(
            handle_completed_run(
                SimpleNamespace(status="cancelled"),
                [],
                client,
            )
        )

        self.assertIsNone(result)
        client.open_files.assert_not_called()


if __name__ == "__main__":
    unittest.main()
