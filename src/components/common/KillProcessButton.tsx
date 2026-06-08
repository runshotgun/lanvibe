import { useState } from "react";
import { Loader2, Power } from "lucide-react";

import { ConfirmationDialog } from "@/components/common/ConfirmationDialog";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { serviceProcessOwner } from "@/lib/finder";
import { cn } from "@/lib/utils";
import type { Service } from "@/types";

export function canKillServiceProcess({
  service,
  processOwner,
  canOpenLoopbackServices = true,
  onKillProcess,
}: {
  service: Service;
  processOwner?: string | null;
  canOpenLoopbackServices?: boolean;
  onKillProcess?: (service: Service) => Promise<void>;
}) {
  const owner = processOwner ?? serviceProcessOwner(service);
  return Boolean(
    onKillProcess &&
      service.active &&
      canOpenLoopbackServices &&
      owner &&
      !isProtectedProcessOwner(owner)
  );
}

export function KillProcessButton({
  service,
  processOwner = serviceProcessOwner(service),
  canOpenLoopbackServices = true,
  compact = false,
  className,
  onKillProcess,
}: {
  service: Service;
  processOwner?: string | null;
  canOpenLoopbackServices?: boolean;
  compact?: boolean;
  className?: string;
  onKillProcess?: (service: Service) => Promise<void>;
}) {
  const [confirmOpen, setConfirmOpen] = useState(false);
  const [killing, setKilling] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const canKill = canKillServiceProcess({
    service,
    processOwner,
    canOpenLoopbackServices,
    onKillProcess,
  });

  if (!canKill || !onKillProcess || !processOwner) return null;

  const handleConfirm = async () => {
    setKilling(true);
    setError(null);
    try {
      await onKillProcess(service);
      setConfirmOpen(false);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setKilling(false);
    }
  };

  return (
    <>
      <Tooltip>
        <TooltipTrigger asChild>
          <button
            type="button"
            disabled={killing}
            onClick={() => {
              setError(null);
              setConfirmOpen(true);
            }}
            aria-label={`Kill ${processOwner}`}
            className={cn(
              "grid shrink-0 place-items-center rounded-lg border border-transparent text-muted-foreground transition-colors hover:bg-destructive/10 hover:text-destructive focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:pointer-events-none disabled:opacity-55",
              compact ? "size-8" : "size-9",
              className
            )}
          >
            {killing ? (
              <Loader2 className="size-4 animate-spin" />
            ) : (
              <Power className="size-4" />
            )}
          </button>
        </TooltipTrigger>
        <TooltipContent>Kill process</TooltipContent>
      </Tooltip>

      <ConfirmationDialog
        open={confirmOpen}
        destructive
        busy={killing}
        title="Kill process?"
        description={
          <>
            <span className="font-medium text-foreground">{processOwner}</span>{" "}
            owns port{" "}
            <span className="font-mono tabular-nums text-foreground">
              {service.port}
            </span>
            . This will stop the service immediately.
          </>
        }
        confirmLabel={killing ? "Killing..." : "Kill process"}
        error={error}
        onOpenChange={(open) => {
          if (killing) return;
          if (open) setError(null);
          setConfirmOpen(open);
        }}
        onConfirm={handleConfirm}
      />
    </>
  );
}

function isProtectedProcessOwner(processOwner: string) {
  const pid = processOwner.match(/\bPID (\d+)\b/)?.[1];
  return pid ? Number(pid) <= 4 : false;
}
