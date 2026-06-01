import { useMemo, useState } from "react";
import { faMagnifyingGlass } from "@fortawesome/free-solid-svg-icons";
import { FontAwesomeIcon } from "@fortawesome/react-fontawesome";
import { Loader2, RefreshCw, Wifi } from "lucide-react";

import { EmptyState } from "@/components/common/EmptyState";
import { DeviceRow } from "@/components/devices/DeviceRow";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { compareIp, deviceName } from "@/lib/finder";
import type { Device } from "@/types";

export function DevicesView({
  devices,
  discovering,
  onDiscover,
  onToggle,
  onRename,
}: {
  devices: Device[];
  discovering: boolean;
  onDiscover: () => void;
  onToggle: (device: Device, selected: boolean) => void;
  onRename: (device: Device, name: string | null) => void;
}) {
  const [query, setQuery] = useState("");
  const selectedCount = devices.filter((device) => device.selected).length;
  const sorted = useMemo(() => {
    const lower = query.trim().toLowerCase();
    return devices
      .filter((device) => {
        if (!lower) return true;
        const haystack = [
          deviceName(device),
          device.ip,
          device.mac,
          device.vendor,
          device.source,
        ]
          .filter(Boolean)
          .join(" ")
          .toLowerCase();
        return haystack.includes(lower);
      })
      .sort((a, b) => compareIp(a.ip, b.ip));
  }, [devices, query]);

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-wrap items-center gap-2">
        <div className="relative min-w-44 flex-1">
          <FontAwesomeIcon
            icon={faMagnifyingGlass}
            className="pointer-events-none absolute left-3 top-1/2 z-10 size-4 -translate-y-1/2 text-current/60"
          />
          <Input
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="Search devices"
            className="pl-9"
            aria-label="Search devices"
          />
        </div>
        <Button onClick={onDiscover} disabled={discovering}>
          {discovering ? <Loader2 className="animate-spin" /> : <RefreshCw />}
          Discover
        </Button>
      </div>
      <p className="-mt-2 text-sm text-muted-foreground">
        {selectedCount} of {devices.length} selected for scanning
      </p>

      {sorted.length === 0 ? (
        <Card>
          <EmptyState
            icon={<Wifi />}
            title={query ? "No matching devices" : "No LAN devices discovered"}
            body={
              query
                ? "Try a different search term."
                : "Run discovery to scan your local network for devices."
            }
          />
        </Card>
      ) : (
        <Card className="overflow-hidden p-0">
          <CardContent className="flex flex-col gap-0.5 p-1.5">
            {sorted.map((device) => (
              <DeviceRow
                key={device.id}
                device={device}
                onToggle={onToggle}
                onRename={onRename}
              />
            ))}
          </CardContent>
        </Card>
      )}
    </div>
  );
}
