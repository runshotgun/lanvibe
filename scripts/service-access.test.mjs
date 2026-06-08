import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import vm from "node:vm";
import ts from "typescript";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(__dirname, "..");
const sourcePath = path.join(root, "src", "lib", "service-access.ts");
const source = await readFile(sourcePath, "utf8");

const { outputText } = ts.transpileModule(source, {
  compilerOptions: {
    module: ts.ModuleKind.CommonJS,
    target: ts.ScriptTarget.ES2020,
    esModuleInterop: true,
  },
  fileName: sourcePath,
});

const module = { exports: {} };
vm.runInNewContext(
  outputText,
  {
    exports: module.exports,
    module,
    URL,
    require(specifier) {
      throw new Error(`Unexpected runtime import: ${specifier}`);
    },
  },
  { filename: sourcePath }
);

const {
  isServiceOpenable,
  serviceOpenBlockReason,
  isServiceUnavailableFromHere,
} = module.exports;

assert.equal(typeof isServiceOpenable, "function");
assert.equal(typeof serviceOpenBlockReason, "function");
assert.equal(typeof isServiceUnavailableFromHere, "function");

const service = {
  id: 1,
  deviceId: "ip:192.168.1.100",
  ip: "192.168.1.100",
  port: 8765,
  scheme: "http",
  url: "http://192.168.1.100:8765/",
  title: "Stale dev server",
  statusCode: null,
  server: null,
  firstSeen: "2026-06-07T00:00:00Z",
  lastSeen: "2026-06-07T00:00:00Z",
  lastChecked: "2026-06-07T00:00:00Z",
  active: false,
  lastFailure: null,
  processOwner: null,
};

assert.equal(serviceOpenBlockReason(service, true), "Service is inactive");
assert.equal(isServiceOpenable(service, true), false);

const activeLanService = { ...service, active: true };
assert.equal(serviceOpenBlockReason(activeLanService, false), null);
assert.equal(isServiceOpenable(activeLanService, false), true);

const loopbackService = {
  ...activeLanService,
  ip: "127.0.0.1",
  url: "http://127.0.0.1:8765/",
};
assert.equal(
  serviceOpenBlockReason(loopbackService, false),
  "Only available on the machine running LANVibe"
);
assert.equal(isServiceOpenable(loopbackService, false), false);
assert.equal(serviceOpenBlockReason(loopbackService, true), null);
assert.equal(isServiceOpenable(loopbackService, true), true);

console.log("service-access tests passed");
