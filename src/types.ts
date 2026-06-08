export interface Device {
  id: string;
  ip: string;
  hostname?: string | null;
  mac?: string | null;
  vendor?: string | null;
  nameOverride?: string | null;
  selected: boolean;
  ignored: boolean;
  source: string;
  lastSeen: string;
}

export interface Service {
  id: number;
  deviceId: string;
  ip: string;
  port: number;
  scheme: "http" | "https";
  url: string;
  title?: string | null;
  statusCode?: number | null;
  server?: string | null;
  firstSeen: string;
  lastSeen: string;
  lastChecked: string;
  active: boolean;
  lastFailure?: string | null;
  processOwner?: string | null;
}

export interface Settings {
  autoScan: boolean;
  manualOnly: boolean;
  minimizeToTray: boolean;
  launchAtStartup: boolean;
  scanIntervalSeconds: number;
  discoveryIntervalSeconds: number;
  retentionDays: number;
  scanConcurrency: number;
  connectTimeoutMs: number;
  httpTimeoutMs: number;
  dashboardBind: string;
  dashboardPort: number;
}

export interface SettingsView {
  settings: Settings;
  actualDashboardPort: number;
  dashboardUrls: string[];
  canOpenLoopbackServices: boolean;
}

export interface ScanResult {
  scannedDevices: number;
  discoveredServices: number;
}

export interface KillProcessResult {
  serviceId: number;
  port: number;
  pid: number;
  processOwner: string;
}

export type ScanPhase = "idle" | "starting" | "scanning" | "updating";

export interface ScanStatus {
  phase: ScanPhase;
  selectedDevices: number;
  scannedDevices: number;
  discoveredServices: number;
  currentDeviceIp?: string | null;
  currentDeviceScannedPorts?: number;
  currentDeviceTotalPorts?: number;
  startedAt?: string | null;
  finishedAt?: string | null;
}

export type DiscoveryPhase = "idle" | "discovering";

export interface DiscoveryStatus {
  phase: DiscoveryPhase;
  discoveredDevices: number;
  startedAt?: string | null;
  finishedAt?: string | null;
}

export type UpdatePhase =
  | "idle"
  | "checking"
  | "current"
  | "downloading"
  | "installing"
  | "restarting"
  | "error";

export interface UpdateStatus {
  phase: UpdatePhase;
  currentVersion: string;
  latestVersion?: string | null;
  downloadedBytes: number;
  totalBytes?: number | null;
  message: string;
  startedAt?: string | null;
  finishedAt?: string | null;
}
