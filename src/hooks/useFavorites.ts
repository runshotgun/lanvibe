import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { listFavorites, reorderFavorites as apiReorderFavorites, setFavorite } from "@/api";
import { serviceKey } from "@/lib/finder";
import type { Service } from "@/types";

export function useFavorites() {
  const [favoriteKeys, setFavoriteKeys] = useState<string[]>([]);
  const [loading, setLoading] = useState(true);
  // While a reorder/toggle is awaiting the server, skip background poll results
  // so an in-flight optimistic order isn't clobbered by stale data.
  const mutating = useRef(false);

  const favorites = useMemo(() => new Set(favoriteKeys), [favoriteKeys]);

  const refreshFavorites = useCallback(async () => {
    try {
      const saved = await listFavorites();
      if (!mutating.current) setFavoriteKeys(saved);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    let cancelled = false;
    async function load() {
      if (mutating.current) return;
      try {
        const saved = await listFavorites();
        if (!cancelled && !mutating.current) setFavoriteKeys(saved);
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
      const previous = favoriteKeys;
      const optimistic = favorite
        ? [...favoriteKeys.filter((item) => item !== key), key]
        : favoriteKeys.filter((item) => item !== key);
      setFavoriteKeys(optimistic);

      mutating.current = true;
      try {
        const saved = await setFavorite(key, favorite);
        setFavoriteKeys(saved);
      } catch (error) {
        console.error("Unable to save favorite", error);
        setFavoriteKeys(previous);
      } finally {
        mutating.current = false;
      }
    },
    [favoriteKeys, favorites]
  );

  const reorderFavorites = useCallback(
    async (orderedKeys: string[]) => {
      const previous = favoriteKeys;
      setFavoriteKeys(orderedKeys);

      mutating.current = true;
      try {
        const saved = await apiReorderFavorites(orderedKeys);
        setFavoriteKeys(saved);
      } catch (error) {
        console.error("Unable to reorder favorites", error);
        setFavoriteKeys(previous);
      } finally {
        mutating.current = false;
      }
    },
    [favoriteKeys]
  );

  return {
    favorites,
    favoriteKeys,
    loading,
    isFavorite,
    toggleFavorite,
    reorderFavorites,
    refreshFavorites,
  };
}
