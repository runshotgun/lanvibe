import { useMemo, useState } from "react";
import { Globe2, Loader2 } from "lucide-react";

import { EmptyState } from "@/components/common/EmptyState";
import {
  ServiceGroup,
  type ServiceGroupData,
} from "@/components/services/ServiceGroup";
import { ServicesToolbar } from "@/components/services/ServicesToolbar";
import { Card } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import { useFavicons } from "@/hooks/useFavicons";
import {
  compareIp,
  compareServices,
  serviceLabel,
  serviceOrigin,
} from "@/lib/finder";
import type { Device, ScanStatus, Service } from "@/types";

export function ServicesView({
  devices,
  services,
  loading,
  scanning,
  scanStatus,
  onScan,
  isFavorite,
  onFavorite,
}: {
  devices: Device[];
  services: Service[];
  loading: boolean;
  scanning: boolean;
  scanStatus: ScanStatus;
  onScan: () => void;
  isFavorite: (service: Service) => boolean;
  onFavorite: (service: Service) => void;
}) {
  const [query, setQuery] = useState("");
  const [expandedGroups, setExpandedGroups] = useState<Set<string>>(
    () => new Set()
  );

  const filteredServices = useMemo(() => {
    const lower = query.trim().toLowerCase();
    return services
      .filter((service) => {
        if (!lower) return true;
        const haystack = [
          service.url,
          service.title,
          service.server,
          service.ip,
          service.port.toString(),
          serviceLabel(service, devices),
        ]
          .filter(Boolean)
          .join(" ")
          .toLowerCase();
        return haystack.includes(lower);
      })
      .sort((a, b) => compareServices(a, b, devices));
  }, [devices, query, services]);

  const groups = useMemo<ServiceGroupData[]>(() => {
    const map = new Map<string, ServiceGroupData>();
    for (const service of filteredServices) {
      const device = devices.find((item) => item.id === service.deviceId);
      const key = device?.id ?? service.ip;
      const group = map.get(key) ?? { device, ip: service.ip, services: [] };
      group.services.push(service);
      map.set(key, group);
    }
    return [...map.values()]
      .map((group) => ({
        ...group,
        services: group.services.sort((a, b) => compareServices(a, b, devices)),
      }))
      .sort((a, b) => compareIp(a.ip, b.ip));
  }, [devices, filteredServices]);

  const visibleServices = useMemo(
    () =>
      groups.flatMap((group) => {
        const key = group.device?.id ?? group.ip;
        return expandedGroups.has(key) ? group.services : [];
      }),
    [expandedGroups, groups]
  );

  const origins = useMemo(
    () => visibleServices.map(serviceOrigin),
    [visibleServices]
  );
  const favicons = useFavicons(origins);

  function toggleGroup(key: string) {
    setExpandedGroups((current) => {
      const next = new Set(current);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  }

  return (
    <div className="flex flex-col gap-4">
      <ServicesToolbar
        query={query}
        onQueryChange={setQuery}
        onScan={onScan}
        scanning={scanning}
        scanStatus={scanStatus}
      />

      {loading && services.length === 0 ? (
        <div className="flex flex-col gap-3">
          {[0, 1, 2].map((index) => (
            <Card key={index} className="flex items-center gap-3 p-4">
              <Skeleton className="size-9 rounded-lg" />
              <div className="flex-1 space-y-2">
                <Skeleton className="h-4 w-1/3" />
                <Skeleton className="h-3 w-1/2" />
              </div>
            </Card>
          ))}
        </div>
      ) : null}

      {!loading && filteredServices.length === 0 ? (
        <Card>
          <EmptyState
            icon={<Globe2 />}
            title={query ? "No matching services" : "No web UIs found yet"}
            body={
              query
                ? "Try a different search term."
                : "Select devices on the Devices tab, then run a scan."
            }
          />
        </Card>
      ) : null}

      {groups.length > 0 ? (
        <div className="flex flex-col gap-3">
          {groups.map((group) => {
            const key = group.device?.id ?? group.ip;
            return (
              <ServiceGroup
                key={key}
                group={group}
                expanded={expandedGroups.has(key)}
                onToggle={() => toggleGroup(key)}
                devices={devices}
                favicons={favicons}
                isFavorite={isFavorite}
                onFavorite={onFavorite}
              />
            );
          })}
        </div>
      ) : null}

      {scanning ? (
        <p className="flex items-center justify-center gap-2 py-2 text-xs text-muted-foreground">
          <Loader2 className="size-3.5 animate-spin" />
          {scanStatus.phase === "updating"
            ? "Updating discovered services"
            : "Scanning selected devices"}
        </p>
      ) : null}
    </div>
  );
}
