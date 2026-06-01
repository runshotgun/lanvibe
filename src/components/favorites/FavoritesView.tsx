import { type TouchEvent, useMemo, useRef, useState } from "react";
import { faMagnifyingGlass } from "@fortawesome/free-solid-svg-icons";
import { FontAwesomeIcon } from "@fortawesome/react-fontawesome";
import { RefreshCw, Star } from "lucide-react";

import { openService } from "@/api";
import { EmptyState } from "@/components/common/EmptyState";
import { FavoriteTile, serviceOrigin } from "@/components/favorites/FavoriteTile";
import { Input } from "@/components/ui/input";
import { Skeleton } from "@/components/ui/skeleton";
import { useFavicons } from "@/hooks/useFavicons";
import { compareServices, serviceLabel } from "@/lib/finder";
import type { Device, Service } from "@/types";

export function FavoritesView({
  devices,
  services,
  cachedDevices = [],
  cachedServices = [],
  isFavorite,
  onRefresh,
  loading,
}: {
  devices: Device[];
  services: Service[];
  cachedDevices?: Device[];
  cachedServices?: Service[];
  isFavorite: (service: Service) => boolean;
  onFavorite: (service: Service) => void;
  onRefresh: () => Promise<void>;
  loading: boolean;
}) {
  const [query, setQuery] = useState("");
  const [pullDistance, setPullDistance] = useState(0);
  const [refreshing, setRefreshing] = useState(false);
  const startY = useRef<number | null>(null);
  const pullDistanceRef = useRef(0);
  const trackingPull = useRef(false);

  const visiblePull = refreshing ? 56 : pullDistance;
  const pullProgress = Math.min(1, visiblePull / 68);
  const shouldAnimatePull = startY.current === null || refreshing;

  const favoriteRows = useMemo(
    () =>
      services
        .filter((service) => isFavorite(service))
        .sort((a, b) => compareServices(a, b, devices)),
    [devices, services, isFavorite]
  );
  const cachedRows = useMemo(
    () => [...cachedServices].sort((a, b) => compareServices(a, b, cachedDevices)),
    [cachedDevices, cachedServices]
  );
  const showingCache =
    loading && favoriteRows.length === 0 && cachedRows.length > 0;
  const displayDevices = showingCache ? cachedDevices : devices;
  const displayRows = showingCache ? cachedRows : favoriteRows;

  const filteredRows = useMemo(() => {
    const lower = query.trim().toLowerCase();
    if (!lower) return displayRows;
    return displayRows.filter((service) => {
      const haystack = [
        service.url,
        service.title,
        service.server,
        service.ip,
        service.port.toString(),
        serviceLabel(service, displayDevices),
      ]
        .filter(Boolean)
        .join(" ")
        .toLowerCase();
      return haystack.includes(lower);
    });
  }, [displayDevices, displayRows, query]);

  const origins = useMemo(() => filteredRows.map(serviceOrigin), [filteredRows]);
  const favicons = useFavicons(origins);

  const openFavorite = (service: Service) => {
    void openService(service.url);
  };

  const handleTouchStart = (event: TouchEvent<HTMLDivElement>) => {
    if (event.touches.length !== 1 || refreshing || window.scrollY > 0) return;
    startY.current = event.touches[0].clientY;
    trackingPull.current = true;
  };

  const handleTouchMove = (event: TouchEvent<HTMLDivElement>) => {
    if (!trackingPull.current || startY.current === null) return;

    const delta = event.touches[0].clientY - startY.current;
    if (delta <= 0) {
      pullDistanceRef.current = 0;
      setPullDistance(0);
      return;
    }

    if (window.scrollY <= 0) {
      event.preventDefault();
      const nextPullDistance = Math.min(92, delta * 0.46);
      pullDistanceRef.current = nextPullDistance;
      setPullDistance(nextPullDistance);
    }
  };

  const finishPull = () => {
    if (!trackingPull.current) return;

    const shouldRefresh = pullDistanceRef.current >= 64;
    trackingPull.current = false;
    startY.current = null;
    pullDistanceRef.current = 0;

    if (!shouldRefresh) {
      setPullDistance(0);
      return;
    }

    setRefreshing(true);
    setPullDistance(56);
    void onRefresh().finally(() => {
      setRefreshing(false);
      setPullDistance(0);
    });
  };

  return (
    <div
      className="relative min-h-[calc(100dvh-12rem)] md:min-h-0"
      onTouchStart={handleTouchStart}
      onTouchMove={handleTouchMove}
      onTouchEnd={finishPull}
      onTouchCancel={finishPull}
    >
      <div
        className="safe-x pointer-events-none fixed inset-x-0 top-[calc(env(safe-area-inset-top,0px)+0.75rem)] z-30 flex justify-center opacity-0 transition-opacity [--safe-left-offset:1rem] [--safe-right-offset:1rem] md:hidden"
        style={{ opacity: pullProgress }}
        aria-hidden="true"
      >
        <div
          className="flex items-center gap-2 rounded-full border border-border/70 bg-card/85 px-3 py-2 text-xs font-semibold text-foreground shadow-soft backdrop-blur-xl"
          style={{
            transform: `translateY(${-18 + visiblePull * 0.42}px)`,
          }}
        >
          <RefreshCw
            className={refreshing ? "size-3.5 animate-spin" : "size-3.5"}
            style={{
              transform: refreshing
                ? undefined
                : `rotate(${Math.round(pullProgress * 180)}deg)`,
            }}
          />
          {refreshing ? "Refreshing" : pullProgress >= 1 ? "Release" : "Pull"}
        </div>
      </div>

      <div
        className="flex flex-col gap-4"
        style={{
          transform: `translateY(${visiblePull}px)`,
          transition: shouldAnimatePull
            ? "transform 220ms cubic-bezier(0.2, 0.8, 0.2, 1)"
            : undefined,
        }}
      >
        <div className="relative">
          <FontAwesomeIcon
            icon={faMagnifyingGlass}
            className="pointer-events-none absolute left-3 top-1/2 z-10 size-4 -translate-y-1/2 text-current/60"
          />
          <Input
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="Search favorites"
            className="pl-9"
            aria-label="Search favorites"
          />
        </div>

        {loading && displayRows.length === 0 ? (
          <FavoriteSkeletonGrid />
        ) : filteredRows.length === 0 ? (
          <EmptyState
            icon={<Star />}
            title={query ? "No matching favorites" : "No favorites yet"}
            body={
              query
                ? "Try a different search term."
                : "Star a service from the Services page to pin it here."
            }
          />
        ) : (
          <div className="grid grid-cols-2 gap-2 sm:gap-3">
            {filteredRows.map((service) => (
              <FavoriteTile
                key={service.id}
                service={service}
                devices={displayDevices}
                favicon={favicons[serviceOrigin(service)]}
                loading={showingCache}
                onOpen={openFavorite}
              />
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

function FavoriteSkeletonGrid() {
  return (
    <div className="grid grid-cols-2 gap-2 sm:gap-3" aria-hidden="true">
      {Array.from({ length: 6 }).map((_, index) => (
        <div
          key={index}
          className="tactile flex min-h-[98px] flex-col gap-2 rounded-xl p-2.5 sm:min-h-[118px] sm:gap-3 sm:p-4"
        >
          <div className="flex items-start gap-2">
            <Skeleton className="size-8 rounded-md" />
            <Skeleton className="ml-auto mt-1 size-3 rounded-full" />
          </div>
          <div className="mt-auto space-y-2">
            <Skeleton className="h-4 w-4/5 rounded-full" />
            <Skeleton className="h-3 w-3/5 rounded-full" />
          </div>
        </div>
      ))}
    </div>
  );
}
