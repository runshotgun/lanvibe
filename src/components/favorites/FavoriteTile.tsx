import { type MouseEvent, type ReactNode } from "react";

import { inTauri } from "@/api";
import { ServiceFavicon } from "@/components/common/ServiceFavicon";
import { StatusDot } from "@/components/common/StatusDot";
import { serviceHostName, serviceLabel } from "@/lib/finder";
import { cn } from "@/lib/utils";
import type { Device, Service } from "@/types";

export { serviceOrigin } from "@/lib/finder";

export function FavoriteTile({
  service,
  devices,
  favicon,
  compact = false,
  loading = false,
  editing = false,
  onOpen,
}: {
  service: Service;
  devices: Device[];
  favicon?: string | null;
  compact?: boolean;
  loading?: boolean;
  editing?: boolean;
  onOpen: (service: Service) => void;
}) {
  const primary = service.title?.trim() || serviceHostName(service, devices);
  const secondary = serviceLabel(service, devices);

  const containerClass = cn(
    "group tactile flex flex-col rounded-xl text-left focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-1 focus-visible:ring-offset-background",
    compact ? "gap-2 p-2.5" : "gap-2 p-2.5 sm:gap-3 sm:p-4"
  );

  if (editing) {
    return (
      <div
        className={cn(
          containerClass,
          "relative h-full cursor-grab select-none active:cursor-grabbing"
        )}
        aria-label={`Reorder ${primary}`}
      >
        <TileContent
          favicon={favicon}
          primary={primary}
          secondary={secondary}
          active={service.active}
          loading={loading}
        />
      </div>
    );
  }

  const handleOpen = (event: MouseEvent<HTMLAnchorElement>) => {
    if (!inTauri()) return;
    event.preventDefault();
    onOpen(service);
  };

  return (
    <a
      href={service.url}
      target="_blank"
      rel="noopener noreferrer external"
      onClick={handleOpen}
      className={containerClass}
    >
      <TileContent
        favicon={favicon}
        primary={primary}
        secondary={secondary}
        active={service.active}
        loading={loading}
      />
    </a>
  );
}

function TileContent({
  favicon,
  primary,
  secondary,
  active,
  loading,
}: {
  favicon?: string | null;
  primary: ReactNode;
  secondary: ReactNode;
  active: boolean;
  loading: boolean;
}) {
  return (
    <>
      <div className="flex items-start gap-2">
        <ServiceFavicon url={favicon} />
        <StatusDot active={active} loading={loading} className="ml-auto mt-1" />
      </div>
      <div className="min-w-0">
        <div className="truncate text-sm font-semibold leading-tight">{primary}</div>
        <div className="truncate text-xs text-muted-foreground tabular-nums">{secondary}</div>
      </div>
    </>
  );
}
