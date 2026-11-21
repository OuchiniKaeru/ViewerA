import { spawn as spawnChild } from "node:child_process";
import { accessSync, constants } from "node:fs";
import { randomUUID } from "node:crypto";
import {
  chmod,
  lstat,
  mkdir,
  open,
  readFile,
  readdir,
  realpath,
  rename,
  stat,
  unlink,
} from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { setTimeout as delay } from "node:timers/promises";

const requestName =
  /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}(?:\.receipt)?\.json$/;
const finalReceiptStatuses = new Set(["opened", "rejected", "failed"]);

export class FileViewerError extends Error {}
export class FileViewerNotInstalled extends FileViewerError {}
export class ViewerLaunchError extends FileViewerError {}
export class AgentReturnRejected extends FileViewerError {}
export class ReceiptTimeout extends FileViewerError {}

export class FileViewerClient {
  constructor({
    executable,
    dataDir = defaultDataDirectory(),
    receiptTimeoutMs = 10_000,
    retentionDays = 7,
    spawnProcess = spawnChild,
  } = {}) {
    if (receiptTimeoutMs <= 0) {
      throw new RangeError("receiptTimeoutMs must be positive");
    }
    if (retentionDays < 0) {
      throw new RangeError("retentionDays cannot be negative");
    }
    this.executable = executable ?? findExecutable();
    this.dataDir = dataDir;
    this.requestsDir = path.join(dataDir, "agent-inbox", "requests");
    this.receiptsDir = path.join(dataDir, "agent-inbox", "receipts");
    this.receiptTimeoutMs = receiptTimeoutMs;
    this.retentionDays = retentionDays;
    this.spawnProcess = spawnProcess;
  }

  async openFiles(
    paths,
    {
      sourcePlatform,
      agentId,
      runId = null,
      message = null,
      waitForReceipt = false,
    } = {},
  ) {
    const payload = await this.#buildPayload(paths, {
      sourcePlatform,
      agentId,
      runId,
      message,
    });
    await this.#ensureDirectories();
    await this.#cleanupExpired();

    const requestId = payload.request_id;
    const payloadPath = path.join(this.requestsDir, `${requestId}.json`);
    const receiptPath = path.join(this.receiptsDir, `${requestId}.receipt.json`);
    await this.#writePayload(payloadPath, payload);

    let child;
    try {
      child = this.spawnProcess(
        this.executable,
        [
          "agent-return",
          "--payload",
          payloadPath,
          "--receipt",
          receiptPath,
          "--quiet",
        ],
        { shell: false, stdio: "ignore" },
      );
      await new Promise((resolve, reject) => {
        child.once("spawn", resolve);
        child.once("error", reject);
      });
    } catch (error) {
      if (error?.code === "ENOENT") {
        throw new FileViewerNotInstalled(
          `Could not launch ViewerA at ${this.executable}`,
          { cause: error },
        );
      }
      throw new ViewerLaunchError(
        `ViewerA could not be started at ${this.executable}`,
        { cause: error },
      );
    }

    const result = {
      requestId,
      payloadPath,
      receiptPath,
      processId: child.pid,
    };
    if (waitForReceipt) {
      result.receipt = await this.#waitForReceipt(child, receiptPath);
    }
    return result;
  }

  async #buildPayload(paths, { sourcePlatform, agentId, runId, message }) {
    if (
      typeof sourcePlatform !== "string" ||
      sourcePlatform.length < 1 ||
      sourcePlatform.length > 128
    ) {
      throw new TypeError("sourcePlatform must contain 1 to 128 characters");
    }
    if (typeof agentId !== "string" || agentId.length < 1 || agentId.length > 128) {
      throw new TypeError("agentId must contain 1 to 128 characters");
    }
    if (message !== null && Buffer.byteLength(message, "utf8") > 32 * 1024) {
      throw new RangeError("message exceeds 32 KiB");
    }
    if (!Array.isArray(paths) || paths.length < 1 || paths.length > 20) {
      throw new RangeError("paths must contain between 1 and 20 files");
    }

    const artifacts = [];
    const seenPaths = new Set();
    for (const candidate of paths) {
      const filePath = await realpath(candidate);
      if (seenPaths.has(filePath)) continue;
      const metadata = await stat(filePath);
      if (!metadata.isFile()) {
        throw new TypeError(`Artifact is not a regular file: ${filePath}`);
      }
      artifacts.push({
        artifact_id: randomUUID(),
        kind: "file",
        path: filePath,
        name: path.basename(filePath),
        mime_type: mimeTypeForPath(filePath),
        size: metadata.size,
      });
      seenPaths.add(filePath);
    }
    if (artifacts.length < 1 || artifacts.length > 20) {
      throw new RangeError("paths must contain between 1 and 20 unique files");
    }

    const payload = {
      schema_version: 1,
      request_id: randomUUID(),
      source: { platform: sourcePlatform, agent_id: agentId, run_id: runId },
      run: { status: "completed", completed_at: new Date().toISOString() },
      viewer: {
        action: "open_artifacts",
        selection: "first",
        focus: true,
        open_preview: true,
      },
      artifacts,
    };
    if (message !== null) payload.result = { text: message, content_type: "text" };
    if (Buffer.byteLength(JSON.stringify(payload), "utf8") > 1024 * 1024) {
      throw new RangeError("Agent Return payload exceeds 1 MiB");
    }
    return payload;
  }

  async #ensureDirectories() {
    for (const directory of [
      this.dataDir,
      path.dirname(this.requestsDir),
      this.requestsDir,
      this.receiptsDir,
    ]) {
      await mkdir(directory, { recursive: true, mode: 0o700 });
      const metadata = await lstat(directory);
      if (metadata.isSymbolicLink() || !metadata.isDirectory()) {
        throw new FileViewerError(
          `Handoff directory is not a regular directory: ${directory}`,
        );
      }
      if (process.platform !== "win32") {
        await chmod(directory, 0o700);
      }
    }
  }

  async #cleanupExpired() {
    const cutoff = Date.now() - this.retentionDays * 24 * 60 * 60 * 1000;
    for (const directory of [this.requestsDir, this.receiptsDir]) {
      for (const name of await readdir(directory)) {
        if (!requestName.test(name)) continue;
        const filePath = path.join(directory, name);
        const metadata = await lstat(filePath);
        if (
          metadata.isSymbolicLink() ||
          !metadata.isFile() ||
          metadata.mtimeMs >= cutoff
        ) {
          continue;
        }
        await unlink(filePath);
      }
    }
  }

  async #writePayload(filePath, payload) {
    const temporaryPath = `${filePath}.${randomUUID()}.tmp`;
    const handle = await open(temporaryPath, "wx", 0o600);
    try {
      await handle.writeFile(JSON.stringify(payload));
      await handle.sync();
      await handle.close();
      await rename(temporaryPath, filePath);
    } catch (error) {
      await handle.close().catch(() => {});
      await unlink(temporaryPath).catch(() => {});
      throw error;
    }
  }

  async #waitForReceipt(child, receiptPath) {
    const deadline = Date.now() + this.receiptTimeoutMs;
    while (Date.now() < deadline) {
      try {
        const metadata = await lstat(receiptPath);
        if (metadata.isSymbolicLink() || !metadata.isFile()) {
          throw new FileViewerError("ViewerA receipt must be a regular file");
        }
        if (metadata.size > 1024 * 1024) {
          throw new FileViewerError("ViewerA receipt exceeds the size limit");
        }
        let receipt;
        try {
          receipt = JSON.parse(await readFile(receiptPath, "utf8"));
        } catch (error) {
          if (!(error instanceof SyntaxError)) throw error;
        }
        if (receipt && finalReceiptStatuses.has(receipt.status)) {
          if (receipt.status !== "opened") {
            throw new AgentReturnRejected(
              `ViewerA returned ${receipt.status}: ${JSON.stringify(receipt.errors ?? [])}`,
            );
          }
          return receipt;
        }
      } catch (error) {
        if (error?.code !== "ENOENT") throw error;
      }
      if (child.exitCode !== null && child.exitCode !== 0) {
        throw new FileViewerError(
          `ViewerA exited with status ${child.exitCode} before opening the request`,
        );
      }
      await delay(50);
    }
    throw new ReceiptTimeout(
      `ViewerA did not finish the request within ${this.receiptTimeoutMs} ms`,
    );
  }
}

