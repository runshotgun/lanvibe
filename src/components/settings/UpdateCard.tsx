import { useEffect, useMemo, useState } from "react";
import { Check, Download, Loader2, RefreshCw, RotateCcw, TriangleAlert } from "lucide-react";

import { getUpdateStatus, triggerHostUpdate } from "@/api";
import type { UpdateStatus } from "@/types";
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Button } from "@/components/ui/button";

const initialStatus: UpdateStatus = {
  phase: "idle",
  currentVersion: "",
  latestVersion: null,
  downloadedBytes: 0,
  totalBytes: null,
  message: "Check GitHub Releases for a signed LANVibe update.",
  startedAt: null,
  finishedAt: null,
};

const activePhases = new Set(["checking", "downloading", "installing", "restarting"]);

export function UpdateCard() {
  const [status, setStatus] = useState<UpdateStatus>(initialStatus);
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    let cancelled = false;

    void getUpdateStatus()
      .then((next) => {
        if (!cancelled) setStatus(next);
      })
      .catch(() => {
        if (!cancelled) {
          setStatus((current) => ({
            ...current,
            phase: "error",
            message: "Update status is unavailable from this dashboard.",
          }));
        }
      });

    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    if (!activePhases.has(status.phase)) return;

    const timer = window.setInterval(() => {
      void getUpdateStatus()
        .then(setStatus)
        .catch(() => {
          if (status.phase === "restarting" || status.phase === "installing") {
            setStatus((current) => ({
              ...current,
              phase: "restarting",
              message: "LANVibe is restarting. This dashboard will reconnect shortly.",
            }));
          }
        });
    }, 2_000);

    return () => window.clearInterval(timer);
  }, [status.phase]);

  const progress = useMemo(() => {
    if (!status.totalBytes || status.totalBytes <= 0) return null;
    return Math.round((status.downloadedBytes / status.totalBytes) * 100);
  }, [status.downloadedBytes, status.totalBytes]);

  async function checkAndInstall() {
    try {
      setLoading(true);
      const next = await triggerHostUpdate();
      setStatus(next);
    } catch (error) {
      setStatus((current) => ({
        ...current,
        phase: "error",
        message: error instanceof Error ? error.message : "Update check failed.",
      }));
    } finally {
      setLoading(false);
    }
  }

  const busy = loading || activePhases.has(status.phase);
  const icon = status.phase === "error" ? (
    <TriangleAlert className="size-4 text-destructive" />
  ) : status.phase === "current" ? (
    <Check className="size-4 text-success" />
  ) : status.phase === "restarting" ? (
    <RotateCcw className="size-4 text-primary" />
  ) : busy ? (
    <Loader2 className="size-4 animate-spin text-primary" />
  ) : (
    <RefreshCw className="size-4 text-primary" />
  );

  return (
    <Card>
      <CardHeader className="p-3 pb-2 sm:p-4 sm:pb-2">
        <CardTitle>
          {icon}
          Updates
        </CardTitle>
      </CardHeader>
      <CardContent className="flex flex-col gap-3 p-3 pt-0 sm:p-4 sm:pt-0">
        <div className="space-y-1">
          <p className="text-sm leading-snug text-muted-foreground">{status.message}</p>
          <p className="text-xs text-muted-foreground">
            Current {status.currentVersion || "unknown"}
            {status.latestVersion ? ` · Latest ${status.latestVersion}` : ""}
          </p>
        </div>
        {progress !== null ? (
          <div className="h-2 overflow-hidden rounded-full bg-secondary">
            <div
              className="h-full rounded-full bg-primary transition-all"
              style={{ width: `${Math.max(0, Math.min(progress, 100))}%` }}
            />
          </div>
        ) : null}
        <div className="flex flex-wrap gap-2">
          <Button onClick={checkAndInstall} disabled={busy}>
            {busy ? (
              <Loader2 className="size-4 animate-spin" />
            ) : status.phase === "current" ? (
              <RefreshCw className="size-4" />
            ) : (
              <Download className="size-4" />
            )}
            Check for updates
          </Button>
        </div>
      </CardContent>
    </Card>
  );
}
