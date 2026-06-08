import { ChevronDown, Globe2, Loader2 } from "lucide-react";

import { ServiceRow } from "@/components/services/ServiceRow";
import { Badge } from "@/components/ui/badge";
import { Card } from "@/components/ui/card";
import { Separator } from "@/components/ui/separator";
import { deviceName, serviceOrigin } from "@/lib/finder";
import { cn } from "@/lib/utils";
import type { Device, Service } from "@/types";

export interface ServiceGroupData {
  device?: Device;
  ip: string;
  services: Service[];
  scanState: "idle" | "queued" | "scanning" | "scanned" | "updating";
  scanPercent?: number | null;
}

export function ServiceGroup({
  group,
  expanded,
  onToggle,
  devices,
  favicons,
  isFavorite,
  canOpenLoopbackServices = true,
  onFavorite,
  onKillProcess,
}: {
  group: ServiceGroupData;
  expanded: boolean;
  onToggle: () => void;
  devices: Device[];
  favicons: Record<string, string | null>;
  isFavorite: (service: Service) => boolean;
  canOpenLoopbackServices?: boolean;
  onFavorite: (service: Service) => void;
  onKillProcess: (service: Service) => Promise<void>;
}) {
  const scanBadge =
    group.scanState === "scanning"
      ? {
          label: "Scanning",
          variant: "warning" as const,
          loading: true,
        }
      : group.scanState === "queued"
        ? {
            label: "Queued",
            variant: "muted" as const,
            loading: false,
          }
        : group.scanState === "scanned"
          ? {
              label: "Scanned",
              variant: "success" as const,
              loading: false,
            }
          : group.scanState === "updating"
            ? {
                label: "Updating",
              variant: "warning" as const,
              loading: true,
            }
            : null;
  const scanPercent =
    typeof group.scanPercent === "number" ? group.scanPercent : null;
  const progressFillClass =
    group.scanState === "scanned"
      ? "bg-success/10"
      : group.scanState === "scanning" || group.scanState === "updating"
        ? "bg-warning/15"
        : "bg-muted/60";

  return (
    <Card className="overflow-hidden p-0">
      <div className="flex items-center gap-2 px-2 py-1.5">
        <button
          type="button"
          onClick={onToggle}
          aria-expanded={expanded}
          className="relative flex min-w-0 flex-1 items-center gap-3 overflow-hidden rounded-lg px-2 py-2 text-left outline-none transition-colors hover:bg-accent/40 focus-visible:ring-2 focus-visible:ring-ring"
        >
          {scanPercent !== null ? (
            <span
              className="pointer-events-none absolute inset-1 overflow-hidden rounded-sm"
              aria-hidden="true"
            >
              <span
                className={cn(
                  "absolute inset-y-0 left-0 rounded-[inherit] transition-[width]",
                  progressFillClass
                )}
                style={{ width: `${scanPercent}%` }}
              />
            </span>
          ) : null}
          <ChevronDown
            className={cn(
              "relative z-10 size-4 shrink-0 text-muted-foreground transition-transform",
              expanded ? "rotate-0" : "-rotate-90"
            )}
          />
          <div className="relative z-10 min-w-0 flex-1">
            <p className="truncate text-sm font-semibold">
              {group.device ? deviceName(group.device) : group.ip}
            </p>
            <p className="truncate text-xs text-muted-foreground">{group.ip}</p>
          </div>
          {scanBadge ? (
            <Badge
              variant={scanBadge.variant}
              className="relative z-10 shrink-0 px-1.5 text-[10px] tabular-nums sm:px-2.5 sm:text-xs"
            >
              {scanBadge.loading ? (
                <Loader2 className="size-3 animate-spin" />
              ) : null}
              <span>{scanBadge.label}</span>
              {scanPercent !== null ? <span>{scanPercent}%</span> : null}
            </Badge>
          ) : null}
          <span className="relative z-10 ml-auto inline-flex shrink-0 items-center gap-1.5 rounded-full bg-muted/70 px-2.5 py-1 text-xs font-semibold text-muted-foreground tabular-nums">
            <Globe2 className="size-3.5" />
            {group.services.length}
          </span>
        </button>
      </div>
      {expanded ? (
        <>
          <Separator />
          <div className="flex flex-col gap-0.5 p-1.5">
            {group.services.length > 0 ? (
              group.services.map((service) => (
                <ServiceRow
                  key={service.id}
                  service={service}
                  devices={devices}
                  favicon={favicons[serviceOrigin(service)]}
                  favorite={isFavorite(service)}
                  canOpenLoopbackServices={canOpenLoopbackServices}
                  onFavorite={onFavorite}
                  onKillProcess={onKillProcess}
                />
              ))
            ) : (
              <div className="rounded-lg px-3 py-4 text-sm text-muted-foreground">
                {group.scanState === "scanning"
                  ? "Looking for web services on this device..."
                  : "No web services found yet."}
              </div>
            )}
          </div>
        </>
      ) : null}
    </Card>
  );
}
