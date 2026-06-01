import http from "node:http";
import https from "node:https";
import net from "node:net";
import os from "node:os";
import fs from "node:fs/promises";
import path from "node:path";
import dns from "node:dns/promises";
import { execFile } from "node:child_process";
import { promisify } from "node:util";

const API_PORT = Number(process.env.LANVIBE_DEV_API_PORT ?? 18765);
const CONNECT_TIMEOUT_MS = Number(process.env.LANVIBE_CONNECT_TIMEOUT_MS ?? 450);
const HTTP_TIMEOUT_MS = Number(process.env.LANVIBE_HTTP_TIMEOUT_MS ?? 1200);
const CONCURRENCY = Number(process.env.LANVIBE_SCAN_CONCURRENCY ?? 512);
const RETENTION_DAYS = Number(process.env.LANVIBE_RETENTION_DAYS ?? 30);

function usableLanInterface(name, iface) {
  if (!iface || iface.family !== "IPv4" || iface.internal) return false;
  const [first, second] = iface.address.split(".").map(Number);
  const privateLan =
    first === 10 ||
    (first === 172 && second >= 16 && second <= 31) ||
    (first === 192 && second === 168);
  if (!privateLan) return false;

  const lowered = name.toLowerCase();
  return ![
    "bluetooth",
    "docker",
    "hyper-v",
    "loopback",
    "tailscale",
    "virtual",
    "vethernet",
    "vmware",
    "wsl",
  ].some((needle) => lowered.includes(needle));
}

const localIps = Object.entries(os.networkInterfaces())
  .flatMap(([name, ifaces]) =>
    (ifaces ?? [])
      .filter((iface) => usableLanInterface(name, iface))
      .map((iface) => iface.address),
  )
  .sort((a, b) => {
    const rank = (ip) => {
      const [first, second] = ip.split(".").map(Number);
      if (first === 192 && second === 168) return 0;
      if (first === 10) return 1;
      if (first === 172) return 2;
      return 3;
    };
    return rank(a) - rank(b) || a.localeCompare(b, undefined, { numeric: true });
  });

const primaryIp = localIps.find((ip) => ip.startsWith("192.168.1.")) ?? localIps[0] ?? "127.0.0.1";
const subnetPrefix = primaryIp.split(".").slice(0, 3).join(".");
const defaultTargetIps = [primaryIp];
const seedIps = (process.env.LANVIBE_DEV_TARGETS ?? "")
  .split(",")
  .map((ip) => ip.trim())
  .filter(Boolean);
const initialIps = seedIps.length > 0 ? seedIps : defaultTargetIps;
const execFileAsync = promisify(execFile);
const hostnameCache = new Map();

const nowIso = () => new Date().toISOString();

const stateDir = process.env.APPDATA
  ? path.join(process.env.APPDATA, "LANVibe")
  : path.join(os.homedir(), ".config", "lanvibe");
const statePath = path.join(stateDir, "dev-state.json");

const defaultSettings = {
  autoScan: true,
  manualOnly: false,
  minimizeToTray: true,
  launchAtStartup: true,
  scanIntervalSeconds: 120,
  discoveryIntervalSeconds: 60,
  retentionDays: RETENTION_DAYS,
  scanConcurrency: CONCURRENCY,
  connectTimeoutMs: CONNECT_TIMEOUT_MS,
  httpTimeoutMs: HTTP_TIMEOUT_MS,
  dashboardBind: "0.0.0.0",
  dashboardPort: API_PORT,
};

async function readPersistedState() {
  try {
    return JSON.parse(await fs.readFile(statePath, "utf8"));
  } catch {
    return {};
  }
}

const persistedState = {
  favorites: [],
  deviceAliases: {},
  settings: null,
  ...(await readPersistedState()),
};

async function savePersistedState() {
  await fs.mkdir(stateDir, { recursive: true });
  await fs.writeFile(
    statePath,
    `${JSON.stringify(
      {
        favorites: [...favorites],
        deviceAliases: persistedState.deviceAliases,
        settings,
      },
      null,
      2,
    )}\n`,
  );
}

function aliasFor(deviceId, ip) {
  return persistedState.deviceAliases?.[deviceId] ?? persistedState.deviceAliases?.[`ip:${ip}`] ?? null;
}

