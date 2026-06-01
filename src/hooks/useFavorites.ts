import { useCallback, useEffect, useState } from "react";

import { listFavorites, setFavorite } from "@/api";
import { serviceKey } from "@/lib/finder";
import type { Service } from "@/types";

export function useFavorites() {
  const [favorites, setFavorites] = useState<Set<string>>(() => new Set());
  const [loading, setLoading] = useState(true);

  const refreshFavorites = useCallback(async () => {
    try {
      const saved = await listFavorites();
      setFavorites(new Set(saved));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    let cancelled = false;
    async function load() {
      try {
        const saved = await listFavorites();
        if (!cancelled) setFavorites(new Set(saved));
      } catch (error) {
        console.error("Unable to load favorites", error);
      } finally {
        if (!cancelled) setLoading(false);
      }
    }

    void load();
    const interval = window.setInterval(() => void load(), 5_000);
    return () => {
      cancelled = true;
      window.clearInterval(interval);
    };
  }, []);

  const isFavorite = useCallback(
    (service: Service) => favorites.has(serviceKey(service)),
    [favorites]
  );

  const toggleFavorite = useCallback(
    async (service: Service) => {
      const key = serviceKey(service);
      const favorite = !favorites.has(key);
      const optimistic = new Set(favorites);
      if (favorite) optimistic.add(key);
      else optimistic.delete(key);
      setFavorites(optimistic);

      try {
        const saved = await setFavorite(key, favorite);
        setFavorites(new Set(saved));
      } catch (error) {
        console.error("Unable to save favorite", error);
        await refreshFavorites().catch(() => setFavorites(favorites));
      }
    },
    [favorites, refreshFavorites]
  );

  return { favorites, loading, isFavorite, toggleFavorite };
}
