import { useMemo, useState } from "react";
import { Globe2 } from "lucide-react";

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
  deviceName,
  serviceLabel,
  serviceOrigin,
  serviceProcessOwner,
} from "@/lib/finder";
import type { Device, ScanStatus, Service } from "@/types";

type GroupScanState = ServiceGroupData["scanState"];

function deviceMatchesQuery(device: Device, query: string): boolean {
  if (!query) return true;
  return [
    deviceName(device),
    device.ip,
    device.hostname,
    device.mac,
    device.vendor,
  ]
    .filter(Boolean)
    .join(" ")
    .toLowerCase()
    .includes(query);
}

function scanStateForDevice(
  device: Device | undefined,
  selectedDevices: Device[],
  scanStatus: ScanStatus
): GroupScanState {
  if (!device) return "idle";
  if (scanStatus.phase === "updating") return "updating";
  if (scanStatus.phase !== "scanning" && scanStatus.phase !== "starting") {
    return "idle";
  }

  if (scanStatus.currentDeviceIp === device.ip) return "scanning";

  const index = selectedDevices.findIndex((item) => item.id === device.id);
  if (index < 0) return "idle";
  if (index < scanStatus.scannedDevices) return "scanned";
  return "queued";
}

function scanPercentForDevice(
  device: Device | undefined,
  selectedDevices: Device[],
  scanStatus: ScanStatus
): number | null {
  if (!device) return null;
  if (
    scanStatus.phase !== "starting" &&
    scanStatus.phase !== "scanning" &&
    scanStatus.phase !== "updating"
  ) {
    return null;
  }

  if (scanStatus.phase === "updating") return 100;

  if (scanStatus.currentDeviceIp === device.ip) {
    const totalPorts = scanStatus.currentDeviceTotalPorts ?? 0;
    if (totalPorts <= 0) return 0;
    return Math.min(
      100,
      Math.round(((scanStatus.currentDeviceScannedPorts ?? 0) / totalPorts) * 100)
    );
  }

  const index = selectedDevices.findIndex((item) => item.id === device.id);
  if (index < 0) return null;
  return index < scanStatus.scannedDevices ? 100 : 0;
}

export function ServicesView({
  devices,
  services,
  loading,
  scanning,
  scanStatus,
  onScan,
  isFavorite,
  onFavorite,
  onKillProcess,
  canOpenLoopbackServices,
}: {
  devices: Device[];
  services: Service[];
  loading: boolean;
  scanning: boolean;
  scanStatus: ScanStatus;
  onScan: () => void;
  isFavorite: (service: Service) => boolean;
  onFavorite: (service: Service) => void;
  onKillProcess: (service: Service) => Promise<void>;
  canOpenLoopbackServices: boolean;
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
          serviceProcessOwner(service),
          serviceLabel(service, devices),
        ]
          .filter(Boolean)
          .join(" ")
          .toLowerCase();
        return haystack.includes(lower);
      })
      .sort((a, b) => compareServices(a, b, devices));
  }, [devices, query, services]);

  const selectedDevices = useMemo(
    () =>
      devices
        .filter((device) => device.selected && !device.ignored)
        .sort((a, b) => compareIp(a.ip, b.ip)),
    [devices]
  );
  const scanningActive =
    scanning ||
    scanStatus.phase === "starting" ||
    scanStatus.phase === "scanning" ||
    scanStatus.phase === "updating";

  const groups = useMemo<ServiceGroupData[]>(() => {
    const lower = query.trim().toLowerCase();
    const map = new Map<string, ServiceGroupData>();

    for (const device of selectedDevices) {
      if (!deviceMatchesQuery(device, lower)) continue;
      map.set(device.id, {
        device,
        ip: device.ip,
        services: [],
        scanState: scanStateForDevice(device, selectedDevices, scanStatus),
        scanPercent: scanPercentForDevice(device, selectedDevices, scanStatus),
      });
    }

    for (const service of filteredServices) {
      const device = devices.find((item) => item.id === service.deviceId);
      const key = device?.id ?? service.ip;
      const group = map.get(key) ?? {
        device,
        ip: device?.ip ?? service.ip,
        services: [],
        scanState: scanStateForDevice(device, selectedDevices, scanStatus),
        scanPercent: scanPercentForDevice(device, selectedDevices, scanStatus),
      };
      group.services.push(service);
      map.set(key, group);
    }
    return [...map.values()]
      .map((group) => ({
        ...group,
        services: group.services.sort((a, b) => compareServices(a, b, devices)),
      }))
      .sort((a, b) => compareIp(a.ip, b.ip));
  }, [devices, filteredServices, query, scanStatus, selectedDevices]);

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
        scanning={scanningActive}
        scanStatus={scanStatus}
        selectedDeviceCount={selectedDevices.length}
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

      {!loading && groups.length === 0 ? (
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
                canOpenLoopbackServices={canOpenLoopbackServices}
                onFavorite={onFavorite}
                onKillProcess={onKillProcess}
              />
            );
          })}
        </div>
      ) : null}
    </div>
  );
}
