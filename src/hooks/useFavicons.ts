import { useEffect, useRef, useState } from "react";

import { getFavicon } from "@/api";

/**
 * Resolves favicon data URLs for a set of service origins, caching each result
 * (including misses) for the lifetime of the component so we never refetch.
 */
export function useFavicons(origins: string[]): Record<string, string | null> {
  const [icons, setIcons] = useState<Record<string, string | null>>({});
  const cache = useRef<Record<string, string | null>>({});
  const key = origins.join("|");

  useEffect(() => {
    const missing = origins.filter((origin) => !(origin in cache.current));
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
      if (cancelled) return;
      const next = { ...cache.current };
      for (const [origin, url] of entries) next[origin] = url;
      cache.current = next;
      setIcons(next);
    });

    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key]);

  return icons;
}
