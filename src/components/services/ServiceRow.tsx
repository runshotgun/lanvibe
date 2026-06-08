import { type MouseEvent } from "react";
import { ExternalLink, LockKeyhole, Star } from "lucide-react";

import { inTauri, openService } from "@/api";
import { KillProcessButton } from "@/components/common/KillProcessButton";
import { ProcessOwnerBadge } from "@/components/common/ProcessOwnerBadge";
import { ServiceFavicon } from "@/components/common/ServiceFavicon";
import { StatusDot } from "@/components/common/StatusDot";
import { Badge } from "@/components/ui/badge";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import {
  formatTime,
  hasPageTitle,
  serviceHostName,
  serviceLabel,
  serviceProcessOwner,
} from "@/lib/finder";
import {
  isLoopbackService,
  isServiceUnavailableFromHere,
  serviceOpenBlockReason,
} from "@/lib/service-access";
import { cn } from "@/lib/utils";
import type { Device, Service } from "@/types";

export function ServiceRow({
  service,
  devices,
  favicon,
  favorite,
  canOpenLoopbackServices = true,
  onFavorite,
  onKillProcess,
}: {
  service: Service;
  devices: Device[];
  favicon?: string | null;
  favorite: boolean;
  canOpenLoopbackServices?: boolean;
  onFavorite: (service: Service) => void;
  onKillProcess?: (service: Service) => Promise<void>;
}) {
  const titled = hasPageTitle(service);
  const processOwner = serviceProcessOwner(service);
  const localOnly = isLoopbackService(service);
  const unavailableFromHere = isServiceUnavailableFromHere(
    service,
    canOpenLoopbackServices
  );
  const openBlockReason = serviceOpenBlockReason(
    service,
    canOpenLoopbackServices
  );
  const openBlocked = openBlockReason !== null;
  const handleOpen = (event: MouseEvent<HTMLAnchorElement>) => {
    if (openBlocked) {
      event.preventDefault();
      return;
    }

    if (!inTauri()) return;
    event.preventDefault();
    void openService(service.url);
  };

  return (
    <div
      className={cn(
        "group flex items-center gap-2 rounded-xl px-2 transition-colors hover:bg-accent/40",
        unavailableFromHere && "opacity-55 hover:bg-transparent"
      )}
    >
      <a
        href={openBlocked ? undefined : service.url}
        target="_blank"
        rel="noopener noreferrer external"
        onClick={handleOpen}
        aria-disabled={openBlocked}
        title={openBlockReason ?? undefined}
        className={cn(
          "flex min-w-0 flex-1 items-center gap-3 rounded-lg py-2.5 text-left outline-none focus-visible:ring-2 focus-visible:ring-ring",
          openBlocked && "cursor-not-allowed opacity-65"
        )}
      >
        <StatusDot active={service.active} />
        <ServiceFavicon url={favicon} />
        <span className="flex min-w-0 flex-1 flex-col">
          <span className="flex items-center gap-1.5 truncate text-sm font-semibold text-foreground">
            {titled ? service.title : serviceLabel(service, devices)}
            {service.scheme === "https" ? (
              <Badge variant="outline" className="px-1.5 py-0 text-[10px]">
                TLS
              </Badge>
            ) : null}
          </span>
          <span className="flex min-w-0 flex-col gap-1 text-xs text-muted-foreground sm:flex-row sm:items-center sm:gap-1.5">
            <span className="flex min-w-0 items-center gap-1.5">
              <span className="truncate">
                {serviceHostName(service, devices)}
              </span>
              <Badge
                variant="secondary"
                className="shrink-0 px-1.5 py-0 font-mono text-[10px] tabular-nums"
              >
                {service.port}
              </Badge>
              {localOnly ? (
                <Badge
                  variant="warning"
                  className="shrink-0 px-1.5 py-0 text-[10px]"
                >
                  <LockKeyhole className="size-3" />
                  Local only
                </Badge>
              ) : null}
            </span>
            <ProcessOwnerBadge
              owner={processOwner}
              className="w-fit max-w-full sm:max-w-[13rem] sm:shrink"
            />
          </span>
        </span>
        <span className="hidden shrink-0 flex-col items-end gap-0.5 sm:flex">
          <Badge variant={service.active ? "success" : "muted"}>
            {service.active ? "Active" : "Inactive"}
          </Badge>
          <span className="text-[11px] text-muted-foreground">
            {formatTime(service.lastSeen)}
          </span>
        </span>
      </a>
      {!unavailableFromHere ? (
        <span className="flex shrink-0 items-center gap-1">
          <KillProcessButton
            service={service}
            processOwner={processOwner}
            canOpenLoopbackServices={canOpenLoopbackServices}
            onKillProcess={onKillProcess}
          />
          {!openBlocked ? (
            <Tooltip>
              <TooltipTrigger asChild>
                <a
                  href={service.url}
                  target="_blank"
                  rel="noopener noreferrer external"
                  onClick={handleOpen}
                  aria-label="Open service"
                  className="grid size-9 shrink-0 place-items-center rounded-lg border border-transparent text-muted-foreground transition-colors hover:bg-accent hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                >
                  <ExternalLink className="size-4" />
                </a>
              </TooltipTrigger>
              <TooltipContent>Open service</TooltipContent>
            </Tooltip>
          ) : null}
          <Tooltip>
            <TooltipTrigger asChild>
              <button
                type="button"
                onClick={() => onFavorite(service)}
                aria-label={favorite ? "Remove favorite" : "Add favorite"}
                className={cn(
                  "grid size-9 shrink-0 place-items-center rounded-lg border transition-colors",
                  favorite
                    ? "border-warning/40 bg-warning/15 text-warning"
                    : "border-transparent text-muted-foreground hover:bg-accent hover:text-foreground"
                )}
              >
                <Star
                  className="size-4"
                  fill={favorite ? "currentColor" : "none"}
                />
              </button>
            </TooltipTrigger>
            <TooltipContent>
              {favorite ? "Remove favorite" : "Add favorite"}
            </TooltipContent>
          </Tooltip>
        </span>
      ) : null}
    </div>
  );
}