function defaultDataDirectory() {
  const applicationDirectory = "com.fileviewer.desktop";
  if (process.platform === "win32") {
    return path.join(
      process.env.APPDATA ?? path.join(os.homedir(), "AppData", "Roaming"),
      applicationDirectory,
    );
  }
  if (process.platform === "darwin") {
    return path.join(
      os.homedir(),
      "Library",
      "Application Support",
      applicationDirectory,
    );
  }
  return path.join(
    process.env.XDG_DATA_HOME ?? path.join(os.homedir(), ".local", "share"),
    applicationDirectory,
  );
}

function findExecutable() {
  const executableName = process.platform === "win32" ? "viewera.exe" : "viewera";
  const searchPath = process.env.PATH ?? "";
  const candidates = searchPath
    .split(path.delimiter)
    .filter(Boolean)
    .map((directory) => path.join(directory, executableName));
  const candidate = candidates.find((filePath) => {
    try {
      accessSync(filePath, process.platform === "win32" ? constants.F_OK : constants.X_OK);
      return true;
    } catch {
      return false;
    }
  });
  if (!candidate) {
    throw new FileViewerNotInstalled("ViewerA was not found on PATH");
  }
  return candidate;
}

function mimeTypeForPath(filePath) {
  const extension = path.extname(filePath).slice(1).toLowerCase();
  const commonTypes = {
    csv: "text/csv",
    docx: "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
    json: "application/json",
    md: "text/markdown",
    pdf: "application/pdf",
    png: "image/png",
    pptx: "application/vnd.openxmlformats-officedocument.presentationml.presentation",
    txt: "text/plain",
    xlsx: "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
  };
  return commonTypes[extension] ?? "application/octet-stream";
}