const devices = new Map(
  initialIps.map((ip) => [
    `ip:${ip}`,
    {
      id: `ip:${ip}`,
      ip,
      hostname: ip === primaryIp ? os.hostname() : null,
      mac: null,
      vendor: null,
      nameOverride: aliasFor(`ip:${ip}`, ip),
      selected: true,
      ignored: false,
      source: seedIps.length > 0 ? "manual" : "dev-default",
      lastSeen: nowIso(),
    },
  ]),
);

async function discoverDevices() {
  for (const device of await arpDevices()) {
    const existing = devices.get(device.id) ?? devices.get(`ip:${device.ip}`);
    const hostname = device.hostname ?? existing?.hostname ?? await resolveHostname(device.ip);
    if (existing && existing.id !== device.id) {
      devices.delete(existing.id);
      for (const [key, service] of services) {
        if (service.deviceId === existing.id) {
          services.set(key, { ...service, deviceId: device.id });
        }
      }
      const favoritesChanged = migrateFavoriteKeys(existing.id, device.id);
      if (persistedState.deviceAliases[existing.id] && !persistedState.deviceAliases[device.id]) {
        persistedState.deviceAliases[device.id] = persistedState.deviceAliases[existing.id];
        delete persistedState.deviceAliases[existing.id];
        await savePersistedState();
      } else if (favoritesChanged) {
        await savePersistedState();
      }
    }
    devices.set(device.id, {
      ...existing,
      ...device,
      hostname,
      selected: existing?.selected ?? false,
      ignored: existing?.ignored ?? false,
      nameOverride: existing?.nameOverride ?? aliasFor(device.id, device.ip),
      lastSeen: nowIso(),
    });
  }
  return [...devices.values()];
}

async function resolveHostname(ip) {
  if (hostnameCache.has(ip)) return hostnameCache.get(ip);
  let hostname = null;

  if (ip === primaryIp) {
    hostname = os.hostname();
  } else {
    hostname = await reverseDnsHostname(ip) ?? await pingResolvedHostname(ip);
  }

  hostname = normalizeHostname(hostname);
  hostnameCache.set(ip, hostname);
  return hostname;
}

async function reverseDnsHostname(ip) {
  try {
    const names = await dns.reverse(ip);
    return names[0] ?? null;
  } catch {
    return null;
  }
}

