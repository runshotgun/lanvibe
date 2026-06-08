import { Loader2, RefreshCw, Search } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import type { ScanStatus } from "@/types";

export function ServicesToolbar({
  query,
  onQueryChange,
  onScan,
  scanning,
  scanStatus,
  selectedDeviceCount,
}: {
  query: string;
  onQueryChange: (value: string) => void;
  onScan: () => void;
  scanning: boolean;
  scanStatus: ScanStatus;
  selectedDeviceCount: number;
}) {
  const totalSelected = scanStatus.selectedDevices || selectedDeviceCount;
  const currentDeviceFraction =
    scanStatus.currentDeviceTotalPorts && scanStatus.currentDeviceTotalPorts > 0
      ? (scanStatus.currentDeviceScannedPorts ?? 0) /
        scanStatus.currentDeviceTotalPorts
      : 0;
  const scanPercent =
    scanning && totalSelected > 0
      ? Math.min(
          100,
          Math.round(
            ((scanStatus.scannedDevices + currentDeviceFraction) /
              totalSelected) *
              100
          )
        )
      : null;
  const scanLabel =
    scanStatus.phase === "starting"
      ? "Starting"
      : scanStatus.phase === "scanning"
        ? "Scanning"
        : scanStatus.phase === "updating"
          ? "Updating"
          : scanStatus.finishedAt
            ? "Scan again"
            : "Scan";

  return (
    <div className="flex flex-wrap items-center gap-2">
      <div className="relative min-w-44 flex-1">
        <Search className="pointer-events-none absolute left-3 top-1/2 z-10 size-4 -translate-y-1/2 text-muted-foreground" />
        <Input
          value={query}
          onChange={(event) => onQueryChange(event.target.value)}
          placeholder="Search services"
          className="pl-9"
          aria-label="Search services by URL, host, title, port, or process"
        />
      </div>
      <Button
        onClick={onScan}
        disabled={scanning}
        aria-label={
          scanPercent !== null ? `${scanLabel} ${scanPercent}%` : scanLabel
        }
        className={scanPercent !== null ? "relative overflow-hidden" : undefined}
      >
        {scanPercent !== null ? (
          <span
            className="absolute inset-1 overflow-hidden rounded-full"
            aria-hidden="true"
          >
            <span
              className="absolute inset-y-0 left-0 rounded-[inherit] bg-primary-foreground/25 transition-[width]"
              style={{ width: `${scanPercent}%` }}
            />
          </span>
        ) : null}
        <span className="relative z-10 inline-flex items-center gap-2">
          {scanning ? <Loader2 className="animate-spin" /> : <RefreshCw />}
          {scanLabel}
        </span>
      </Button>
    </div>
  );
}
