import { readFileSync } from "node:fs";

const tray = readFileSync("src-tauri/src/tray.rs", "utf8");
const config = JSON.parse(readFileSync("src-tauri/tauri.conf.json", "utf8"));

function optionalFunctionBody(source, name) {
  const start = source.indexOf(`fn ${name}`);
  if (start === -1) return "";

  const open = source.indexOf("{", start);
  let depth = 0;
  for (let index = open; index < source.length; index += 1) {
    if (source[index] === "{") depth += 1;
    if (source[index] === "}") depth -= 1;
    if (depth === 0) return source.slice(open + 1, index);
  }

  throw new Error(`Could not parse function ${name}`);
}

function functionBodies(source, name) {
  const bodies = [];
  let searchFrom = 0;

  while (searchFrom < source.length) {
    const start = source.indexOf(`fn ${name}`, searchFrom);
    if (start === -1) break;

    const open = source.indexOf("{", start);
    let depth = 0;
    for (let index = open; index < source.length; index += 1) {
      if (source[index] === "{") depth += 1;
      if (source[index] === "}") depth -= 1;
      if (depth === 0) {
        bodies.push(source.slice(open + 1, index));
        searchFrom = index + 1;
        break;
      }
    }
  }

  return bodies;
}

const primeBody = optionalFunctionBody(tray, "prime_popover");
const hideBodies = functionBodies(tray, "hide_popover_window");
const hiddenSurfaceBody = hideBodies.find((body) => body.includes("POPOVER_HIDDEN_WIDTH")) ?? "";
const popoverConfig = config.app.windows.find((window) => window.label === "popover");
const eagerWebviewLabels = config.app.windows.map((window) => window.label);

if (eagerWebviewLabels.includes("main") || eagerWebviewLabels.includes("popover")) {
  throw new Error("main and popover WebViews must not be created at startup");
}

if (primeBody.includes(".show(")) {
  throw new Error("prime_popover must not show a visible off-screen popover");
}

if (!hideBodies.some((body) => body.includes(".hide("))) {
  throw new Error("hide_popover_window must hide the popover window");
}

if (hideBodies.some((body) => body.includes(".destroy("))) {
  throw new Error("hide_popover_window must not destroy WebView windows during tray/focus handling");
}

if (tray.includes(".destroy(") || tray.includes(".close(")) {
  throw new Error("popover lifecycle must not close or destroy WebView windows during tray/focus handling");
}

if (!hiddenSurfaceBody) {
  throw new Error("hidden popover should shrink its surface after hide");
}

if (hideBodies.some((body) => body.includes("park_popover_offscreen"))) {
  throw new Error("hide_popover_window must not park the popover visible off-screen");
}

if (popoverConfig?.backgroundThrottling === "disabled") {
  throw new Error("popover backgroundThrottling must not be disabled");
}
