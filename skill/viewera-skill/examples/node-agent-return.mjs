import { FileViewerClient } from "../node/file_viewer_client.mjs";

const paths = process.argv.slice(2);
if (paths.length === 0) {
  throw new Error("Usage: node node-agent-return.mjs <generated-file> [...]");
}

const client = new FileViewerClient();
const handoff = await client.openFiles(paths, {
  sourcePlatform: "custom-node-agent",
  agentId: "example-agent",
  message: "エージェントがファイルを生成しました。",
});
console.log(`Sent Agent Return ${handoff.requestId}`);