async function pingResolvedHostname(ip) {
  if (os.platform() !== "win32") return null;
  try {
    const { stdout } = await execFileAsync("ping", ["-a", "-n", "1", "-w", "700", ip], { timeout: 1500 });
    return stdout.match(/Pinging\s+([^\s\[]+)\s+\[/i)?.[1] ?? null;
  } catch {
    return null;
  }
}

function normalizeHostname(value) {
  const trimmed = value?.trim().replace(/\.$/, "");
  if (!trimmed || trimmed === "?" || /^\d{1,3}(?:\.\d{1,3}){3}$/.test(trimmed)) return null;
  return trimmed;
}

async function arpDevices() {
  try {
    const { stdout } = os.platform() === "win32"
      ? await execFileAsync("arp", ["-a"])
      : await execFileAsync("arp", ["-a"]);
    return parseArp(stdout);
  } catch {
    return [];
  }
}

function parseArp(text) {
  const ipRe = /(?<ip>(?:\d{1,3}\.){3}\d{1,3})/;
  const macRe = /(?<mac>[0-9a-f]{2}[:-][0-9a-f]{2}[:-][0-9a-f]{2}[:-][0-9a-f]{2}[:-][0-9a-f]{2}[:-][0-9a-f]{2})/i;
  return text
    .split(/\r?\n/)
    .map((line) => {
      const ip = line.match(ipRe)?.groups?.ip;
      const mac = line.match(macRe)?.groups?.mac?.replaceAll("-", ":").toLowerCase();
      if (!ip || !isPrivateIp(ip)) return null;
      return {
        id: mac ? `mac:${mac}` : `ip:${ip}`,
        ip,
        hostname: null,
        mac,
        vendor: inferVendor(mac),
        nameOverride: null,
        selected: false,
        ignored: false,
        source: "arp",
        lastSeen: nowIso(),
      };
    })
    .filter(Boolean);
}

function isPrivateIp(ip) {
  const [a, b] = ip.split(".").map(Number);
  return a === 10 || (a === 172 && b >= 16 && b <= 31) || (a === 192 && b === 168);
}

function inferVendor(mac) {
  const oui = mac?.toLowerCase().replaceAll("-", ":").split(":").slice(0, 3).join(":");
  if (!oui) return null;
  if (["d0:11:e5", "a8:20:66", "bc:d0:74", "f0:18:98", "3c:22:fb"].includes(oui)) return "Apple";
  return null;
}

const services = new Map();
let nextServiceId = 1;
let activeScanPromise = null;

const scanStatus = {
  phase: "idle",
  selectedDevices: 0,
  scannedDevices: 0,
  discoveredServices: 0,
  currentDeviceIp: null,
  startedAt: null,
  finishedAt: null,
};

const settings = { ...defaultSettings, ...(persistedState.settings ?? {}) };
const favorites = new Set(Array.isArray(persistedState.favorites) ? persistedState.favorites : []);

function migrateFavoriteKeys(oldDeviceId, newDeviceId) {
  let changed = false;
  const oldPrefix = `${oldDeviceId}:`;
  for (const key of [...favorites]) {
    if (!key.startsWith(oldPrefix)) continue;
    favorites.delete(key);
    favorites.add(`${newDeviceId}:${key.slice(oldPrefix.length)}`);
    changed = true;
  }
  return changed;
}

function json(response, status, value) {
  const body = JSON.stringify(value);
  response.writeHead(status, {
    "content-type": "application/json",
    "cache-control": "no-store",
    "access-control-allow-origin": "*",
    "access-control-allow-methods": "GET,POST,PATCH,OPTIONS",
    "access-control-allow-headers": "content-type",
  });
  response.end(body);
}

function text(response, status, value) {
  response.writeHead(status, { "content-type": "text/plain; charset=utf-8" });
  response.end(value);
}

async function bodyJson(request) {
  let body = "";
  for await (const chunk of request) body += chunk;
  return body ? JSON.parse(body) : {};
}

function retainedServices() {
  const cutoff = Date.now() - settings.retentionDays * 24 * 60 * 60 * 1000;
  return [...services.values()]
    .filter((service) => {
      const device = devices.get(service.deviceId);
      if (!device?.selected || device.ignored) return false;
      return service.active || new Date(service.lastSeen).getTime() >= cutoff;
    })
    .sort(compareServices);
}

function hasPageTitle(service) {
  return Boolean(service?.title?.trim());
}

function compareIp(a, b) {
  const left = a.split(".").map(Number);
  const right = b.split(".").map(Number);
  for (let index = 0; index < Math.max(left.length, right.length); index += 1) {
    const diff = (left[index] ?? 0) - (right[index] ?? 0);
    if (diff !== 0) return diff;
  }
  return a.localeCompare(b);
}

function compareServices(a, b) {
  return (
    Number(b.active) - Number(a.active) ||
    Number(hasPageTitle(b)) - Number(hasPageTitle(a)) ||
    (a.title?.trim() || `${a.ip}:${a.port}`).localeCompare(b.title?.trim() || `${b.ip}:${b.port}`, undefined, {
      numeric: true,
      sensitivity: "base",
    }) ||
    compareIp(a.ip, b.ip) ||
    a.port - b.port
  );
}

function tcpOpen(ip, port) {
  return new Promise((resolve) => {
    const socket = net.connect({ host: ip, port });
    const done = (value) => {
      socket.destroy();
      resolve(value);
    };
    socket.setTimeout(settings.connectTimeoutMs);
    socket.once("connect", () => done(true));
    socket.once("timeout", () => done(false));
    socket.once("error", () => done(false));
  });
}

function httpProbe(ip, port, scheme, redirectCount = 0, path = "/") {
  return new Promise((resolve) => {
    const client = scheme === "https" ? https : http;
    const request = client.get(
      {
        host: ip,
        port,
        path,
        timeout: settings.httpTimeoutMs,
        headers: { "user-agent": "LAN-Web-UI-Finder/dev" },
        rejectUnauthorized: false,
      },
      (response) => {
        const location = response.headers.location;
        if (
          redirectCount < 3 &&
          response.statusCode &&
          response.statusCode >= 300 &&
          response.statusCode < 400 &&
          typeof location === "string"
        ) {
          response.resume();
          const nextUrl = new URL(location, `${scheme}://${ip}:${port}/`);
          if (nextUrl.hostname === ip && Number(nextUrl.port || port) === port) {
            httpProbe(
              ip,
              port,
              nextUrl.protocol.replace(":", ""),
              redirectCount + 1,
              `${nextUrl.pathname}${nextUrl.search}`
            ).then(resolve);
            return;
          }
        }

        let body = "";
        response.setEncoding("utf8");
        response.on("data", (chunk) => {
          if (body.length < 256_000) body += chunk;
        });
        response.on("end", () => {
          const title = extractPageTitle(body);
          resolve({
            scheme,
            url: `${scheme}://${ip}:${port}/`,
            title: title ? title.slice(0, 140) : null,
            statusCode: response.statusCode ?? null,
            server: response.headers.server ?? null,
          });
        });
      },
    );
    request.setTimeout(settings.httpTimeoutMs, () => {
      request.destroy();
      resolve(null);
    });
    request.on("error", () => resolve(null));
  });
}

function extractPageTitle(body) {
  const title = normalizeTitle(body.match(/<title[^>]*>(.*?)<\/title>/is)?.[1]);
  const metadataTitle = extractMetaTitle(body);
  if (title?.toLowerCase().startsWith("login - ")) return metadataTitle ?? title;
  return title ?? metadataTitle;
}

function extractMetaTitle(body) {
  const names = "application-name|apple-mobile-web-app-title|og:site_name|og:title|description";
  const patterns = [
    new RegExp(`<meta[^>]+(?:name|property)=["'](?:${names})["'][^>]+content=["']([^"']+)["'][^>]*>`, "is"),
    new RegExp(`<meta[^>]+content=["']([^"']+)["'][^>]+(?:name|property)=["'](?:${names})["'][^>]*>`, "is"),
  ];
  for (const pattern of patterns) {
    const title = normalizeTitle(body.match(pattern)?.[1]);
    if (title) return title;
  }
  return null;
}

function normalizeTitle(value) {
  const title = value?.replace(/\s+/g, " ").trim();
  return title ? title.slice(0, 140) : null;
}

async function probePort(ip, port) {
  if (ip === primaryIp && port === API_PORT) return null;
  if (!(await tcpOpen(ip, port))) return null;
  const httpHit = await httpProbe(ip, port, "http");
  if (hasPageTitle(httpHit)) return httpHit;
  const httpsHit = await httpProbe(ip, port, "https");
  if (hasPageTitle(httpsHit)) return httpsHit;
  return httpHit ?? httpsHit;
}

async function scanDevice(device) {
  const scanStartedAt = nowIso();
  let found = 0;
  let nextPort = 1;
  const workers = Array.from({ length: settings.scanConcurrency }, async () => {
    while (nextPort <= 65535) {
      const port = nextPort++;
      const hit = await probePort(device.ip, port);
      if (!hit) continue;

      const key = `${device.id}:${port}`;
      const existing = services.get(key);
      const timestamp = nowIso();
      services.set(key, {
        id: existing?.id ?? nextServiceId++,
        deviceId: device.id,
        ip: device.ip,
        port,
        scheme: hit.scheme,
        url: hit.url,
        title: hit.title,
        statusCode: hit.statusCode,
        server: hit.server,
        firstSeen: existing?.firstSeen ?? timestamp,
        lastSeen: timestamp,
        lastChecked: timestamp,
        active: true,
        lastFailure: null,
      });
      found++;
    }
  });
  await Promise.all(workers);

  for (const [key, service] of services) {
    if (service.deviceId === device.id && service.active && service.lastChecked < scanStartedAt) {
      services.set(key, {
        ...service,
        active: false,
        lastChecked: nowIso(),
        lastFailure: "Not seen in latest scan",
      });
    }
  }

  return found;
}

function markDeviceServicesInactive(device, failure) {
  const timestamp = nowIso();
  for (const [key, service] of services) {
    if (service.deviceId !== device.id) continue;
    services.set(key, {
      ...service,
      active: false,
      lastChecked: timestamp,
      lastFailure: failure,
    });
  }
}

async function refreshDeviceServices(device) {
  if (!device.selected || device.ignored) {
    markDeviceServicesInactive(device, "Device disabled for scanning");
    return { scannedDevices: 0, discoveredServices: 0 };
  }

  Object.assign(scanStatus, {
    phase: "scanning",
    selectedDevices: 1,
    scannedDevices: 0,
    discoveredServices: 0,
    currentDeviceIp: device.ip,
    startedAt: nowIso(),
    finishedAt: null,
  });

  const discoveredServices = await scanDevice(device);
  Object.assign(scanStatus, {
    phase: "idle",
    currentDeviceIp: null,
    scannedDevices: 1,
    discoveredServices,
    finishedAt: nowIso(),
  });

  return { scannedDevices: 1, discoveredServices };
}

async function scanSelected() {
  if (activeScanPromise) return activeScanPromise;

  activeScanPromise = (async () => {
    let discoveredServices = 0;
    const selected = [...devices.values()].filter((device) => device.selected && !device.ignored);
    Object.assign(scanStatus, {
      phase: "scanning",
      selectedDevices: selected.length,
      scannedDevices: 0,
      discoveredServices: 0,
      currentDeviceIp: null,
      startedAt: nowIso(),
      finishedAt: null,
    });
    for (const device of selected) {
      scanStatus.currentDeviceIp = device.ip;
      device.lastSeen = nowIso();
      discoveredServices += await scanDevice(device);
      scanStatus.scannedDevices += 1;
      scanStatus.discoveredServices = discoveredServices;
    }
    const result = { scannedDevices: selected.length, discoveredServices };
    Object.assign(scanStatus, {
      phase: "idle",
      currentDeviceIp: null,
      scannedDevices: result.scannedDevices,
      discoveredServices: result.discoveredServices,
      finishedAt: nowIso(),
    });
    return result;
  })();

  try {
    return await activeScanPromise;
  } finally {
    activeScanPromise = null;
  }
}

const server = http.createServer(async (request, response) => {
  try {
    if (request.method === "OPTIONS") return json(response, 204, {});
    const url = new URL(request.url ?? "/", `http://${request.headers.host}`);

    if (request.method === "GET" && url.pathname === "/api/devices") return json(response, 200, [...devices.values()]);
    if (request.method === "POST" && url.pathname === "/api/devices/refresh") return json(response, 200, await discoverDevices());
    if (request.method === "GET" && url.pathname === "/api/services") return json(response, 200, retainedServices());
    if (request.method === "GET" && url.pathname === "/api/scan/status") return json(response, 200, scanStatus);
    if (request.method === "GET" && url.pathname === "/api/favorites") return json(response, 200, [...favorites]);
    if (request.method === "PATCH" && url.pathname === "/api/favorites") {
      const patch = await bodyJson(request);
      const key = typeof patch.serviceKey === "string" ? patch.serviceKey.trim() : "";
      if (patch.favorite && key) favorites.add(key);
      else favorites.delete(key);
      await savePersistedState();
      return json(response, 200, [...favorites]);
    }
    if (request.method === "GET" && url.pathname === "/api/settings") {
      return json(response, 200, {
        settings,
        actualDashboardPort: API_PORT,
        dashboardUrls: localIps.map((ip) => `http://${ip}:${API_PORT}`),
      });
    }
    if (request.method === "PATCH" && url.pathname === "/api/settings") {
      Object.assign(settings, await bodyJson(request));
      await savePersistedState();
      return json(response, 200, {
        settings,
        actualDashboardPort: API_PORT,
        dashboardUrls: localIps.map((ip) => `http://${ip}:${API_PORT}`),
      });
    }
    if (request.method === "PATCH" && url.pathname.startsWith("/api/devices/")) {
      const id = decodeURIComponent(url.pathname.slice("/api/devices/".length));
      const device = devices.get(id);
      if (!device) return text(response, 404, "Device not found");
      const patch = await bodyJson(request);
      const previousSelected = device.selected;
      const previousIgnored = device.ignored;
      if (typeof patch.selected === "boolean") device.selected = patch.selected;
      if (typeof patch.ignored === "boolean") device.ignored = patch.ignored;
      if (Object.hasOwn(patch, "nameOverride")) {
        device.nameOverride = patch.nameOverride?.trim() || null;
        if (device.nameOverride) persistedState.deviceAliases[device.id] = device.nameOverride;
        else delete persistedState.deviceAliases[device.id];
      }
      await savePersistedState();
      if (previousSelected !== device.selected || previousIgnored !== device.ignored) {
        if (device.selected && !device.ignored) void refreshDeviceServices(device);
        else markDeviceServicesInactive(device, "Device disabled for scanning");
      }
      return json(response, 200, device);
    }
    if (request.method === "POST" && url.pathname === "/api/scan") return json(response, 200, await scanSelected());

    return text(response, 404, "Not found");
  } catch (error) {
    console.error(error);
    return text(response, 500, error instanceof Error ? error.message : String(error));
  }
});

server.listen(API_PORT, "0.0.0.0", async () => {
  console.log(`LANVibe dev API listening on http://0.0.0.0:${API_PORT}`);
  await discoverDevices();
  console.log(`Devices: ${[...devices.values()].map((device) => device.ip).join(", ")}`);
  void scanSelected().then((result) => console.log(`Initial scan finished: ${JSON.stringify(result)}`));
});
