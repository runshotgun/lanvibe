import type {
  Device,
  ScanResult,
  ScanStatus,
  Service,
  Settings,
  SettingsView,
  UpdateStatus,
} from "./types";

declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown;
    __TAURI__?: unknown;
  }
}

export const inTauri = () =>
  typeof window !== "undefined" &&
  Boolean(window.__TAURI_INTERNALS__ || window.__TAURI__);

async function invokeCommand<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(command, args);
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const response = await fetch(path, {
    ...init,
    headers: {
      "content-type": "application/json",
      ...(init?.headers ?? {}),
    },
  });

  const contentType = response.headers.get("content-type") ?? "";
  if (!contentType.includes("application/json")) {
    throw new Error("Dashboard API is unavailable. Start the Tauri app to use LAN discovery and scanning.");
  }

  if (!response.ok) {
    const errorBody = await response.json().catch(() => null);
    throw new Error(errorBody?.message ?? `Dashboard API request failed with ${response.status}`);
  }

  return response.json() as Promise<T>;
}

export async function listDevices(): Promise<Device[]> {
  return inTauri() ? invokeCommand<Device[]>("list_devices") : request<Device[]>("/api/devices");
}

export async function listServices(): Promise<Service[]> {
  return inTauri() ? invokeCommand<Service[]>("list_services") : request<Service[]>("/api/services");
}

export async function getScanStatus(): Promise<ScanStatus> {
  return inTauri() ? invokeCommand<ScanStatus>("get_scan_status") : request<ScanStatus>("/api/scan/status");
}

export async function getUpdateStatus(): Promise<UpdateStatus> {
  return inTauri()
    ? invokeCommand<UpdateStatus>("get_update_status")
    : request<UpdateStatus>("/api/update/status");
}

export async function triggerHostUpdate(): Promise<UpdateStatus> {
  return inTauri()
    ? invokeCommand<UpdateStatus>("trigger_host_update")
    : request<UpdateStatus>("/api/update", { method: "POST" });
}

export async function listFavorites(): Promise<string[]> {
  return inTauri() ? invokeCommand<string[]>("list_favorites") : request<string[]>("/api/favorites");
}

export async function setFavorite(serviceKey: string, favorite: boolean): Promise<string[]> {
  if (inTauri()) {
    return invokeCommand<string[]>("set_favorite", { serviceKey, favorite });
  }

  return request<string[]>("/api/favorites", {
    method: "PATCH",
    body: JSON.stringify({ serviceKey, favorite }),
  });
}

export async function reorderFavorites(serviceKeys: string[]): Promise<string[]> {
  if (inTauri()) {
    return invokeCommand<string[]>("reorder_favorites", { serviceKeys });
  }

  return request<string[]>("/api/favorites/order", {
    method: "PATCH",
    body: JSON.stringify({ serviceKeys }),
  });
}

export async function getSettings(): Promise<SettingsView> {
  return inTauri() ? invokeCommand<SettingsView>("get_settings_view") : request<SettingsView>("/api/settings");
}

export async function updateDevice(id: string, selected: boolean, ignored: boolean, nameOverride?: string | null): Promise<Device> {
  if (inTauri()) {
    return invokeCommand<Device>("update_device", { id, selected, ignored, nameOverride });
  }

  return request<Device>(`/api/devices/${encodeURIComponent(id)}`, {
    method: "PATCH",
    body: JSON.stringify({ selected, ignored, nameOverride }),
  });
}

export async function updateSettings(settings: Settings): Promise<SettingsView> {
  if (inTauri()) {
    return invokeCommand<SettingsView>("update_settings", { settings });
  }

  return request<SettingsView>("/api/settings", {
    method: "PATCH",
    body: JSON.stringify(settings),
  });
}

export async function startScan(): Promise<ScanResult> {
  return inTauri()
    ? invokeCommand<ScanResult>("start_manual_scan")
    : request<ScanResult>("/api/scan", { method: "POST" });
}

export async function refreshDevices(): Promise<Device[]> {
  return inTauri()
    ? invokeCommand<Device[]>("refresh_devices")
    : request<Device[]>("/api/devices/refresh", { method: "POST" });
}

export async function getFavicon(origin: string): Promise<string | null> {
  if (inTauri()) {
    return invokeCommand<string | null>("get_favicon", { origin });
  }

  return request<string | null>(`/api/favicon?origin=${encodeURIComponent(origin)}`);
}

export async function closePopover(): Promise<void> {
  if (inTauri()) {
    await invokeCommand<void>("close_popover");
  }
}

export async function openMainWindow(): Promise<void> {
  if (inTauri()) {
    await invokeCommand<void>("open_main_window");
  }
}

export async function resizePopover(favoriteCount: number, loading: boolean): Promise<void> {
  if (inTauri()) {
    await invokeCommand<void>("resize_popover", { favoriteCount, loading });
  }
}

export async function openService(url: string): Promise<void> {
  if (inTauri()) {
    await invokeCommand<void>("open_url", { url });
    return;
  }
}
