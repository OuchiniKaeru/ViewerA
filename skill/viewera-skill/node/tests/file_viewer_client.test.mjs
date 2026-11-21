import assert from "node:assert/strict";
import { EventEmitter } from "node:events";
import {
  mkdtemp,
  mkdir,
  readFile,
  realpath,
  rm,
  writeFile,
} from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";

import {
  AgentReturnRejected,
  FileViewerClient,
} from "../file_viewer_client.mjs";

test("writes a validated payload and launches the CLI with separate arguments", async () => {
  const temporary = await mkdtemp(path.join(os.tmpdir(), "file-viewer-node-"));
  const artifactDirectory = path.join(temporary, "generated files");
  await mkdir(artifactDirectory);
  const artifactPath = path.join(artifactDirectory, "report ; $(not-a-command).txt");
  await writeFile(artifactPath, "report");
  let invocation;

  const client = new FileViewerClient({
    executable: "file-viewer",
    dataDir: path.join(temporary, "data"),
    spawnProcess: (executable, args, options) => {
      invocation = { executable, args, options };
      const child = new EventEmitter();
      child.pid = 123;
      child.exitCode = null;
      queueMicrotask(() => child.emit("spawn"));
      return child;
    },
  });

  try {
    const result = await client.openFiles([artifactPath, artifactPath], {
      sourcePlatform: "node-test",
      agentId: "test-agent",
    });
    const payload = JSON.parse(await readFile(result.payloadPath, "utf8"));

    assert.equal(payload.schema_version, 1);
    assert.equal(payload.artifacts.length, 1);
    assert.equal(payload.artifacts[0].path, await realpath(artifactPath));
    assert.equal(invocation.executable, "file-viewer");
    assert.deepEqual(invocation.args.slice(0, 3), [
      "agent-return",
      "--payload",
      result.payloadPath,
    ]);
    assert.equal(invocation.options.shell, false);
    assert.equal(result.processId, 123);
  } finally {
    await rm(temporary, { recursive: true, force: true });
  }
});

test("rejects non-files and an empty artifact list", async () => {
  const temporary = await mkdtemp(path.join(os.tmpdir(), "file-viewer-node-"));
  const client = new FileViewerClient({
    executable: "file-viewer",
    dataDir: path.join(temporary, "data"),
    spawnProcess: () => {
      throw new Error("must not launch for invalid input");
    },
  });

  try {
    await assert.rejects(
      client.openFiles([], { sourcePlatform: "node-test", agentId: "agent" }),
      /between 1 and 20/,
    );
    await assert.rejects(
      client.openFiles([temporary], {
        sourcePlatform: "node-test",
        agentId: "agent",
      }),
      /regular file/,
    );
  } finally {
    await rm(temporary, { recursive: true, force: true });
  }
});

test("reports a rejected receipt instead of treating it as success", async () => {
  const temporary = await mkdtemp(path.join(os.tmpdir(), "file-viewer-node-"));
  const artifactPath = path.join(temporary, "report.txt");
  await writeFile(artifactPath, "report");
  const client = new FileViewerClient({
    executable: "file-viewer",
    dataDir: path.join(temporary, "data"),
    spawnProcess: (_executable, args) => {
      const child = new EventEmitter();
      child.pid = 456;
      child.exitCode = null;
      const receiptPath = args[args.indexOf("--receipt") + 1];
      queueMicrotask(() =>
        writeFile(
          receiptPath,
          JSON.stringify({ status: "rejected", errors: ["root not approved"] }),
        ),
      );
      queueMicrotask(() => child.emit("spawn"));
      return child;
    },
  });

  try {
    await assert.rejects(
      client.openFiles([artifactPath], {
        sourcePlatform: "node-test",
        agentId: "agent",
        waitForReceipt: true,
      }),
      AgentReturnRejected,
    );
  } finally {
    await rm(temporary, { recursive: true, force: true });
  }
});
