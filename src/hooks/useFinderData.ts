import { useCallback, useEffect, useRef, useState } from "react";

import {
  getDiscoveryStatus,
  getSettings,
  getScanStatus,
  killServiceProcess,
  listDevices,
  listServices,
  refreshDevices,
  startScan,
  updateDevice,
  updateSettings,
} from "@/api";
import { emptySettings } from "@/lib/finder";
import type {
  Device,
  DiscoveryStatus,
  ScanStatus,
  Service,
  Settings,
  SettingsView,
} from "@/types";

const POLL_INTERVAL_MS = 5000;

export interface FinderData {
  devices: Device[];
  services: Service[];
  settingsView: SettingsView;
  loading: boolean;
  devicesLoaded: boolean;
  busy: string | null;
  scanStatus: ScanStatus;
  discoveryStatus: DiscoveryStatus;
  error: string | null;
  refreshAll: () => Promise<void>;
  killProcess: (service: Service) => Promise<void>;
  scan: () => Promise<void>;
  discoverDevices: () => Promise<void>;
  setDeviceSelected: (device: Device, selected: boolean) => Promise<void>;
  setDeviceName: (device: Device, nameOverride: string | null) => Promise<void>;
  saveSettings: (settings: Settings) => Promise<void>;
}

export function useFinderData(): FinderData {
  const [devices, setDevices] = useState<Device[]>([]);
  const [services, setServices] = useState<Service[]>([]);
  const [settingsView, setSettingsView] = useState<SettingsView>({
    settings: emptySettings,
    actualDashboardPort: emptySettings.dashboardPort,
    dashboardUrls: [],
    canOpenLoopbackServices: true,
  });
  const [loading, setLoading] = useState(true);
  const [devicesLoaded, setDevicesLoaded] = useState(false);
  const [busy, setBusy] = useState<string | null>(null);
  const [scanStatus, setScanStatus] = useState<ScanStatus>({
    phase: "idle",
    selectedDevices: 0,
    scannedDevices: 0,
    discoveredServices: 0,
  });
  const [discoveryStatus, setDiscoveryStatus] = useState<DiscoveryStatus>({
    phase: "idle",
    discoveredDevices: 0,
  });
  const [error, setError] = useState<string | null>(null);
  const scanningRef = useRef(false);

  const mergeServiceState = useCallback(
    (nextServices: Service[], retainMissing: boolean) => {
      setServices((current) => {
        if (current.length === 0) return nextServices;

        const nextByKey = new Map(
          nextServices.map((service) => [serviceIdentity(service), service])
        );
        const seen = new Set<string>();
        const merged: Service[] = [];

        for (const service of current) {
          const key = serviceIdentity(service);
          const next = nextByKey.get(key);
          if (next) {
            merged.push({ ...service, ...next });
            seen.add(key);
          } else if (retainMissing) {
            merged.push(service);
          }
        }

        for (const service of nextServices) {
          const key = serviceIdentity(service);
          if (!seen.has(key)) merged.push(service);
        }

        return merged;
      });
    },
    []
  );

  const refreshAll = useCallback(async (servicesFirst = false) => {
    try {
      setError(null);

      if (servicesFirst) {
        const servicesPromise = listServices();
        const devicesPromise = listDevices();
        const settingsPromise = getSettings();
        const scanStatusPromise = getScanStatus();
        const discoveryStatusPromise = getDiscoveryStatus();

        try {
          const nextServices = await servicesPromise;
          mergeServiceState(nextServices, scanningRef.current);
        } catch (cause) {
          setError(cause instanceof Error ? cause.message : String(cause));
        } finally {
          setLoading(false);
        }

        try {
          const [
            nextDevices,
            nextSettings,
            nextScanStatus,
            nextDiscoveryStatus,
          ] = await Promise.all([
            devicesPromise,
            settingsPromise,
            scanStatusPromise,
            discoveryStatusPromise,
          ]);
          setDevices(nextDevices);
          setDevicesLoaded(true);
          setSettingsView(nextSettings);
          setScanStatus((current) =>
            current.phase === "starting" || current.phase === "updating"
              ? current
              : nextScanStatus
          );
          setDiscoveryStatus(nextDiscoveryStatus);
        } catch (cause) {
          setError(cause instanceof Error ? cause.message : String(cause));
          setDevicesLoaded(true);
        }
        return;
      }

      const [
        nextDevices,
        nextServices,
        nextSettings,
        nextScanStatus,
        nextDiscoveryStatus,
      ] =
        await Promise.all([
          listDevices(),
          listServices(),
          getSettings(),
          getScanStatus(),
          getDiscoveryStatus(),
        ]);
      setDevices(nextDevices);
      setDevicesLoaded(true);
      mergeServiceState(
        nextServices,
        scanningRef.current || nextScanStatus.phase === "scanning"
      );
      setSettingsView(nextSettings);
      setScanStatus((current) =>
        current.phase === "starting" || current.phase === "updating"
          ? current
          : nextScanStatus
      );
      setDiscoveryStatus(nextDiscoveryStatus);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
      setDevicesLoaded(true);
    } finally {
      setLoading(false);
    }
  }, [mergeServiceState]);

  useEffect(() => {
    void refreshAll(true);
    const id = window.setInterval(() => void refreshAll(), POLL_INTERVAL_MS);
    return () => window.clearInterval(id);
  }, [refreshAll]);

  useEffect(() => {
    if (
      scanStatus.phase !== "starting" &&
      scanStatus.phase !== "scanning" &&
      scanStatus.phase !== "updating"
    ) {
      return;
    }

    let cancelled = false;
    const pollScanStatus = async () => {
      try {
        const next = await getScanStatus();
        if (!cancelled) setScanStatus(next);
      } catch {
        // The regular refresh loop owns user-visible error handling.
      }
    };

    const id = window.setInterval(() => void pollScanStatus(), 1000);
    void pollScanStatus();
    return () => {
      cancelled = true;
      window.clearInterval(id);
    };
  }, [scanStatus.phase]);

  useEffect(() => {
    if (!window.__TAURI_INTERNALS__) return;

    const unlisteners: Array<() => void> = [];
    void import("@tauri-apps/api/event").then(async ({ listen }) => {
      unlisteners.push(
        await listen<DiscoveryStatus>("discovery-status", (event) => {
          setDiscoveryStatus(event.payload);
        })
      );
      unlisteners.push(
        await listen<Device[]>("devices-updated", (event) => {
          setDevices(event.payload);
          setDevicesLoaded(true);
        })
      );
    });

    return () => unlisteners.forEach((unlisten) => unlisten());
  }, []);

  const runBusy = useCallback(
    async <T>(label: string, action: () => Promise<T>): Promise<T | undefined> => {
      setBusy(label);
      setError(null);
      try {
        return await action();
      } catch (cause) {
        setError(cause instanceof Error ? cause.message : String(cause));
        return undefined;
      } finally {
        setBusy(null);
      }
    },
    []
  );

  const scan = useCallback(async () => {
    scanningRef.current = true;
    setScanStatus((current) => ({ ...current, phase: "starting" }));
    await runBusy("scan", async () => {
      setScanStatus((current) => ({ ...current, phase: "scanning" }));
      const result = await startScan();
      const latestScanStatus = await getScanStatus();
      if (isActiveScan(latestScanStatus)) {
        setScanStatus(latestScanStatus);
        return;
      }
      setScanStatus((current) => ({
        ...current,
        phase: "updating",
        scannedDevices: result.scannedDevices,
        discoveredServices: result.discoveredServices,
      }));
      scanningRef.current = false;
      await refreshAll();
      setScanStatus((current) => ({
        ...current,
        phase: "idle",
        scannedDevices: result.scannedDevices,
        discoveredServices: result.discoveredServices,
      }));
    });
    scanningRef.current = false;
    const latestScanStatus = await getScanStatus().catch(() => null);
    if (latestScanStatus && isActiveScan(latestScanStatus)) {
      setScanStatus(latestScanStatus);
      return;
    }
    setScanStatus((current) =>
      current.phase === "idle" ? current : { ...current, phase: "idle" }
    );
  }, [refreshAll, runBusy]);

  const killProcess = useCallback(
    async (service: Service) => {
      setBusy(`kill-${service.id}`);
      setError(null);
      try {
        await killServiceProcess(service.id);
        await refreshAll(true);
      } catch (cause) {
        const message = cause instanceof Error ? cause.message : String(cause);
        setError(message);
        throw new Error(message);
      } finally {
        setBusy(null);
      }
    },
    [refreshAll]
  );

  const discoverDevices = useCallback(async () => {
    await runBusy("devices", async () => {
      setDiscoveryStatus((current) => ({
        ...current,
        phase: "discovering",
        startedAt:
          current.phase === "discovering"
            ? current.startedAt
            : new Date().toISOString(),
        finishedAt: null,
      }));
      try {
        const nextDevices = await refreshDevices();
        const nextDiscoveryStatus = await getDiscoveryStatus();
        setDevices(nextDevices);
        setDevicesLoaded(true);
        setDiscoveryStatus(nextDiscoveryStatus);
      } catch (cause) {
        setDiscoveryStatus((current) => ({
          ...current,
          phase: "idle",
          finishedAt: new Date().toISOString(),
        }));
        throw cause;
      }
    });
  }, [runBusy]);

  const setDeviceSelected = useCallback(
    async (device: Device, selected: boolean) => {
      const next = await runBusy(`device-${device.id}`, () =>
        updateDevice(device.id, selected, device.ignored, device.nameOverride)
      );
      if (!next) return;
      setDevices((current) =>
        current.map((item) => (item.id === next.id ? next : item))
      );
      if (!next.selected || next.ignored) {
        setServices((current) =>
          current.filter((service) => service.deviceId !== next.id)
        );
      }
      await refreshAll();
    },
    [refreshAll, runBusy]
  );

  const setDeviceName = useCallback(
    async (device: Device, nameOverride: string | null) => {
      const next = await runBusy(`device-${device.id}`, () =>
        updateDevice(device.id, device.selected, device.ignored, nameOverride)
      );
      if (!next) return;
      setDevices((current) =>
        current.map((item) => (item.id === next.id ? next : item))
      );
    },
    [runBusy]
  );

  const saveSettings = useCallback(
    async (settings: Settings) => {
      const saved = await runBusy("settings", () => updateSettings(settings));
      if (saved) setSettingsView(saved);
    },
    [runBusy]
  );

  return {
    devices,
    services,
    settingsView,
    loading,
    devicesLoaded,
    busy,
    scanStatus,
    discoveryStatus,
    error,
    refreshAll,
    killProcess,
    scan,
    discoverDevices,
    setDeviceSelected,
    setDeviceName,
    saveSettings,
  };
}

function serviceIdentity(service: Service) {
  return `${service.deviceId}:${service.port}`;
}

function isActiveScan(status: ScanStatus) {
  return (
    status.phase === "starting" ||
    status.phase === "scanning" ||
    status.phase === "updating"
  );
}
