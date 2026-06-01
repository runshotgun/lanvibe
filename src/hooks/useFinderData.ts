import { useCallback, useEffect, useRef, useState } from "react";

import {
  getSettings,
  getScanStatus,
  listDevices,
  listServices,
  refreshDevices,
  startScan,
  updateDevice,
  updateSettings,
} from "@/api";
import { emptySettings } from "@/lib/finder";
import type { Device, ScanStatus, Service, Settings, SettingsView } from "@/types";

const POLL_INTERVAL_MS = 5000;

export interface FinderData {
  devices: Device[];
  services: Service[];
  settingsView: SettingsView;
  loading: boolean;
  devicesLoaded: boolean;
  busy: string | null;
  scanStatus: ScanStatus;
  error: string | null;
  refreshAll: () => Promise<void>;
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

        try {
          const nextServices = await servicesPromise;
          mergeServiceState(nextServices, scanningRef.current);
        } catch (cause) {
          setError(cause instanceof Error ? cause.message : String(cause));
        } finally {
          setLoading(false);
        }

        try {
          const [nextDevices, nextSettings, nextScanStatus] = await Promise.all([
            devicesPromise,
            settingsPromise,
            scanStatusPromise,
          ]);
          setDevices(nextDevices);
          setDevicesLoaded(true);
          setSettingsView(nextSettings);
          setScanStatus((current) =>
            current.phase === "starting" || current.phase === "updating"
              ? current
              : nextScanStatus
          );
        } catch (cause) {
          setError(cause instanceof Error ? cause.message : String(cause));
          setDevicesLoaded(true);
        }
        return;
      }

      const [nextDevices, nextServices, nextSettings, nextScanStatus] =
        await Promise.all([
          listDevices(),
          listServices(),
          getSettings(),
          getScanStatus(),
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
    setScanStatus((current) =>
      current.phase === "idle" ? current : { ...current, phase: "idle" }
    );
  }, [refreshAll, runBusy]);

  const discoverDevices = useCallback(async () => {
    await runBusy("devices", async () => {
      const nextDevices = await refreshDevices();
      setDevices(nextDevices);
      setDevicesLoaded(true);
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
    error,
    refreshAll,
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
