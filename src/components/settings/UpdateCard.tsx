import { useState } from "react";
import { Check, Download, Loader2, RefreshCw, RotateCcw } from "lucide-react";
import type { DownloadEvent, Update } from "@tauri-apps/plugin-updater";

import { inTauri } from "@/api";
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Button } from "@/components/ui/button";

type UpdateState =
  | "idle"
  | "checking"
  | "available"
  | "downloading"
  | "installed"
  | "current"
  | "unavailable"
  | "error";

export function UpdateCard() {
  const [state, setState] = useState<UpdateState>("idle");
  const [message, setMessage] = useState(
    "Check GitHub Releases for a signed LANVibe update."
  );
  const [update, setUpdate] = useState<Update | null>(null);
  const [progress, setProgress] = useState<number | null>(null);

  async function checkForUpdates() {
    if (!inTauri()) {
      setState("unavailable");
      setMessage("Updates are available in the installed desktop app.");
      return;
    }

    try {
      setState("checking");
      setMessage("Checking for updates...");
      setProgress(null);

      const { check } = await import("@tauri-apps/plugin-updater");
      const nextUpdate = await check();

      if (!nextUpdate) {
        setUpdate(null);
        setState("current");
        setMessage("LANVibe is up to date.");
        return;
      }

      setUpdate(nextUpdate);
      setState("available");
      setMessage(`Version ${nextUpdate.version} is ready to install.`);
    } catch (error) {
      setState("error");
      setMessage(error instanceof Error ? error.message : "Update check failed.");
    }
  }

  async function installUpdate() {
    if (!update) return;

    let downloaded = 0;
    let total: number | undefined;

    function onDownload(event: DownloadEvent) {
      if (event.event === "Started") {
        downloaded = 0;
        total = event.data.contentLength;
        setProgress(total ? 0 : null);
        setMessage("Downloading update...");
      }

      if (event.event === "Progress") {
        downloaded += event.data.chunkLength;
        if (total) setProgress(Math.round((downloaded / total) * 100));
      }

      if (event.event === "Finished") {
        setProgress(100);
        setMessage("Installing update...");
      }
    }

    try {
      setState("downloading");
      await update.downloadAndInstall(onDownload);
      setState("installed");
      setMessage("Update installed. Restart LANVibe to finish.");
    } catch (error) {
      setState("error");
      setMessage(error instanceof Error ? error.message : "Update install failed.");
    }
  }

  async function restartApp() {
    try {
      const { relaunch } = await import("@tauri-apps/plugin-process");
      await relaunch();
    } catch (error) {
      setState("error");
      setMessage(error instanceof Error ? error.message : "Restart failed.");
    }
  }

  const busy = state === "checking" || state === "downloading";

  return (
    <Card>
      <CardHeader className="p-3 pb-2 sm:p-4 sm:pb-2">
        <CardTitle>
          {busy ? (
            <Loader2 className="size-4 animate-spin text-primary" />
          ) : state === "current" || state === "installed" ? (
            <Check className="size-4 text-success" />
          ) : (
            <RefreshCw className="size-4 text-primary" />
          )}
          Updates
        </CardTitle>
      </CardHeader>
      <CardContent className="flex flex-col gap-3 p-3 pt-0 sm:p-4 sm:pt-0">
        <p className="text-sm leading-snug text-muted-foreground">{message}</p>
        {progress !== null ? (
          <div className="h-2 overflow-hidden rounded-full bg-secondary">
            <div
              className="h-full rounded-full bg-primary transition-all"
              style={{ width: `${Math.max(0, Math.min(progress, 100))}%` }}
            />
          </div>
        ) : null}
        <div className="flex flex-wrap gap-2">
          {state === "available" ? (
            <Button onClick={installUpdate} disabled={busy}>
              <Download className="size-4" />
              Install update
            </Button>
          ) : state === "installed" ? (
            <Button onClick={restartApp}>
              <RotateCcw className="size-4" />
              Restart
            </Button>
          ) : (
            <Button onClick={checkForUpdates} disabled={busy}>
              {busy ? (
                <Loader2 className="size-4 animate-spin" />
              ) : (
                <RefreshCw className="size-4" />
              )}
              Check now
            </Button>
          )}
        </div>
      </CardContent>
    </Card>
  );
}
