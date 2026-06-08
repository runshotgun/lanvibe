import { useCallback, useEffect, useState } from "react";

import {
  inTauri,
  killServiceProcess,
  listDevices,
  listFavorites,
  listServices,
  reorderFavorites,
  startScan,
} from "@/api";
import type { Device, Service } from "@/types";

/**
 * Lightweight data source for the tray popover. Unlike the dashboard's
 * `useFinderData`, it does not poll on a timer — it only refetches when the
 * popover is actually shown (or focused), so the kept-warm window stays idle.
 */
export function usePopoverData() {
  const [favorites, setFavorites] = useState<string[]>([]);
  const [services, setServices] = useState<Service[]>([]);
  const [devices, setDevices] = useState<Device[]>([]);
  const [loading, setLoading] = useState(true);
  const [scanning, setScanning] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const reload = useCallback(async () => {
    try {
      setError(null);
      const [favs, svcs, devs] = await Promise.all([
        listFavorites(),
        listServices(),
        listDevices(),
      ]);
      setFavorites(favs);
      setServices(svcs);
      setDevices(devs);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setLoading(false);
    }
  }, []);

  const scan = useCallback(async () => {
    setScanning(true);
    try {
      await startScan();
      await reload();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setScanning(false);
    }
  }, [reload]);

  const killProcess = useCallback(
    async (service: Service) => {
      try {
        setError(null);
        await killServiceProcess(service.id);
        await reload();
      } catch (cause) {
        const message = cause instanceof Error ? cause.message : String(cause);
        setError(message);
        throw new Error(message);
      }
    },
    [reload]
  );

  const reorder = useCallback(
    async (orderedKeys: string[]) => {
      const previous = favorites;
      setFavorites(orderedKeys);
      try {
        const saved = await reorderFavorites(orderedKeys);
        setFavorites(saved);
      } catch (cause) {
        setFavorites(previous);
        setError(cause instanceof Error ? cause.message : String(cause));
      }
    },
    [favorites]
  );

  useEffect(() => {
    void reload();
  }, [reload]);

  useEffect(() => {
    if (!inTauri()) return;
    let disposed = false;
    const cleanups: Array<() => void> = [];

    void (async () => {
      const { listen } = await import("@tauri-apps/api/event");
      const unlistenShown = await listen("popover-shown", () => void reload());
      if (disposed) unlistenShown();
      else cleanups.push(unlistenShown);

      const unlistenServices = await listen("services-updated", () => void reload());
      if (disposed) unlistenServices();
      else cleanups.push(unlistenServices);

      const unlistenFavorites = await listen<string[]>("favorites-updated", ({ payload }) => {
        setFavorites(payload);
        void reload();
      });
      if (disposed) unlistenFavorites();
      else cleanups.push(unlistenFavorites);

      const { getCurrentWindow } = await import("@tauri-apps/api/window");
      const unlistenFocus = await getCurrentWindow().onFocusChanged(
        ({ payload: focused }) => {
          if (focused) void reload();
        }
      );
      if (disposed) unlistenFocus();
      else cleanups.push(unlistenFocus);
    })();

    return () => {
      disposed = true;
      cleanups.forEach((cleanup) => cleanup());
    };
  }, [reload]);

  return {
    favorites,
    services,
    devices,
    loading,
    scanning,
    error,
    reload,
    scan,
    killProcess,
    reorder,
  };
}
