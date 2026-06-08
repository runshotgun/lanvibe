import { type MouseEvent, type ReactNode } from "react";
import { LockKeyhole } from "lucide-react";

import { inTauri } from "@/api";
import {
  canKillServiceProcess,
  KillProcessButton,
} from "@/components/common/KillProcessButton";
import { ProcessOwnerBadge } from "@/components/common/ProcessOwnerBadge";
import { ServiceFavicon } from "@/components/common/ServiceFavicon";
import { StatusDot } from "@/components/common/StatusDot";
import { Badge } from "@/components/ui/badge";
import { serviceHostName, serviceLabel, serviceProcessOwner } from "@/lib/finder";
import {
  isLoopbackService,
  serviceOpenBlockReason,
} from "@/lib/service-access";
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
  canOpenLoopbackServices = true,
  onOpen,
  onKillProcess,
}: {
  service: Service;
  devices: Device[];
  favicon?: string | null;
  compact?: boolean;
  loading?: boolean;
  editing?: boolean;
  canOpenLoopbackServices?: boolean;
  onOpen: (service: Service) => void;
  onKillProcess?: (service: Service) => Promise<void>;
}) {
  const primary = service.title?.trim() || serviceHostName(service, devices);
  const secondary = serviceLabel(service, devices);
  const processOwner = serviceProcessOwner(service);
  const localOnly = isLoopbackService(service);
  const openBlockReason = serviceOpenBlockReason(
    service,
    canOpenLoopbackServices
  );
  const openBlocked = openBlockReason !== null;
  const showKillProcess =
    !editing &&
    !loading &&
    canKillServiceProcess({
      service,
      processOwner,
      canOpenLoopbackServices,
      onKillProcess,
    });
  const killAction = showKillProcess ? (
    <KillProcessButton
      service={service}
      processOwner={processOwner}
      canOpenLoopbackServices={canOpenLoopbackServices}
      compact={compact}
      className="pointer-events-auto"
      onKillProcess={onKillProcess}
    />
  ) : null;

  const containerClass = cn(
    "group tactile flex flex-col rounded-xl text-left focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-1 focus-visible:ring-offset-background",
    compact ? "gap-2 p-2.5" : "gap-2 p-2.5 sm:gap-3 sm:p-4",
    openBlocked && "cursor-not-allowed opacity-55"
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
          processOwner={processOwner}
          active={service.active}
          loading={loading}
          localOnly={localOnly}
          compact={compact}
          action={null}
        />
      </div>
    );
  }

  const handleOpen = (event: MouseEvent<HTMLAnchorElement>) => {
    if (openBlocked) {
      event.preventDefault();
      return;
    }

    if (!inTauri()) return;
    event.preventDefault();
    onOpen(service);
  };

  if (killAction) {
    return (
      <div className={cn(containerClass, "relative")}>
        <a
          href={openBlocked ? undefined : service.url}
          target="_blank"
          rel="noopener noreferrer external"
          onClick={handleOpen}
          aria-disabled={openBlocked}
          title={openBlockReason ?? undefined}
          className="absolute inset-0 z-0 rounded-xl outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-1 focus-visible:ring-offset-background"
        >
          <span className="sr-only">Open {primary}</span>
        </a>
        <div
          className={cn(
            "pointer-events-none relative z-10 flex min-h-full flex-col",
            compact ? "gap-2" : "gap-2 sm:gap-3"
          )}
        >
          <TileContent
            favicon={favicon}
            primary={primary}
            secondary={secondary}
            processOwner={processOwner}
            active={service.active}
            loading={loading}
            localOnly={localOnly}
            compact={compact}
            action={killAction}
          />
        </div>
      </div>
    );
  }

  return (
    <a
      href={openBlocked ? undefined : service.url}
      target="_blank"
      rel="noopener noreferrer external"
      onClick={handleOpen}
      aria-disabled={openBlocked}
      title={openBlockReason ?? undefined}
      className={containerClass}
    >
      <TileContent
        favicon={favicon}
        primary={primary}
        secondary={secondary}
        processOwner={processOwner}
        active={service.active}
        loading={loading}
        localOnly={localOnly}
        compact={compact}
        action={null}
      />
    </a>
  );
}

function TileContent({
  favicon,
  primary,
  secondary,
  processOwner,
  active,
  loading,
  localOnly,
  compact,
  action,
}: {
  favicon?: string | null;
  primary: ReactNode;
  secondary: ReactNode;
  processOwner: string | null;
  active: boolean;
  loading: boolean;
  localOnly: boolean;
  compact: boolean;
  action: ReactNode;
}) {
  const stateIndicator = localOnly ? (
    <Badge variant="warning" className="px-1.5 py-0 text-[10px]">
      <LockKeyhole className="size-3" />
      Local
    </Badge>
  ) : (
    <StatusDot active={active} loading={loading} />
  );

  return (
    <>
      <div className="flex items-start gap-2">
        <ServiceFavicon url={favicon} />
        <span
          className={cn(
            "ml-auto flex items-center gap-1",
            action && (compact ? "h-8" : "h-9")
          )}
        >
          {action ? (
            <span
              className={cn(
                "grid shrink-0 place-items-center",
                compact ? "size-8" : "size-9"
              )}
            >
              {stateIndicator}
            </span>
          ) : (
            stateIndicator
          )}
          {action}
        </span>
      </div>
      <div className="min-w-0">
        <div className="truncate text-sm font-semibold leading-tight">{primary}</div>
        <div className="truncate text-xs text-muted-foreground tabular-nums">{secondary}</div>
        <ProcessOwnerBadge owner={processOwner} className="mt-1 max-w-full" />
      </div>
    </>
  );
}
