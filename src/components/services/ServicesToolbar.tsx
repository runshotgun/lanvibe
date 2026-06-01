import { faMagnifyingGlass } from "@fortawesome/free-solid-svg-icons";
import { FontAwesomeIcon } from "@fortawesome/react-fontawesome";
import { Loader2, RefreshCw } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import type { ScanStatus } from "@/types";

export function ServicesToolbar({
  query,
  onQueryChange,
  onScan,
  scanning,
  scanStatus,
}: {
  query: string;
  onQueryChange: (value: string) => void;
  onScan: () => void;
  scanning: boolean;
  scanStatus: ScanStatus;
}) {
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
        <FontAwesomeIcon
          icon={faMagnifyingGlass}
          className="pointer-events-none absolute left-3 top-1/2 z-10 size-4 -translate-y-1/2 text-muted-foreground"
        />
        <Input
          value={query}
          onChange={(event) => onQueryChange(event.target.value)}
          placeholder="Search URL, host, title, port"
          className="pl-9"
          aria-label="Search services"
        />
      </div>
      <Button onClick={onScan} disabled={scanning}>
        {scanning ? <Loader2 className="animate-spin" /> : <RefreshCw />}
        {scanLabel}
      </Button>
    </div>
  );
}
