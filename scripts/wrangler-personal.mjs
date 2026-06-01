import { existsSync, mkdirSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawn } from "node:child_process";

const __dirname = dirname(fileURLToPath(import.meta.url));
const projectRoot = resolve(__dirname, "..");
const authHome = resolve(projectRoot, ".wrangler-auth");
const args = process.argv.slice(2);
const bundledNode =
  "C:\\Users\\keven\\.cache\\codex-runtimes\\codex-primary-runtime\\dependencies\\node\\bin\\node.exe";
const localWrangler = resolve(projectRoot, "node_modules/wrangler/bin/wrangler.js");

mkdirSync(authHome, { recursive: true });

const hasBundledNode = process.platform === "win32" && existsSync(bundledNode);
const hasLocalWrangler = existsSync(localWrangler);
const command =
  hasBundledNode && hasLocalWrangler
    ? bundledNode
    : process.platform === "win32"
      ? "npx.cmd"
      : "npx";
const commandArgs =
  hasBundledNode && hasLocalWrangler
    ? [localWrangler, ...args]
    : ["wrangler", ...args];

const child = spawn(command, commandArgs, {
  cwd: projectRoot,
  env: {
    ...process.env,
    XDG_CONFIG_HOME: authHome,
  },
  shell: process.platform === "win32" && !hasBundledNode,
  stdio: "inherit",
});

child.on("exit", (code, signal) => {
  if (signal) {
    process.kill(process.pid, signal);
    return;
  }

  process.exit(code ?? 1);
});
