import { useEffect, useRef, useState } from "react";

import { getFavicon } from "@/api";

const STORAGE_KEY = "lanvibe:favicon-cache:v1";
const cache: Record<string, string | null> = {};
let storageLoaded = false;

function loadStoredFavicons() {
  if (storageLoaded || typeof window === "undefined") return;
  storageLoaded = true;

  try {
    const stored = window.localStorage.getItem(STORAGE_KEY);
    if (!stored) return;
    const parsed = JSON.parse(stored) as unknown;
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) return;

    for (const [origin, dataUrl] of Object.entries(parsed)) {
      if (typeof dataUrl === "string" && dataUrl.startsWith("data:image/")) {
        cache[origin] = dataUrl;
      }
    }
  } catch {
    // Ignore corrupt or unavailable browser storage; the backend cache still works.
  }
}

function saveStoredFavicons() {
  if (typeof window === "undefined") return;

  try {
    const stored = Object.fromEntries(
      Object.entries(cache).filter(
        (entry): entry is [string, string] => typeof entry[1] === "string"
      )
    );
    window.localStorage.setItem(STORAGE_KEY, JSON.stringify(stored));
  } catch {
    // Quota or privacy settings should not block favicon display.
  }
}

function getCachedIcons(origins: string[]): Record<string, string | null> {
  loadStoredFavicons();

  return origins.reduce<Record<string, string | null>>((icons, origin) => {
    if (origin in cache) icons[origin] = cache[origin];
    return icons;
  }, {});
}

/**
 * Resolves favicon data URLs for service origins. Cached data URLs are hydrated
 * synchronously from browser storage, then misses are resolved through the
 * backend cache/fetch path.
 */
export function useFavicons(origins: string[]): Record<string, string | null> {
  const [icons, setIcons] = useState<Record<string, string | null>>(() => getCachedIcons(origins));
  const mounted = useRef(true);
  const key = origins.join("|");

  useEffect(() => {
    mounted.current = true;
    const cached = getCachedIcons(origins);
    if (Object.keys(cached).length > 0) {
      setIcons((current) => ({ ...current, ...cached }));
    }

    const missing = origins.filter((origin) => !(origin in cache));
    if (missing.length === 0) return;

    let cancelled = false;
    void Promise.all(
      missing.map(async (origin) => {
        try {
          return [origin, await getFavicon(origin)] as const;
        } catch {
          return [origin, null] as const;
        }
      })
    ).then((entries) => {
      if (cancelled || !mounted.current) return;
      for (const [origin, url] of entries) cache[origin] = url;
      saveStoredFavicons();
      setIcons({ ...cache });
    });

    return () => {
      cancelled = true;
      mounted.current = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key]);

  return icons;
}
