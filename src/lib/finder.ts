import type { Device, Service, Settings } from "@/types";

export function formatTime(value?: string | null): string {
  if (!value) return "Never";
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return new Intl.DateTimeFormat(undefined, {
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
  }).format(date);
}

export function deviceName(device: Device): string {
  return device.nameOverride || device.hostname || device.ip;
}

export function serviceLabel(service: Service, devices: Device[]): string {
  const device = devices.find((item) => item.id === service.deviceId);
  return `${device ? deviceName(device) : service.ip}:${service.port}`;
}

export function serviceHostName(service: Service, devices: Device[]): string {
  const device = devices.find((item) => item.id === service.deviceId);
  return device ? deviceName(device) : service.ip;
}

export function serviceKey(service: Service): string {
  return `${service.deviceId}:${service.port}`;
}

export function serviceOrigin(service: Service): string {
  return `${service.scheme}://${service.ip}:${service.port}`;
}

export function hasPageTitle(service: Service): boolean {
  return Boolean(service.title?.trim());
}

export function serviceProcessOwner(service: Service): string | null {
  const owner = service.processOwner?.trim();
  return owner || null;
}

export function serviceSortLabel(service: Service, devices: Device[]): string {
  return service.title?.trim() || serviceLabel(service, devices);
}

export function compareServices(a: Service, b: Service, devices: Device[]): number {
  const activeDiff = Number(b.active) - Number(a.active);
  if (activeDiff !== 0) return activeDiff;

  const titleDiff = Number(hasPageTitle(b)) - Number(hasPageTitle(a));
  if (titleDiff !== 0) return titleDiff;

  const labelDiff = serviceSortLabel(a, devices).localeCompare(
    serviceSortLabel(b, devices),
    undefined,
    { numeric: true, sensitivity: "base" }
  );
  if (labelDiff !== 0) return labelDiff;

  const ipDiff = compareIp(a.ip, b.ip);
  if (ipDiff !== 0) return ipDiff;

  return a.port - b.port;
}

export function compareIp(a: string, b: string): number {
  const left = a.split(".").map(Number);
  const right = b.split(".").map(Number);
  for (let index = 0; index < Math.max(left.length, right.length); index += 1) {
    const diff = (left[index] ?? 0) - (right[index] ?? 0);
    if (diff !== 0) return diff;
  }
  return a.localeCompare(b);
}

export const emptySettings: Settings = {
  autoScan: true,
  manualOnly: false,
  minimizeToTray: true,
  launchAtStartup: true,
  scanIntervalSeconds: 120,
  discoveryIntervalSeconds: 60,
  retentionDays: 30,
  scanConcurrency: 512,
  connectTimeoutMs: 450,
  httpTimeoutMs: 1200,
  dashboardBind: "0.0.0.0",
  dashboardPort: 41580,
};
