import { useMemo, useState } from "react";
import { Maximize2, RefreshCw, Star, X } from "lucide-react";

import { closePopover, openMainWindow, openService } from "@/api";
import { lanvibeLogoUrl } from "@/brand";
import { EmptyState } from "@/components/common/EmptyState";
import { FavoriteTile, serviceOrigin } from "@/components/favorites/FavoriteTile";
import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Skeleton } from "@/components/ui/skeleton";
import { useFavicons } from "@/hooks/useFavicons";
import { usePopoverData } from "@/hooks/usePopoverData";
import { serviceKey } from "@/lib/finder";
import { cn } from "@/lib/utils";
import type { Service } from "@/types";

async function hideSelf() {
  await closePopover();
}

async function openDashboard() {
  await openMainWindow();
}

export function PopoverApp() {
  const { favorites, services, devices, loading, scanning, error, scan } =
    usePopoverData();
  const [actionError, setActionError] = useState<string | null>(null);

  const favoriteServices = useMemo(() => {
    const byKey = new Map(services.map((service) => [serviceKey(service), service]));
    return favorites
      .map((key) => byKey.get(key))
      .filter((service): service is Service => Boolean(service));
  }, [favorites, services]);

  const origins = useMemo(
    () => favoriteServices.map(serviceOrigin),
    [favoriteServices]
  );
  const favicons = useFavicons(origins);

  const openFavorite = async (service: Service) => {
    try {
      setActionError(null);
      await openService(service.url);
      await hideSelf();
    } catch (cause) {
      setActionError(cause instanceof Error ? cause.message : String(cause));
    }
  };

  return (
    <div className="popover-surface fixed inset-0 flex min-h-0 flex-col overflow-hidden text-popover-foreground">
      <header className="flex items-center gap-2 border-b border-border/60 px-3 py-2.5">
        <span className="grid size-7 shrink-0 place-items-center overflow-hidden rounded-lg bg-card shadow-soft ring-1 ring-border/70">
          <img
            src={lanvibeLogoUrl}
            alt=""
            className="size-full object-cover"
            draggable={false}
          />
        </span>
        <span className="flex min-w-0 items-center gap-1.5 text-sm font-semibold tracking-tight">
          <Star className="size-3.5 shrink-0 fill-current text-warning" />
          Favorites
        </span>
        {favoriteServices.length > 0 ? (
          <span className="text-xs text-muted-foreground tabular-nums">
            {favoriteServices.length}
          </span>
        ) : null}
        <span className="ml-auto flex items-center gap-1.5">
          <Button
            variant="tactile"
            size="icon-sm"
            onClick={() => void scan()}
            disabled={scanning}
            aria-label="Scan now"
            title="Scan now"
          >
            <RefreshCw className={cn("size-4", scanning && "animate-spin")} />
          </Button>
          <Button
            variant="tactile"
            size="icon-sm"
            onClick={() => {
              void openDashboard().catch((cause) =>
                setActionError(cause instanceof Error ? cause.message : String(cause))
              );
            }}
            aria-label="Open LANVibe"
            title="Open LANVibe"
          >
            <Maximize2 className="size-4" />
          </Button>
          <Button
            variant="tactile"
            size="icon-sm"
            onClick={() => {
              void hideSelf().catch((cause) =>
                setActionError(cause instanceof Error ? cause.message : String(cause))
              );
            }}
            aria-label="Close"
            title="Close"
          >
            <X className="size-4" />
          </Button>
        </span>
      </header>

      <ScrollArea className="min-h-0 flex-1">
        <div className="p-2.5">
          {error || actionError ? (
            <p className="px-1 pb-2 text-xs text-destructive">
              {actionError ?? error}
            </p>
          ) : null}

          {loading ? (
            <div className="grid grid-cols-2 gap-2">
              {Array.from({ length: 6 }).map((_, index) => (
                <Skeleton key={index} className="h-[88px] rounded-xl" />
              ))}
            </div>
          ) : favoriteServices.length === 0 ? (
            <EmptyState
              icon={<Star />}
              title="No favorites yet"
              body="Star a service in the dashboard to pin it here for one-click access."
            />
          ) : (
            <div className="grid grid-cols-2 gap-2">
              {favoriteServices.map((service) => (
                <FavoriteTile
                  key={service.id}
                  service={service}
                  devices={devices}
                  favicon={favicons[serviceOrigin(service)]}
                  compact
                  onOpen={openFavorite}
                />
              ))}
            </div>
          )}
        </div>
      </ScrollArea>
    </div>
  );
}
