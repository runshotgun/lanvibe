import { useEffect, useMemo, useState } from "react";

import { DevicesView } from "@/components/devices/DevicesView";
import { FavoritesView } from "@/components/favorites/FavoritesView";
import { AppShell, type Tab } from "@/components/layout/AppShell";
import { ServicesView } from "@/components/services/ServicesView";
import { SettingsView } from "@/components/settings/SettingsView";
import { useFavorites } from "@/hooks/useFavorites";
import { useFinderData } from "@/hooks/useFinderData";
import type { Device, Service } from "@/types";

const FAVORITES_CACHE_KEY = "lanvibe:favorites-cache:v1";

interface CachedFavorites {
  version: 1;
  savedAt: string;
  devices: Device[];
  services: Service[];
}

function readFavoritesCache(): CachedFavorites | null {
  try {
    const raw = window.localStorage.getItem(FAVORITES_CACHE_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as Partial<CachedFavorites>;
    if (
      parsed.version !== 1 ||
      !Array.isArray(parsed.devices) ||
      !Array.isArray(parsed.services)
    ) {
      return null;
    }
    return parsed as CachedFavorites;
  } catch {
    return null;
  }
}

function writeFavoritesCache(cache: CachedFavorites | null) {
  try {
    if (!cache) {
      window.localStorage.removeItem(FAVORITES_CACHE_KEY);
      return;
    }
    window.localStorage.setItem(FAVORITES_CACHE_KEY, JSON.stringify(cache));
  } catch {
    // localStorage can be unavailable in hardened browser contexts.
  }
}

export default function App() {
  const [tab, setTab] = useState<Tab>("favorites");
  const [cachedFavorites, setCachedFavorites] = useState<CachedFavorites | null>(
    () => readFavoritesCache()
  );
  const data = useFinderData();
  const {
    loading: favoritesLoading,
    favoriteKeys,
    isFavorite,
    toggleFavorite,
    reorderFavorites,
  } = useFavorites();
  const visibleServices = useMemo(() => {
    if (!data.devicesLoaded) return [];

    const visibleDeviceIds = new Set(
      data.devices
        .filter((device) => device.selected && !device.ignored)
        .map((device) => device.id)
    );

    return data.services.filter((service) =>
      visibleDeviceIds.has(service.deviceId)
    );
  }, [data.devices, data.devicesLoaded, data.services]);
  const favoriteServices = useMemo(
    () => visibleServices.filter((service) => isFavorite(service)),
    [isFavorite, visibleServices]
  );
  const liveFavoritesReady =
    !data.loading && !favoritesLoading && data.devicesLoaded;

  useEffect(() => {
    if (!window.__TAURI_INTERNALS__) return;
    const unlisteners: Array<() => void> = [];
    void import("@tauri-apps/api/event").then(async ({ listen }) => {
      unlisteners.push(await listen("manual-scan-requested", () => {
        void data.scan();
      }));
      unlisteners.push(await listen("settings-updated", () => {
        void data.refreshAll();
      }));
    });
    return () => unlisteners.forEach((unlisten) => unlisten());
  }, [data.refreshAll, data.scan]);

  useEffect(() => {
    if (!liveFavoritesReady) return;

    if (favoriteServices.length === 0) {
      setCachedFavorites(null);
      writeFavoritesCache(null);
      return;
    }

    const cache: CachedFavorites = {
      version: 1,
      savedAt: new Date().toISOString(),
      devices: data.devices,
      services: favoriteServices,
    };
    setCachedFavorites(cache);
    writeFavoritesCache(cache);
  }, [data.devices, favoriteServices, liveFavoritesReady]);

  return (
    <AppShell
      activeTab={tab}
      onTabChange={setTab}
      error={data.error}
    >
      {tab === "favorites" ? (
        <FavoritesView
          devices={data.devices}
          services={visibleServices}
          cachedDevices={cachedFavorites?.devices ?? []}
          cachedServices={cachedFavorites?.services ?? []}
          favoriteKeys={favoriteKeys}
          isFavorite={isFavorite}
          onFavorite={toggleFavorite}
          onReorder={reorderFavorites}
          onRefresh={data.refreshAll}
          loading={data.loading || favoritesLoading || !data.devicesLoaded}
        />
      ) : null}

      {tab === "services" ? (
        <ServicesView
          devices={data.devices}
          services={visibleServices}
          loading={data.loading}
          scanning={data.busy === "scan"}
          scanStatus={data.scanStatus}
          onScan={() => void data.scan()}
          isFavorite={isFavorite}
          onFavorite={toggleFavorite}
        />
      ) : null}

      {tab === "devices" ? (
        <DevicesView
          devices={data.devices}
          discovering={
            data.busy === "devices" ||
            data.discoveryStatus.phase === "discovering"
          }
          onDiscover={() => void data.discoverDevices()}
          onToggle={(device, selected) =>
            void data.setDeviceSelected(device, selected)
          }
          onRename={(device, name) => void data.setDeviceName(device, name)}
        />
      ) : null}

      {tab === "settings" ? (
        <SettingsView
          value={data.settingsView}
          saving={data.busy === "settings"}
          onChange={(settings) => void data.saveSettings(settings)}
        />
      ) : null}
    </AppShell>
  );
}
