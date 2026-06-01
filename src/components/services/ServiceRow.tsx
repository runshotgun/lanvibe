import { type MouseEvent } from "react";
import { ExternalLink, Star } from "lucide-react";

import { inTauri, openService } from "@/api";
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
} from "@/lib/finder";
import { cn } from "@/lib/utils";
import type { Device, Service } from "@/types";

export function ServiceRow({
  service,
  devices,
  favorite,
  onFavorite,
}: {
  service: Service;
  devices: Device[];
  favorite: boolean;
  onFavorite: (service: Service) => void;
}) {
  const titled = hasPageTitle(service);
  const handleOpen = (event: MouseEvent<HTMLAnchorElement>) => {
    if (!inTauri()) return;
    event.preventDefault();
    void openService(service.url);
  };

  return (
    <div className="group flex items-center gap-2 rounded-xl px-2 transition-colors hover:bg-accent/40">
      <a
        href={service.url}
        target="_blank"
        rel="noopener noreferrer external"
        onClick={handleOpen}
        className="flex min-w-0 flex-1 items-center gap-3 rounded-lg py-2.5 text-left outline-none focus-visible:ring-2 focus-visible:ring-ring"
      >
        <StatusDot active={service.active} />
        <Badge variant="secondary" className="font-mono tabular-nums">
          {service.port}
        </Badge>
        <span className="flex min-w-0 flex-1 flex-col">
          <span className="flex items-center gap-1.5 truncate text-sm font-semibold text-foreground">
            {titled ? service.title : serviceLabel(service, devices)}
            {service.scheme === "https" ? (
              <Badge variant="outline" className="px-1.5 py-0 text-[10px]">
                TLS
              </Badge>
            ) : null}
            {!titled ? (
              <Badge variant="muted" className="px-1.5 py-0 text-[10px]">
                No page title
              </Badge>
            ) : null}
          </span>
          <span className="truncate text-xs text-muted-foreground">
            {serviceHostName(service, devices)} ▸ {service.url}
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
        <ExternalLink className="size-4 shrink-0 text-muted-foreground transition-colors group-hover:text-foreground" />
      </a>
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
    </div>
  );
}
