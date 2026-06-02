import { ChevronDown, Globe2 } from "lucide-react";

import { ServiceRow } from "@/components/services/ServiceRow";
import { Card } from "@/components/ui/card";
import { Separator } from "@/components/ui/separator";
import { deviceName, serviceOrigin } from "@/lib/finder";
import { cn } from "@/lib/utils";
import type { Device, Service } from "@/types";

export interface ServiceGroupData {
  device?: Device;
  ip: string;
  services: Service[];
}

export function ServiceGroup({
  group,
  expanded,
  onToggle,
  devices,
  favicons,
  isFavorite,
  onFavorite,
}: {
  group: ServiceGroupData;
  expanded: boolean;
  onToggle: () => void;
  devices: Device[];
  favicons: Record<string, string | null>;
  isFavorite: (service: Service) => boolean;
  onFavorite: (service: Service) => void;
}) {
  return (
    <Card className="overflow-hidden p-0">
      <div className="flex items-center gap-2 px-2 py-1.5">
        <button
          type="button"
          onClick={onToggle}
          aria-expanded={expanded}
          className="flex min-w-0 flex-1 items-center gap-3 rounded-lg px-2 py-2 text-left outline-none transition-colors hover:bg-accent/40 focus-visible:ring-2 focus-visible:ring-ring"
        >
          <ChevronDown
            className={cn(
              "size-4 shrink-0 text-muted-foreground transition-transform",
              expanded ? "rotate-0" : "-rotate-90"
            )}
          />
          <div className="min-w-0 flex-1">
            <p className="truncate text-sm font-semibold">
              {group.device ? deviceName(group.device) : group.ip}
            </p>
            <p className="truncate text-xs text-muted-foreground">{group.ip}</p>
          </div>
          <span className="ml-auto inline-flex shrink-0 items-center gap-1.5 rounded-full bg-muted/70 px-2.5 py-1 text-xs font-semibold text-muted-foreground tabular-nums">
            <Globe2 className="size-3.5" />
            {group.services.length}
          </span>
        </button>
      </div>
      {expanded ? (
        <>
          <Separator />
          <div className="flex flex-col gap-0.5 p-1.5">
            {group.services.map((service) => (
              <ServiceRow
                key={service.id}
                service={service}
                devices={devices}
                favicon={favicons[serviceOrigin(service)]}
                favorite={isFavorite(service)}
                onFavorite={onFavorite}
              />
            ))}
          </div>
        </>
      ) : null}
    </Card>
  );
}
